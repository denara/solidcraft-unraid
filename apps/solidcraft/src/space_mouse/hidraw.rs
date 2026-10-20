//! Finding a 3D mouse on Linux. The kernel exposes every HID interface as `/dev/hidrawN`, with
//! its report descriptor and ids under `/sys/class/hidraw/hidrawN/device/`. Reading them is plain
//! file I/O, so no udev or hidapi library is needed.

use std::path::{Path, PathBuf};

use solidcraft_space_mouse::{Descriptor, Layout};

const SYSFS: &str = "/sys/class/hidraw";
const DEV: &str = "/dev";

pub struct Device {
    pub path: PathBuf,
    pub layout: Layout,
    /// Its vendor and product ids, when the kernel gives them.
    pub ids: Option<(u16, u16)>,
}

/// The first hidraw interface that is a multi-axis controller with motion axes, whoever made it.
pub fn find() -> Option<Device> {
    let mut names: Vec<_> = std::fs::read_dir(SYSFS).ok()?.filter_map(Result::ok).map(|entry| entry.file_name()).collect();
    names.sort();
    names.into_iter().find_map(|name| probe(&Path::new(SYSFS).join(&name), Path::new(DEV).join(&name)))
}

fn probe(sysfs: &Path, path: PathBuf) -> Option<Device> {
    let descriptor = Descriptor::parse(&std::fs::read(sysfs.join("device/report_descriptor")).ok()?);
    // A controller that declares no motion axis (buttons only, say) isn't read.
    let layout = descriptor.is_multi_axis_controller().then(|| descriptor.layout()).flatten()?;
    let ids = std::fs::read_to_string(sysfs.join("device/uevent")).ok().as_deref().and_then(ids);
    Some(Device { path, layout, ids })
}

/// The vendor and product ids in a HID device's uevent: `HID_ID=bus:vendor:product`, in hex.
fn ids(uevent: &str) -> Option<(u16, u16)> {
    let id = uevent.lines().find_map(|line| line.strip_prefix("HID_ID="))?;
    let mut hex = id.split(':').skip(1).map(|part| u32::from_str_radix(part, 16).ok().and_then(|v| u16::try_from(v).ok()));
    Some((hex.next()??, hex.next()??))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_come_from_the_uevent() {
        let wireless = "DRIVER=hid-generic\nHID_ID=0005:0000256F:0000C63A\nHID_NAME=SpaceMouse Wireless BT\n";
        assert_eq!(ids(wireless), Some((0x256f, 0xc63a)));
        assert_eq!(ids("HID_ID=0003:0000046D\n"), None);
        assert_eq!(ids("HID_ID=0003:0001046D:0000C62B\n"), None, "a vendor past 16 bits isn't a USB id");
        assert_eq!(ids("HID_NAME=keyboard\n"), None);
    }
}
