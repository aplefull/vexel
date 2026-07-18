use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

fn unpack_segment(data: &[u8], output: &mut Vec<u8>, expected: usize) {
    let mut pos = 0;

    while pos < data.len() && output.len() < expected {
        let n = data[pos];
        pos += 1;

        if n <= 127 {
            let count = (n as usize + 1).min(expected - output.len());
            let available = (data.len() - pos).min(n as usize + 1);
            output.extend_from_slice(&data[pos..pos + available.min(count)]);
            pos += n as usize + 1;
        } else if n >= 129 {
            if pos >= data.len() {
                break;
            }
            let count = (257 - n as usize).min(expected - output.len());
            let byte = data[pos];
            pos += 1;
            output.resize(output.len() + count, byte);
        }
    }
}

pub fn decode_rle_frame(
    data: &[u8],
    pixels_per_frame: usize,
    samples_per_pixel: usize,
    bytes_per_sample: usize,
) -> VexelResult<Vec<u8>> {
    if data.len() < 64 {
        return Err(VexelError::Custom("DICOM RLE: fragment shorter than 64-byte header".to_string()));
    }

    let read_u32 = |i: usize| u32::from_le_bytes([data[i * 4], data[i * 4 + 1], data[i * 4 + 2], data[i * 4 + 3]]);

    let mut num_segments = read_u32(0) as usize;
    let expected_segments = samples_per_pixel * bytes_per_sample;

    if num_segments > 15 {
        log_warn!("DICOM RLE: invalid segment count {}, clamping to 15", num_segments);
        num_segments = 15;
    }

    if num_segments != expected_segments {
        log_warn!(
            "DICOM RLE: segment count {} does not match samples {} x bytes {}",
            num_segments,
            samples_per_pixel,
            bytes_per_sample
        );
    }

    let mut output = vec![0u8; pixels_per_frame * samples_per_pixel * bytes_per_sample];
    let mut segment = Vec::with_capacity(pixels_per_frame);

    for seg_index in 0..num_segments.min(expected_segments) {
        let start = read_u32(seg_index + 1) as usize;
        let end = if seg_index + 1 < num_segments {
            (read_u32(seg_index + 2) as usize).clamp(start, data.len())
        } else {
            data.len()
        };

        if start >= data.len() || start < 64 {
            log_warn!("DICOM RLE: segment {} offset {} out of bounds", seg_index, start);
            continue;
        }

        segment.clear();
        unpack_segment(&data[start..end], &mut segment, pixels_per_frame);

        if segment.len() < pixels_per_frame {
            log_warn!(
                "DICOM RLE: segment {} decoded {} of {} bytes, padding",
                seg_index,
                segment.len(),
                pixels_per_frame
            );
            segment.resize(pixels_per_frame, 0);
        }

        let sample = seg_index / bytes_per_sample;
        let byte_index = seg_index % bytes_per_sample;
        let dst_offset = sample * bytes_per_sample + (bytes_per_sample - 1 - byte_index);
        let stride = samples_per_pixel * bytes_per_sample;

        for (pixel, &value) in segment.iter().enumerate() {
            output[pixel * stride + dst_offset] = value;
        }
    }

    Ok(output)
}
