//! Finding a 3D mouse on Linux. The kernel exposes every HID interface as `/dev/hidrawN`, with
//! its identity and report descriptor under `/sys/class/hidraw/hidrawN/device/`. Reading both is
//! plain file I/O, so no udev or hidapi library is needed.

use std::path::{Path, PathBuf};

use super::hid_descriptor::{Descriptor, Layout};

const SYSFS: &str = "/sys/class/hidraw";
const DEV: &str = "/dev";

pub struct Device {
    pub path: PathBuf,
    pub layout: Layout,
}

/// The vendor id from `HID_ID=<bus>:<vendor>:<product>` (all hexadecimal) in a sysfs `uevent`.
fn vendor_id(uevent: &str) -> Option<u16> {
    let id = uevent.lines().find_map(|line| line.strip_prefix("HID_ID="))?;
    let vendor = id.split(':').nth(1)?;
    u32::from_str_radix(vendor.trim(), 16).ok().and_then(|vendor| u16::try_from(vendor).ok())
}

/// The first hidraw interface that is a 3D mouse's multi-axis controller.
pub fn find() -> Option<Device> {
    let mut names: Vec<_> = std::fs::read_dir(SYSFS).ok()?.filter_map(Result::ok).map(|entry| entry.file_name()).collect();
    names.sort();
    names.into_iter().find_map(|name| probe(&Path::new(SYSFS).join(&name), Path::new(DEV).join(&name)))
}

fn probe(sysfs: &Path, path: PathBuf) -> Option<Device> {
    let uevent = std::fs::read_to_string(sysfs.join("device/uevent")).ok()?;
    if !super::is_3d_mouse_vendor(vendor_id(&uevent)?) {
        return None;
    }
    let descriptor = Descriptor::parse(&std::fs::read(sysfs.join("device/report_descriptor")).ok()?);
    descriptor.is_multi_axis_controller().then(|| Device { path, layout: descriptor.layout() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_vendor_from_the_hid_id() {
        let uevent = "DRIVER=hid-generic\nHID_ID=0003:0000256F:0000C631\nHID_NAME=Some Device\n";
        assert_eq!(vendor_id(uevent), Some(0x256f));
        assert_eq!(vendor_id("HID_ID=0003:0000046D:0000C077"), Some(0x046d));
    }

    #[test]
    fn malformed_ids_are_rejected() {
        for uevent in ["", "HID_NAME=x", "HID_ID=", "HID_ID=0003", "HID_ID=0003:zzzz:0001", "HID_ID=0003:FFFFFFFF:0001"] {
            assert_eq!(vendor_id(uevent), None, "{uevent:?}");
        }
    }
}
