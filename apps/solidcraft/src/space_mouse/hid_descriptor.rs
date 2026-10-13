//! Just enough of a HID report descriptor reader to recognise a 3D mouse and to see how it packs
//! its motion axes. Written from the public USB HID specification. Malformed input never panics;
//! it simply yields less information.

/// Generic Desktop usage page, and the usages in it that matter here.
const PAGE_GENERIC_DESKTOP: u32 = 0x01;
const USAGE_MULTI_AXIS_CONTROLLER: u32 = 0x08;
const FIRST_AXIS_USAGE: u32 = 0x30; // X, Y, Z, Rx, Ry, Rz are 0x30..=0x35
const AXIS_COUNT: u32 = 6;

/// Motion reports are report id 1 (translation, or everything) and report id 2 (rotation).
const MOTION_REPORT_ID: u8 = 1;

/// Descriptors are small (hidapi reserves 4 KiB). Anything past this is ignored.
const MAX_DESCRIPTOR_BYTES: usize = 4096;
/// Bound on the usages one main item can declare through a usage range.
const MAX_USAGES_PER_ITEM: u32 = 256;

/// How a device spreads its six motion axes over input reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// Report 1 carries translation and report 2 carries rotation, three 16-bit values each.
    /// This is how 3Dconnexion devices report, and it is the layout assumed when nothing is known.
    #[default]
    Split,
    /// Report 1 carries all six axes: translation, then rotation. Homemade devices do this.
    Combined,
}

#[derive(Clone, Copy)]
struct Item {
    /// 0 main, 1 global, 2 local (3 is reserved).
    kind: u8,
    tag: u8,
    value: u32,
}

/// Walks the short items of a descriptor. Long items are skipped; a truncated tail ends the walk.
fn items(descriptor: &[u8]) -> impl Iterator<Item = Item> + '_ {
    let descriptor = descriptor.get(..MAX_DESCRIPTOR_BYTES).unwrap_or(descriptor);
    let mut pos = 0_usize;
    std::iter::from_fn(move || {
        loop {
            let prefix = *descriptor.get(pos)?;
            pos += 1;
            if prefix == 0xFE {
                // Long item: a data length byte, a tag byte, then the data.
                let length = usize::from(*descriptor.get(pos)?);
                pos = pos.checked_add(2 + length)?;
                continue;
            }
            let size = match prefix & 0b11 {
                0 => 0,
                1 => 1,
                2 => 2,
                _ => 4,
            };
            let data = descriptor.get(pos..pos + size)?;
            pos += size;
            let value = data.iter().enumerate().fold(0_u32, |value, (index, byte)| value | (u32::from(*byte) << (8 * index)));
            return Some(Item { kind: (prefix >> 2) & 0b11, tag: prefix >> 4, value });
        }
    })
}

/// What a descriptor says about a device, as far as 3D mouse support needs to know.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Descriptor {
    multi_axis_controller: bool,
    /// Distinct axes (X, Y, Z, Rx, Ry, Rz as bits 0..=5) that input report 1 carries.
    motion_report_axes: u8,
}

impl Descriptor {
    pub fn parse(descriptor: &[u8]) -> Self {
        let mut summary = Self::default();
        let mut page = 0_u32;
        let mut report_id = 0_u8;
        let mut depth = 0_u32;
        // Local items belong to the next main item only.
        let mut usages: Vec<u32> = Vec::new();
        let mut usage_min = None;
        for item in items(descriptor) {
            match (item.kind, item.tag) {
                // Global: usage page, report id.
                (1, 0) => page = item.value,
                (1, 8) => report_id = u8::try_from(item.value).unwrap_or(0),
                // Local: usage, usage minimum, usage maximum.
                (2, 0) => usages.push(Self::full_usage(page, item)),
                (2, 1) => usage_min = Some(Self::full_usage(page, item)),
                (2, 2) => {
                    if let Some(min) = usage_min.take() {
                        let max = Self::full_usage(page, item);
                        let count = max.saturating_sub(min).min(MAX_USAGES_PER_ITEM - 1);
                        usages.extend((0..=count).map(|offset| min.saturating_add(offset)));
                    }
                }
                // Main: input, collection, end collection.
                (0, 8) if report_id == MOTION_REPORT_ID || (report_id == 0 && depth > 0) => summary.note_axes(&usages),
                (0, 0xA) => {
                    // An application collection (1) at the top level names the device.
                    if depth == 0 && item.value == 1 && usages.first() == Some(&((PAGE_GENERIC_DESKTOP << 16) | USAGE_MULTI_AXIS_CONTROLLER)) {
                        summary.multi_axis_controller = true;
                    }
                    depth += 1;
                }
                (0, 0xC) => depth = depth.saturating_sub(1),
                _ => {}
            }
            if item.kind == 0 {
                usages.clear();
                usage_min = None;
            }
        }
        summary
    }

    /// Usage items of up to two bytes name a usage on the current page, four bytes carry the page.
    fn full_usage(page: u32, item: Item) -> u32 {
        if item.value > 0xFFFF { item.value } else { (page << 16) | item.value }
    }

    fn note_axes(&mut self, usages: &[u32]) {
        for usage in usages {
            let axis = usage.wrapping_sub((PAGE_GENERIC_DESKTOP << 16) | FIRST_AXIS_USAGE);
            if axis < AXIS_COUNT {
                self.motion_report_axes |= 1 << axis;
            }
        }
    }

    /// True for a top-level Generic Desktop "Multi-Axis Controller" collection. This tells a
    /// SpaceMouse's motion interface apart from its keyboard interface and from ordinary mice.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn is_multi_axis_controller(&self) -> bool {
        self.multi_axis_controller
    }

    /// `Combined` when input report 1 declares all six axes, otherwise `Split`.
    pub fn layout(&self) -> Layout {
        if self.motion_report_axes.count_ones() == AXIS_COUNT { Layout::Combined } else { Layout::Split }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Report 1 = X Y Z, report 2 = Rx Ry Rz, each as three signed 16-bit values.
    const SPLIT: &[u8] = &[
        0x05, 0x01, // usage page: generic desktop
        0x09, 0x08, // usage: multi-axis controller
        0xA1, 0x01, // collection: application
        0x85, 0x01, // report id 1
        0x16, 0xA2, 0xFE, // logical minimum -350
        0x26, 0x5E, 0x01, // logical maximum 350
        0x75, 0x10, // report size 16
        0x95, 0x03, // report count 3
        0x09, 0x30, 0x09, 0x31, 0x09, 0x32, // X Y Z
        0x81, 0x06, // input: data, variable, relative
        0x85, 0x02, // report id 2
        0x09, 0x33, 0x09, 0x34, 0x09, 0x35, // Rx Ry Rz
        0x81, 0x06, // input
        0xC0, // end collection
    ];

    /// Report 1 = X Y Z Rx Ry Rz.
    const COMBINED: &[u8] = &[
        0x05, 0x01, 0x09, 0x08, 0xA1, 0x01, 0x85, 0x01, // multi-axis controller, report id 1
        0x16, 0xA2, 0xFE, 0x26, 0x5E, 0x01, 0x75, 0x10, 0x95, 0x06, // 6 x 16 bit, -350..350
        0x09, 0x30, 0x09, 0x31, 0x09, 0x32, 0x09, 0x33, 0x09, 0x34, 0x09, 0x35, // X Y Z Rx Ry Rz
        0x81, 0x06, 0xC0,
    ];

    /// The same six axes declared as a usage range, with report id 1 after the collection opens.
    const COMBINED_RANGE: &[u8] = &[
        0x05, 0x01, 0x09, 0x08, 0xA1, 0x01, 0x85, 0x01, 0x75, 0x10, 0x95, 0x06, //
        0x19, 0x30, 0x29, 0x35, // usage minimum X, usage maximum Rz
        0x81, 0x06, 0xC0,
    ];

    /// A boot keyboard-like interface: usage page 7 usages must not count as axes.
    const KEYBOARD: &[u8] =
        &[0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0xC0];

    /// An ordinary mouse: Generic Desktop X, Y but top-level usage is Mouse (2).
    const MOUSE: &[u8] = &[
        0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x09, 0x30, 0x09, 0x31, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x02, 0x81,
        0x06, 0xC0, 0xC0,
    ];

    #[test]
    fn split_descriptor_is_a_multi_axis_controller_with_three_axes_in_report_one() {
        let descriptor = Descriptor::parse(SPLIT);
        assert!(descriptor.is_multi_axis_controller());
        assert_eq!(descriptor.motion_report_axes, 0b000_111);
        assert_eq!(descriptor.layout(), Layout::Split);
    }

    #[test]
    fn six_axes_in_report_one_are_a_combined_layout() {
        for bytes in [COMBINED, COMBINED_RANGE] {
            let descriptor = Descriptor::parse(bytes);
            assert!(descriptor.is_multi_axis_controller());
            assert_eq!(descriptor.layout(), Layout::Combined);
        }
    }

    #[test]
    fn other_interfaces_are_not_3d_mice() {
        for bytes in [KEYBOARD, MOUSE, &[][..]] {
            let descriptor = Descriptor::parse(bytes);
            assert!(!descriptor.is_multi_axis_controller());
            assert_eq!(descriptor.layout(), Layout::Split);
        }
    }

    #[test]
    fn four_byte_usages_carry_their_own_page() {
        // Usage (4 bytes) 0x0007_0030 is on page 7, not an axis.
        let descriptor = Descriptor::parse(&[0x05, 0x01, 0x09, 0x08, 0xA1, 0x01, 0x85, 0x01, 0x0B, 0x30, 0x00, 0x07, 0x00, 0x81, 0x06, 0xC0]);
        assert!(descriptor.is_multi_axis_controller());
        assert_eq!(descriptor.motion_report_axes, 0);
    }

    #[test]
    fn truncated_and_hostile_descriptors_never_panic() {
        for end in 0..=COMBINED.len() {
            let _ = Descriptor::parse(&COMBINED[..end]);
        }
        // Long item with a length running past the end, huge usage ranges, deep nesting.
        let _ = Descriptor::parse(&[0xFE, 0xFF, 0x00, 0x01]);
        let _ = Descriptor::parse(&[0x05, 0x01, 0x1B, 0, 0, 0, 0, 0x2B, 0xFF, 0xFF, 0xFF, 0xFF, 0x81, 0x06]);
        let _ = Descriptor::parse(&[0xC0; 64]);
        let _ = Descriptor::parse(&[0xA1; 64]);
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        for _ in 0..200 {
            let bytes: Vec<u8> = (0..64)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state.to_le_bytes()[0]
                })
                .collect();
            let _ = Descriptor::parse(&bytes);
        }
    }
}
