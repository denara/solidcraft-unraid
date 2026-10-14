use super::*;
use crate::lighting::{HdrDecodeTask, HdrTaskPoll};

#[test]
fn cooperative_task_matches_codec_and_is_consumed_once() {
    for bytes in [uncompressed(""), rle(), literal_rle(), old_rle()] {
        let expected = image::load_from_memory_with_format(&bytes, image::ImageFormat::Hdr).unwrap().into_rgb32f();
        let mut task = HdrDecodeTask::new(HdrSource::new(bytes).unwrap()).unwrap();
        let mut calls = 0;
        loop {
            calls += 1;
            assert!(calls < 1000);
            match task.step(1, || false).unwrap() {
                HdrTaskPoll::Pending(progress) => assert!(progress.decoded_pixels < progress.total_pixels),
                HdrTaskPoll::Complete(result) => {
                    assert_eq!(result.pixels().len(), expected.pixels().len());
                    for (actual, expected) in result.pixels().iter().zip(expected.pixels()) {
                        assert_eq!(actual, &expected.0);
                    }
                    break;
                }
            }
        }
        assert!(calls > 8);
        assert_eq!(task.step(1, || false).err().unwrap().code, ErrorCode::Consumed);
    }
}

#[test]
fn cancellation_or_bad_input_cannot_publish_partial_pixels_later() {
    let mut task = HdrDecodeTask::new(HdrSource::new(rle()).unwrap()).unwrap();
    assert!(matches!(task.step(7, || false).unwrap(), HdrTaskPoll::Pending(_)));
    assert_eq!(task.step(1, || true).err().unwrap().code, ErrorCode::Cancelled);
    assert_eq!(task.step(16384, || false).err().unwrap().code, ErrorCode::Consumed);
    let mut bytes = rle();
    bytes.pop();
    let mut task = HdrDecodeTask::new(HdrSource::new(bytes).unwrap()).unwrap();
    assert_eq!(task.step(16384, || false).err().unwrap().code, ErrorCode::InvalidInput);
    assert_eq!(task.step(1, || false).err().unwrap().code, ErrorCode::Consumed);

    let checks = std::cell::Cell::new(0);
    let mut task = HdrDecodeTask::new(HdrSource::new(rle()).unwrap()).unwrap();
    let result = task.step(16384, || {
        checks.set(checks.get() + 1);
        checks.get() == 10
    });
    assert_eq!(result.err().unwrap().code, ErrorCode::Cancelled);
    assert_eq!(checks.get(), 10);
    assert_eq!(task.step(1, || false).err().unwrap().code, ErrorCode::Consumed);
}

#[test]
fn source_primaries_are_preserved_without_assuming_srgb() {
    let source = HdrSource::new(uncompressed("PRIMARIES=0.64 0.33 0.3 0.6 0.15 0.06 0.3127 0.329\n")).unwrap();
    assert_eq!(source.info().primaries, Some([0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]));
    assert!(HdrSource::new(uncompressed("PRIMARIES=NaN 0 0 0 0 0 0 0\n")).is_err());
    let mut excessive_capacity = Vec::with_capacity(MAX_HDR_BYTES + 1);
    excessive_capacity.extend_from_slice(&rle());
    assert_eq!(HdrSource::new(excessive_capacity).err().unwrap().code, ErrorCode::LimitExceeded);
}

fn header(width: u32, height: u32, extra: &str) -> Vec<u8> {
    format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n{extra}\n-Y {height} +X {width}\n").into_bytes()
}
fn uncompressed(extra: &str) -> Vec<u8> {
    let mut bytes = header(4, 2, extra);
    for _ in 0..8 {
        bytes.extend_from_slice(&[128, 64, 32, 131]);
    }
    bytes
}
fn rle() -> Vec<u8> {
    let mut bytes = header(8, 4, "");
    for _ in 0..4 {
        bytes.extend_from_slice(&[2, 2, 0, 8]);
        for value in [128, 64, 32, 129] {
            bytes.extend_from_slice(&[136, value]);
        }
    }
    bytes
}

fn literal_rle() -> Vec<u8> {
    let mut bytes = header(8, 4, "");
    for _ in 0..4 {
        bytes.extend_from_slice(&[2, 2, 0, 8]);
        for value in [128, 64, 32, 129] {
            bytes.push(8);
            bytes.extend_from_slice(&[value; 8]);
        }
    }
    bytes
}

fn old_rle() -> Vec<u8> {
    let mut bytes = header(8, 4, "");
    for _ in 0..4 {
        bytes.extend_from_slice(&[128, 64, 32, 129, 1, 1, 1, 7]);
    }
    bytes
}

#[test]
fn incremental_rows_match_the_original_codec_without_clipping_hdr_values() {
    for bytes in [uncompressed(""), rle(), literal_rle(), old_rle()] {
        let expected = image::load_from_memory_with_format(&bytes, image::ImageFormat::Hdr).unwrap().into_rgb32f();
        let source = HdrSource::new(bytes).unwrap();
        source.validate(|| false).unwrap();
        let mut rows = Rows::new(source).unwrap();
        while !rows.done() {
            if rows.ready() {
                for x in 0..rows.info().width as usize {
                    assert_eq!(rows.linear(x).unwrap(), expected.get_pixel(x as u32, rows.row()).0);
                }
                rows.next_row().unwrap();
            } else {
                rows.step().unwrap();
            }
        }
    }
}

#[test]
fn header_exposure_and_colour_correction_restore_linear_radiance() {
    let source = HdrSource::new(uncompressed("EXPOSURE=2\nCOLORCORR=1 2 4\n")).unwrap();
    let mut rows = Rows::new(source).unwrap();
    while !rows.ready() {
        rows.step().unwrap();
    }
    assert_eq!(rows.linear(0).unwrap(), [2.0, 0.5, 0.125]);
}

#[test]
fn malformed_runs_truncation_nonfinite_metadata_and_hostile_sizes_are_refused() {
    let mut truncated = rle();
    truncated.pop();
    let mut wrong_width = rle();
    let offset = HdrSource::new(wrong_width.clone()).unwrap().info.offset;
    wrong_width[offset + 3] = 9;
    let mut zero_run = rle();
    zero_run[offset + 4] = 0;
    let mut overflow = header(8, 4, "");
    overflow.extend_from_slice(&[128, 64, 32, 129]);
    for _ in 0..6 {
        overflow.extend_from_slice(&[1, 1, 1, 0]);
    }
    for bytes in [truncated, wrong_width, zero_run, overflow] {
        assert!(HdrSource::new(bytes).unwrap().validate(|| false).is_err());
    }
    for bytes in [header(u32::MAX, 1, ""), uncompressed("EXPOSURE=NaN\n"), uncompressed("PIXASPECT=0\n")] {
        assert!(HdrSource::new(bytes).is_err());
    }
    let mut too_large = rle();
    too_large.resize(MAX_HDR_BYTES + 1, 0);
    assert_eq!(HdrSource::new(too_large).err().unwrap().code, ErrorCode::LimitExceeded);
    let mut too_bright = uncompressed("");
    let offset = HdrSource::new(too_bright.clone()).unwrap().info.offset;
    too_bright[offset + 3] = 255;
    assert_eq!(HdrSource::new(too_bright).unwrap().validate(|| false).unwrap_err().code, ErrorCode::LimitExceeded);
    let source = HdrSource::new(rle()).unwrap();

    assert_eq!(source.validate(|| true).unwrap_err().code, ErrorCode::Cancelled);
}

#[test]
fn header_work_is_bounded_before_accepting_metadata() {
    let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n".to_vec();
    bytes.extend(std::iter::repeat_n(b'x', MAX_HEADER_BYTES + 1));
    bytes.extend_from_slice(b"\n\n-Y 2 +X 4\n");
    bytes.extend_from_slice(&[128, 64, 32, 129]);
    assert!(HdrSource::new(bytes).is_err());
}
