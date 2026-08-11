use crate::log_warn;
use crate::utils::lzma::dictionary::Dictionary;
use crate::utils::lzma::lzma1::{Lzma1Decoder, Properties, Status};
use crate::utils::lzma::range_decoder::RangeDecoder;

const DEFAULT_PROPERTIES: Properties = Properties { lc: 3, lp: 0, pb: 2 };

pub fn decode_into(input: &[u8], dict: &mut Dictionary) -> usize {
    let mut decoder = Lzma1Decoder::new(DEFAULT_PROPERTIES);
    let mut properties = DEFAULT_PROPERTIES;
    let mut need_properties = true;
    let mut pos = 0usize;

    while pos < input.len() {
        let control = input[pos];
        pos += 1;

        if control == 0 {
            return pos;
        }

        if control >= 0x80 {
            if pos + 4 > input.len() {
                log_warn!("LZMA2 truncated chunk header");
                return pos;
            }

            let uncompressed_size = ((usize::from(control) & 0x1f) << 16)
                + (usize::from(input[pos]) << 8)
                + usize::from(input[pos + 1])
                + 1;
            let compressed_size = (usize::from(input[pos + 2]) << 8) + usize::from(input[pos + 3]) + 1;
            pos += 4;

            if control >= 0xc0 {
                if pos >= input.len() {
                    log_warn!("LZMA2 truncated properties byte");
                    return pos;
                }

                match Properties::from_byte(input[pos]) {
                    Some(props) => properties = props,
                    None => {
                        log_warn!("LZMA2 invalid properties byte {}, using defaults", input[pos]);
                        properties = DEFAULT_PROPERTIES;
                    }
                }

                pos += 1;
                need_properties = false;
                decoder.reset(properties);
            } else if need_properties {
                log_warn!("LZMA2 chunk before any properties, using defaults");
                need_properties = false;
                decoder.reset(properties);
            } else if control >= 0xa0 {
                decoder.reset(properties);
            }

            let end = (pos + compressed_size).min(input.len());
            if end <= pos {
                log_warn!("LZMA2 empty compressed chunk");
                return pos;
            }

            let mut rc = RangeDecoder::new(&input[pos..end]);
            let status = decoder.decode(&mut rc, dict, Some(uncompressed_size));

            match status {
                Status::OutputFull => return pos,
                Status::Corrupt => {
                    log_warn!("LZMA2 corrupt chunk, keeping {} decoded bytes", dict.len());
                    return pos;
                }
                Status::NeedsInput => {
                    log_warn!("LZMA2 truncated chunk data, keeping {} decoded bytes", dict.len());
                    return pos;
                }
                Status::Finished | Status::EndMarker => {}
            }

            if pos + compressed_size > input.len() {
                log_warn!("LZMA2 chunk extends past end of input");
                return pos;
            }

            pos += compressed_size;
        } else {
            if control > 2 {
                log_warn!("LZMA2 invalid control byte {:#04x}", control);
                return pos;
            }

            if control == 1 {
                need_properties = true;
            }

            if pos + 2 > input.len() {
                log_warn!("LZMA2 truncated uncompressed chunk header");
                return pos;
            }

            let size = (usize::from(input[pos]) << 8) + usize::from(input[pos + 1]) + 1;
            pos += 2;

            let end = (pos + size).min(input.len());
            if end < pos + size {
                log_warn!("LZMA2 truncated uncompressed chunk");
            }

            let written = dict.extend_from_slice(&input[pos..end]);
            if written < end - pos {
                return pos;
            }

            decoder.reset(properties);
            pos = end;
        }
    }

    pos
}
