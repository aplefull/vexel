#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Packet {
    Literal(usize),
    Run(usize),
    Skip,
    End,
}

pub struct RleResult {
    pub consumed: usize,
    pub ended: bool,
}

pub fn decode(
    src: &[u8],
    out: &mut Vec<u8>,
    limit: usize,
    element: usize,
    mut next_packet: impl FnMut(&mut &[u8]) -> Option<Packet>,
) -> RleResult {
    let mut input = src;
    let mut ended = false;

    while out.len() < limit {
        let Some(packet) = next_packet(&mut input) else {
            break;
        };
        match packet {
            Packet::Literal(count) => {
                let wanted = count.saturating_mul(element);
                let available = wanted.min(input.len());
                let writable = available.min(limit - out.len());
                out.extend_from_slice(&input[..writable]);
                input = &input[available..];
                if available < wanted {
                    break;
                }
            }
            Packet::Run(count) => {
                let Some(value) = input.get(..element) else {
                    break;
                };
                input = &input[element..];
                let repeat = count.min((limit - out.len()).div_ceil(element));
                if element == 1 {
                    out.resize(out.len() + repeat, value[0]);
                } else {
                    for _ in 0..repeat {
                        out.extend_from_slice(value);
                    }
                }
            }
            Packet::Skip => {}
            Packet::End => {
                ended = true;
                break;
            }
        }
    }

    out.truncate(limit);
    RleResult { consumed: src.len() - input.len(), ended }
}

pub fn take_byte(input: &mut &[u8]) -> Option<u8> {
    let (&byte, rest) = input.split_first()?;
    *input = rest;
    Some(byte)
}

pub fn packbits_packet(input: &mut &[u8]) -> Option<Packet> {
    Some(match take_byte(input)? {
        n @ 0..=127 => Packet::Literal(n as usize + 1),
        128 => Packet::Skip,
        n => Packet::Run(257 - n as usize),
    })
}

pub fn packbits(src: &[u8], limit: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(limit.min(src.len().saturating_mul(4)));
    decode(src, &mut out, limit, 1, packbits_packet);
    out
}
