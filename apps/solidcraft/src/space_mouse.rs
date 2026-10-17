//! 3Dconnexion HID navigation on Windows (through hidapi) and Linux (through spacenavd when it
//! runs, else hidraw). The device reader never touches the document or UI; it publishes the
//! latest axes, and the egui thread applies them once per frame.

mod hid_descriptor;
#[cfg(target_os = "linux")]
mod hidraw;
#[cfg(target_os = "linux")]
mod spacenavd;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::Context;
use hid_descriptor::Layout;
#[cfg(target_os = "windows")]
use hidapi::{DeviceInfo, HidApi};
use quick_xml::Reader;
use quick_xml::events::Event;
use solidcraft_ui_egui::SolveApp;
use solidcraft_ui_egui::space_mouse::{Axes, AxisRule, DriverMapping, WheelGuard, navigate_frame};

const STALE_AFTER: Duration = Duration::from_millis(150);
const RETRY_AFTER: Duration = Duration::from_secs(2);
const MAX_PROFILE_BYTES: u64 = 1024 * 1024;

/// The mapping used when no 3DxWare profile is found: what the driver applies by default in
/// object mode. On Linux there is no 3DxWare (#1172); on Windows 3DxWare has no profile for
/// SolidCraft until one is made, and the raw axes swap roll and spin (#1380).
fn without_profile() -> DriverMapping {
    DriverMapping::object_mode()
}

#[derive(Clone, Copy, Debug, Default)]
struct ProfileAxis {
    input: Option<usize>,
    output: Option<usize>,
    enabled: bool,
    reversed: bool,
    scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ProfilePart {
    Input,
    Output,
}

fn axis_index(name: &str) -> Option<usize> {
    match name.trim() {
        "HIDMultiAxis_X" => Some(0),
        "HIDMultiAxis_Y" => Some(1),
        "HIDMultiAxis_Z" => Some(2),
        "HIDMultiAxis_Rx" => Some(3),
        "HIDMultiAxis_Ry" => Some(4),
        "HIDMultiAxis_Rz" => Some(5),
        _ => None,
    }
}

/// 3DxWare stores the active KMJ application's axis selection, swapping, inversion and speed in
/// `%APPDATA%/3Dconnexion/3DxWare/Cfg/<app>-KMJ.xml`. Direct HID reads bypass the driver, so apply
/// that profile explicitly when it exists. Unknown or incomplete fields retain the raw HID axis.
fn parse_driver_mapping(xml: &[u8]) -> Option<DriverMapping> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut mapping = DriverMapping::default();
    let mut axis: Option<ProfileAxis> = None;
    let mut part = None;
    let mut field = Vec::new();
    let mut found = false;
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(start) => match start.local_name().as_ref() {
                b"Axis" => axis = Some(ProfileAxis { enabled: true, scale: 1.0, ..Default::default() }),
                b"Input" => part = Some(ProfilePart::Input),
                b"Output" => part = Some(ProfilePart::Output),
                name @ (b"ActionID" | b"Enabled" | b"Reversed" | b"Scale" | b"OverallScale") => field = name.to_vec(),
                _ => {}
            },
            Event::Text(text) => {
                let value = String::from_utf8_lossy(text.as_ref());
                match field.as_slice() {
                    b"ActionID" => {
                        if let Some(current) = axis.as_mut() {
                            match part {
                                Some(ProfilePart::Input) => current.input = axis_index(&value),
                                Some(ProfilePart::Output) => current.output = axis_index(&value),
                                None => {}
                            }
                        }
                    }
                    b"Enabled" => {
                        if let Some(current) = axis.as_mut() {
                            current.enabled = value.trim().eq_ignore_ascii_case("true");
                        }
                    }
                    b"Reversed" => {
                        if part == Some(ProfilePart::Output)
                            && let Some(current) = axis.as_mut()
                        {
                            current.reversed = value.trim().eq_ignore_ascii_case("true");
                        }
                    }
                    b"Scale" => {
                        if part == Some(ProfilePart::Output)
                            && let Some(scale) = value.trim().parse::<f64>().ok().filter(|scale| scale.is_finite())
                            && let Some(current) = axis.as_mut()
                        {
                            current.scale = scale.clamp(0.0, 10.0);
                        }
                    }
                    b"OverallScale" => {
                        if let Some(scale) = value.trim().parse::<f64>().ok().filter(|scale| scale.is_finite()) {
                            mapping.overall_scale = scale.clamp(0.0, 10.0);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(end) => match end.local_name().as_ref() {
                b"Axis" => {
                    if let Some(current) = axis.take()
                        && let (Some(input), Some(output)) = (current.input, current.output)
                    {
                        mapping.axes[input] = AxisRule { output, enabled: current.enabled, reversed: current.reversed, scale: current.scale };
                        found = true;
                    }
                }
                b"Input" | b"Output" => part = None,
                b"ActionID" | b"Enabled" | b"Reversed" | b"Scale" | b"OverallScale" => field.clear(),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    found.then_some(mapping)
}

fn load_driver_mapping() -> Option<DriverMapping> {
    let cfg = std::path::PathBuf::from(std::env::var_os("APPDATA")?).join("3Dconnexion").join("3DxWare").join("Cfg");
    // The driver names the profile after the executable; one made for the pre-rename executable
    // still applies until the driver writes a new one (compatibility shim).
    let profile = ["solidcraft-KMJ.xml", "solvecraft-KMJ.xml"].into_iter().map(|n| cfg.join(n)).find(|p| p.is_file())?;
    if std::fs::metadata(&profile).ok()?.len() > MAX_PROFILE_BYTES {
        log::warn!("3D mouse: ignoring oversized 3DxWare profile {}", profile.display());
        return None;
    }
    let xml = std::fs::read(&profile).ok()?;
    parse_driver_mapping(&xml)
}

#[derive(Clone, Copy)]
struct Reading {
    axes: Axes,
    translation_at: Instant,
    rotation_at: Instant,
    /// The source says when the cap is back at rest, so a reading stays until the next one.
    /// A HID device repeats its report while the cap is held; spacenavd sends a record only when
    /// the values change, and a steady hold can go seconds without one.
    held: bool,
}

impl Reading {
    fn new(now: Instant) -> Self {
        Self { axes: Axes::default(), translation_at: now, rotation_at: now, held: false }
    }

    fn current(self, now: Instant) -> Axes {
        if self.held {
            return self.axes;
        }
        Axes {
            translation: if now.duration_since(self.translation_at) < STALE_AFTER { self.axes.translation } else { [0; 3] },
            rotation: if now.duration_since(self.rotation_at) < STALE_AFTER { self.axes.rotation } else { [0; 3] },
        }
    }
}

/// The old SpaceNavigator uses Logitech's VID; newer 3Dconnexion devices use their own.
fn is_3d_mouse_vendor(vendor_id: u16) -> bool {
    matches!(vendor_id, 0x046d | 0x256f)
}

/// The HID usage excludes ordinary Logitech mice and the keyboard interfaces of SpaceMice.
#[cfg(target_os = "windows")]
fn is_3d_mouse(info: &DeviceInfo) -> bool {
    is_3d_mouse_vendor(info.vendor_id()) && info.usage_page() == 0x01 && info.usage() == 0x08
}

fn three_axes(bytes: &[u8]) -> Option<[i16; 3]> {
    let &[x0, x1, y0, y1, z0, z1] = bytes.get(..6)? else { return None };
    Some([i16::from_le_bytes([x0, x1]), i16::from_le_bytes([y0, y1]), i16::from_le_bytes([z0, z1])])
}

/// SpaceNavigator reports 1 (translation) and 2 (rotation), each with three signed
/// little-endian 16-bit values. The HID layer may pad either report to its maximum size.
/// Some homemade devices send all six values in report 1 instead (`Layout::Combined`): only the
/// device's report descriptor can tell the two apart, because a padded split report 1 is as long.
fn decode_report(report: &[u8], layout: Layout, reading: &mut Reading, now: Instant) -> bool {
    match report.first().copied() {
        Some(1) if layout == Layout::Combined => {
            let Some(body) = report.get(1..) else { return false };
            let (Some(translation), Some(rotation)) = (body.get(..6).and_then(three_axes), body.get(6..).and_then(three_axes)) else { return false };
            reading.axes = Axes { translation, rotation };
            reading.translation_at = now;
            reading.rotation_at = now;
            true
        }
        Some(1) => {
            let Some(translation) = report.get(1..).and_then(three_axes) else { return false };
            reading.axes.translation = translation;
            reading.translation_at = now;
            true
        }
        Some(2) => {
            let Some(rotation) = report.get(1..).and_then(three_axes) else { return false };
            reading.axes.rotation = rotation;
            reading.rotation_at = now;
            true
        }
        _ => false,
    }
}

/// Publishes one input report from the device to the egui thread.
fn publish(ctx: &Context, shared: &Mutex<Reading>, layout: Layout, report: &[u8]) {
    if let Ok(mut reading) = shared.lock()
        && decode_report(report, layout, &mut reading, Instant::now())
    {
        ctx.request_repaint();
    }
}

/// Publishes the cap's position from spacenavd.
#[cfg(target_os = "linux")]
fn publish_held(ctx: &Context, shared: &Mutex<Reading>, axes: Axes) {
    if let Ok(mut reading) = shared.lock() {
        *reading = Reading { axes, held: true, ..Reading::new(Instant::now()) };
        ctx.request_repaint();
    }
}

/// The device is gone: stop reporting its last position.
fn reset(ctx: &Context, shared: &Mutex<Reading>) {
    if let Ok(mut reading) = shared.lock() {
        *reading = Reading::new(Instant::now());
    }
    ctx.request_repaint();
}

#[cfg(target_os = "windows")]
fn reader(ctx: Context, shared: Arc<Mutex<Reading>>) {
    let Ok(mut api) = HidApi::new() else {
        log::warn!("3D mouse: HID initialization failed");
        return;
    };
    loop {
        if let Err(error) = api.refresh_devices() {
            log::warn!("3D mouse: device scan failed: {error}");
        } else if let Some(device) = api.device_list().find(|info| is_3d_mouse(info)).and_then(|info| info.open_device(&api).ok()) {
            let layout = device_layout(&device);
            let mut report = [0_u8; 64];
            loop {
                match device.read_timeout(&mut report, 100) {
                    Ok(0) => {}
                    Ok(size) => publish(&ctx, &shared, layout, report.get(..size).unwrap_or_default()),
                    Err(error) => {
                        log::info!("3D mouse disconnected: {error}");
                        break;
                    }
                }
            }
            reset(&ctx, &shared);
        }
        std::thread::sleep(RETRY_AFTER);
    }
}

/// How the device packs its axes, from its report descriptor; the usual split layout if the
/// descriptor cannot be read.
#[cfg(target_os = "windows")]
fn device_layout(device: &hidapi::HidDevice) -> Layout {
    let mut descriptor = [0_u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
    match device.get_report_descriptor(&mut descriptor) {
        Ok(size) => hid_descriptor::Descriptor::parse(descriptor.get(..size).unwrap_or_default()).layout(),
        Err(error) => {
            log::info!("3D mouse: no report descriptor, assuming separate translation and rotation reports: {error}");
            Layout::Split
        }
    }
}

/// Reads the 3D mouse through spacenavd when it runs, so what `spnavcfg` sets (swapped and inverted
/// axes, sensitivity, dead zones) applies here as in other programs. Without the daemon it reads
/// `/dev/hidrawN` with plain blocking reads, which end with an error when the device is unplugged.
/// Opening needs permission: most distributions grant it to the logged-in user for devices that
/// carry the `uaccess` udev tag, which a rule for the 3D mouse has to add.
#[cfg(target_os = "linux")]
fn reader(ctx: Context, shared: Arc<Mutex<Reading>>) {
    use std::io::Read;
    let mut denied = None;
    loop {
        if let Some(daemon) = spacenavd::connect() {
            log::info!("3D mouse: reading spacenavd, which applies the spnavcfg settings");
            spacenavd::read(daemon, |axes| publish_held(&ctx, &shared, axes));
            reset(&ctx, &shared);
        } else if let Some(device) = hidraw::find() {
            match std::fs::File::open(&device.path) {
                Ok(mut file) => {
                    denied = None;
                    log::info!("3D mouse: reading {} ({:?} reports)", device.path.display(), device.layout);
                    let mut report = [0_u8; 64];
                    loop {
                        match file.read(&mut report) {
                            Ok(0) => break,
                            Ok(size) => publish(&ctx, &shared, device.layout, report.get(..size).unwrap_or_default()),
                            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                            Err(error) => {
                                log::info!("3D mouse disconnected: {error}");
                                break;
                            }
                        }
                    }
                    reset(&ctx, &shared);
                }
                // Say so once per device node, not every retry.
                Err(error) if denied.as_ref() != Some(&device.path) => {
                    log::warn!("3D mouse: cannot open {}: {error}. A udev rule granting the logged-in user access is needed.", device.path.display());
                    denied = Some(device.path);
                }
                Err(_) => {}
            }
        }
        std::thread::sleep(RETRY_AFTER);
    }
}

pub struct SpaceMouse {
    shared: Arc<Mutex<Reading>>,
    last_frame: Instant,
    driver_mapping: DriverMapping,
    mapping_checked_at: Instant,
    /// Drops 3DxWare's emulated wheel while the cap moves (#1380).
    wheel: WheelGuard,
    started: Instant,
}

impl SpaceMouse {
    pub fn start(ctx: &Context) -> Self {
        let now = Instant::now();
        let shared = Arc::new(Mutex::new(Reading::new(now)));
        let worker_state = Arc::clone(&shared);
        let worker_ctx = ctx.clone();
        if let Err(error) = std::thread::Builder::new().name("solidcraft-3d-mouse".into()).spawn(move || reader(worker_ctx, worker_state)) {
            log::warn!("3D mouse: reader could not start: {error}");
        }
        Self {
            shared,
            last_frame: now,
            driver_mapping: load_driver_mapping().unwrap_or_else(without_profile),
            mapping_checked_at: now,
            wheel: WheelGuard::default(),
            started: now,
        }
    }

    /// Before the app's own hook (which adds the control channel's events): with navigation on,
    /// the wheel events 3DxWare makes from the cap's motion are dropped.
    pub fn raw_input_hook(&mut self, enabled: bool, raw: &mut egui::RawInput) {
        self.raw_input_hook_at(enabled, raw, Instant::now());
    }

    fn raw_input_hook_at(&mut self, enabled: bool, raw: &mut egui::RawInput, now: Instant) {
        if !enabled {
            return;
        }
        let moving = self.shared.lock().map(|reading| reading.current(now).moving()).unwrap_or(false);
        self.wheel.filter(moving, now.duration_since(self.started).as_secs_f64(), &mut raw.events);
    }

    pub fn tick(&mut self, app: &mut SolveApp, ctx: &Context) {
        self.tick_at(app, ctx, Instant::now());
    }

    /// The frame at `now`. Tests pass their own clock, so a slow runner can't age a reading.
    fn tick_at(&mut self, app: &mut SolveApp, ctx: &Context, now: Instant) {
        let seconds = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        if now.duration_since(self.mapping_checked_at) >= RETRY_AFTER {
            let updated = load_driver_mapping().unwrap_or_else(without_profile);
            if updated != self.driver_mapping {
                log::info!("3D mouse: applying updated 3DxWare axis profile");
                self.driver_mapping = updated;
            }
            self.mapping_checked_at = now;
        }
        let axes = match self.shared.lock() {
            Ok(reading) => reading.current(now),
            Err(_) => return,
        };
        navigate_frame(app, ctx, self.driver_mapping, axes, seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcraft_ui_egui::space_mouse::{DeviceMotion, Motion};

    /// The logical axes a reading reaches. Logical Rz rolls the view and logical Ry turns it about
    /// its vertical (`navigation::device_camera` in solidcraft-ui-egui, tested there).
    fn rotation(mapping: DriverMapping, rotation: [i16; 3]) -> [f64; 3] {
        mapping.apply(Axes { translation: [0; 3], rotation }).rotation
    }

    #[test]
    fn decodes_separate_reports_without_stale_motion_or_padding_interference() {
        let now = Instant::now();
        let mut reading = Reading::new(now);
        assert!(!decode_report(&[1, 1, 2], Layout::Split, &mut reading, now));
        assert_eq!(reading.current(now), Axes::default());
        assert!(decode_report(&[1, 0x9c, 0xff, 0x2c, 0x01, 0, 0], Layout::Split, &mut reading, now));
        assert_eq!(reading.current(now).translation, [-100, 300, 0]);
        assert!(decode_report(&[2, 0, 0, 1, 0, 0xff, 0xff], Layout::Split, &mut reading, now));
        assert_eq!(reading.current(now).rotation, [0, 1, -1]);
        let later = now + STALE_AFTER + Duration::from_millis(1);
        assert_eq!(reading.current(later), Axes::default());
        assert!(decode_report(&[2, 4, 0, 5, 0, 6, 0], Layout::Split, &mut reading, later));
        assert!(decode_report(&[1, 1, 0, 2, 0, 3, 0, 0, 0, 0, 0, 0, 0], Layout::Split, &mut reading, later));
        assert_eq!(reading.current(later), Axes { translation: [1, 2, 3], rotation: [4, 5, 6] });
    }

    #[test]
    fn a_held_reading_stays_until_the_next_one_and_a_hid_reading_goes_stale() {
        let now = Instant::now();
        let axes = Axes { translation: [0, 200, 0], rotation: [0; 3] };
        let later = now + STALE_AFTER * 20;
        // spacenavd sends nothing while the values stay the same: the push lasts.
        let held = Reading { axes, held: true, ..Reading::new(now) };
        assert_eq!(held.current(later), axes);
        // A HID device repeats its report: silence means the cap, or the device, is gone.
        let hid = Reading { axes, ..Reading::new(now) };
        assert_eq!(hid.current(later), Axes::default());
    }

    #[test]
    fn combined_report_one_carries_translation_and_rotation_together() {
        let now = Instant::now();
        let mut reading = Reading::new(now);
        // Report id 1, then X Y Z Rx Ry Rz as signed little-endian 16-bit values.
        let report = [1, 0x9c, 0xff, 0x2c, 0x01, 0, 0, 1, 0, 0, 0, 0xff, 0xff];
        assert!(decode_report(&report, Layout::Combined, &mut reading, now));
        assert_eq!(reading.current(now), Axes { translation: [-100, 300, 0], rotation: [1, 0, -1] });
        // The same bytes on a split device are only the translation report; its padding is not rotation.
        let mut split = Reading::new(now);
        assert!(decode_report(&report, Layout::Split, &mut split, now));
        assert_eq!(split.current(now), Axes { translation: [-100, 300, 0], rotation: [0; 3] });
    }

    #[test]
    fn combined_layout_ignores_short_and_other_reports() {
        let now = Instant::now();
        let mut reading = Reading::new(now);
        assert!(!decode_report(&[1, 1, 0, 2, 0, 3, 0], Layout::Combined, &mut reading, now));
        assert!(!decode_report(&[3, 1, 0, 0, 0, 0], Layout::Combined, &mut reading, now));
        assert!(!decode_report(&[], Layout::Combined, &mut reading, now));
        assert_eq!(reading.current(now), Axes::default());
        // Extra padding after the six values is fine.
        assert!(decode_report(&[1, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 0, 0], Layout::Combined, &mut reading, now));
        assert_eq!(reading.current(now), Axes { translation: [1, 2, 3], rotation: [4, 5, 6] });
    }

    #[test]
    fn applies_3dxware_axis_swaps_and_reversals() {
        let profile = br#"
            <AppCfg><Devices><Device><AxisBank>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_X</ActionID></Input><Output><ActionID>HIDMultiAxis_X</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Y</ActionID></Input><Output><ActionID>HIDMultiAxis_Z</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Z</ActionID></Input><Output><ActionID>HIDMultiAxis_Y</ActionID><Reversed>true</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Rx</ActionID></Input><Output><ActionID>HIDMultiAxis_Rx</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Ry</ActionID></Input><Output><ActionID>HIDMultiAxis_Rz</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Rz</ActionID></Input><Output><ActionID>HIDMultiAxis_Ry</ActionID><Reversed>true</Reversed></Output></Axis>
            </AxisBank></Device></Devices></AppCfg>
        "#;
        let mapping = parse_driver_mapping(profile).expect("valid 3DxWare profile");
        let motion = mapping.apply(Axes { translation: [350, 0, 350], rotation: [350, 0, 350] });
        assert_eq!(motion.translation, [1.0, -1.0, 0.0]);
        assert_eq!(motion.rotation, [1.0, -1.0, 0.0]);
    }

    #[test]
    fn without_a_profile_the_drivers_object_mode_axes_are_used() {
        // 3DxWare's object-mode mapping, as a profile (#1172: confirmed on a SpaceMouse Compact).
        let object_mode = br#"
            <AppCfg><Devices><Device><AxisBank>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_X</ActionID></Input><Output><ActionID>HIDMultiAxis_X</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Y</ActionID></Input><Output><ActionID>HIDMultiAxis_Y</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Z</ActionID></Input><Output><ActionID>HIDMultiAxis_Z</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Rx</ActionID></Input><Output><ActionID>HIDMultiAxis_Rx</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Ry</ActionID></Input><Output><ActionID>HIDMultiAxis_Rz</ActionID><Reversed>true</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Rz</ActionID></Input><Output><ActionID>HIDMultiAxis_Ry</ActionID><Reversed>true</Reversed></Output></Axis>
            </AxisBank></Device></Devices></AppCfg>
        "#;
        let profile = parse_driver_mapping(object_mode).expect("valid 3DxWare profile");
        // On every platform (#1172 on Linux, #1380 on Windows).
        assert_eq!(without_profile(), profile);
        // Tilting the cap rolls the view and twisting it spins it, as with the driver.
        assert_eq!(rotation(without_profile(), [0, 350, 0]), [0.0, 0.0, -1.0]);
        assert_eq!(rotation(without_profile(), [0, 0, 350]), [0.0, -1.0, 0.0]);
    }

    #[test]
    fn mapped_roll_and_spin_reach_the_roll_and_turn_axes() {
        let profile = br#"
            <AppCfg><Devices><Device><AxisBank>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Ry</ActionID></Input><Output><ActionID>HIDMultiAxis_Rz</ActionID><Reversed>false</Reversed></Output></Axis>
              <Axis><Enabled>true</Enabled><Input><ActionID>HIDMultiAxis_Rz</ActionID></Input><Output><ActionID>HIDMultiAxis_Ry</ActionID><Reversed>true</Reversed></Output></Axis>
            </AxisBank></Device></Devices></AppCfg>
        "#;
        let mapping = parse_driver_mapping(profile).expect("valid 3DxWare profile");
        // The physical roll reaches the roll axis only, and the physical spin the turn only.
        assert_eq!(rotation(mapping, [0, 350, 0]), [0.0, 0.0, 1.0]);
        assert_eq!(rotation(mapping, [0, 0, 350]), [0.0, -1.0, 0.0]);
    }

    #[test]
    fn each_frame_hands_the_view_its_motion_through_the_drivers_mapping() {
        // The reader's latest tilt, and a profile that makes tilt roll: the frame's motion rolls.
        // The frame runs at the reading's own time, so a slow runner can't let the reading go stale.
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), solidcraft_ui_egui::Services::default());
        app.home.open = false;
        let now = Instant::now();
        let mut reading = Reading::new(now);
        reading.axes.rotation = [0, 350, 0];
        let mut mouse = SpaceMouse {
            shared: Arc::new(Mutex::new(reading)),
            last_frame: now - Duration::from_millis(20),
            driver_mapping: DriverMapping::object_mode(),
            mapping_checked_at: now,
            wheel: WheelGuard::default(),
            started: now,
        };
        mouse.tick_at(&mut app, &Context::default(), now);
        let DeviceMotion { motion, seconds } = app.viewport.device_motion.expect("the frame hands the page its motion");
        assert_eq!(motion, Motion { translation: [0.0; 3], rotation: [0.0, 0.0, -1.0] });
        assert!(seconds > 0.0);
    }

    fn wheel() -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -1.0),
            modifiers: Default::default(),
            phase: egui::TouchPhase::Move,
        }
    }

    fn mouse(reading: Reading, now: Instant) -> SpaceMouse {
        SpaceMouse {
            shared: Arc::new(Mutex::new(reading)),
            last_frame: now,
            driver_mapping: DriverMapping::object_mode(),
            mapping_checked_at: now,
            wheel: WheelGuard::default(),
            started: now,
        }
    }

    #[test]
    fn the_wheel_3dxware_makes_from_the_cap_is_dropped() {
        // The cap tilted forward: 3DxWare also sends wheel turns (#1380). They are dropped.
        let now = Instant::now();
        let mut tilted = Reading::new(now);
        tilted.axes.rotation = [-200, 0, 0];
        let mut moving = mouse(tilted, now);
        let mut raw = egui::RawInput { events: vec![wheel(), egui::Event::Zoom(1.1), egui::Event::Text("a".into())], ..Default::default() };
        moving.raw_input_hook_at(true, &mut raw, now);
        assert_eq!(raw.events, [egui::Event::Text("a".into())]);
        // With navigation off, the wheel is the cap's only way to move the view: kept.
        let mut raw = egui::RawInput { events: vec![wheel()], ..Default::default() };
        moving.raw_input_hook_at(false, &mut raw, now);
        assert_eq!(raw.events.len(), 1);
        // The cap at rest: the wheel is the user's.
        let mut resting = mouse(Reading::new(now), now);
        let mut raw = egui::RawInput { events: vec![wheel()], ..Default::default() };
        resting.raw_input_hook_at(true, &mut raw, now);
        assert_eq!(raw.events.len(), 1);
    }

    #[test]
    fn the_wheel_is_the_users_again_once_the_cap_has_been_still_a_moment() {
        let now = Instant::now();
        let mut tilted = Reading::new(now);
        tilted.axes.rotation = [-200, 0, 0];
        let mut cap = mouse(tilted, now);
        let mut raw = egui::RawInput { events: vec![wheel()], ..Default::default() };
        cap.raw_input_hook_at(true, &mut raw, now);
        assert!(raw.events.is_empty(), "dropped while the cap moves");
        // Let go, then wait out the quiet time.
        let later = now + Duration::from_secs_f64(solidcraft_ui_egui::space_mouse::WHEEL_QUIET_S) + Duration::from_millis(60);
        if let Ok(mut reading) = cap.shared.lock() {
            *reading = Reading::new(later);
        }
        let mut raw = egui::RawInput { events: vec![wheel()], ..Default::default() };
        cap.raw_input_hook_at(true, &mut raw, later);
        assert_eq!(raw.events.len(), 1, "kept once the cap has been still");
    }
}
