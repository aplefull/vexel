use crate::decoders::jbig2::bitmap::ComposeOp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentType {
    SymbolDictionary,
    IntermediateTextRegion,
    ImmediateTextRegion,
    ImmediateLosslessTextRegion,
    PatternDictionary,
    IntermediateHalftoneRegion,
    ImmediateHalftoneRegion,
    ImmediateLosslessHalftoneRegion,
    IntermediateGenericRegion,
    ImmediateGenericRegion,
    ImmediateLosslessGenericRegion,
    IntermediateRefinementRegion,
    ImmediateRefinementRegion,
    ImmediateLosslessRefinementRegion,
    PageInfo,
    EndOfPage,
    EndOfStripe,
    EndOfFile,
    Profiles,
    Tables,
    ColorPalette,
    Extension,
    Unknown(u8),
}

impl SegmentType {
    pub fn from_flags(flags: u8) -> SegmentType {
        match flags & 0x3f {
            0 => SegmentType::SymbolDictionary,
            4 => SegmentType::IntermediateTextRegion,
            6 => SegmentType::ImmediateTextRegion,
            7 => SegmentType::ImmediateLosslessTextRegion,
            16 => SegmentType::PatternDictionary,
            20 => SegmentType::IntermediateHalftoneRegion,
            22 => SegmentType::ImmediateHalftoneRegion,
            23 => SegmentType::ImmediateLosslessHalftoneRegion,
            36 => SegmentType::IntermediateGenericRegion,
            38 => SegmentType::ImmediateGenericRegion,
            39 => SegmentType::ImmediateLosslessGenericRegion,
            40 => SegmentType::IntermediateRefinementRegion,
            42 => SegmentType::ImmediateRefinementRegion,
            43 => SegmentType::ImmediateLosslessRefinementRegion,
            48 => SegmentType::PageInfo,
            49 => SegmentType::EndOfPage,
            50 => SegmentType::EndOfStripe,
            51 => SegmentType::EndOfFile,
            52 => SegmentType::Profiles,
            53 => SegmentType::Tables,
            54 => SegmentType::ColorPalette,
            62 => SegmentType::Extension,
            other => SegmentType::Unknown(other),
        }
    }

    pub fn is_intermediate(self) -> bool {
        matches!(
            self,
            SegmentType::IntermediateTextRegion
                | SegmentType::IntermediateHalftoneRegion
                | SegmentType::IntermediateGenericRegion
                | SegmentType::IntermediateRefinementRegion
        )
    }
}

#[derive(Clone, Debug)]
pub struct SegmentHeader {
    pub number: u32,
    pub flags: u8,
    pub segment_type: SegmentType,
    pub retention: Vec<bool>,
    pub referred_to: Vec<u32>,
    pub page_association: u32,
    pub data_length: u32,
    pub header_length: usize,
}

pub fn parse_segment_header(data: &[u8]) -> Option<SegmentHeader> {
    if data.len() < 11 {
        return None;
    }

    let number = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let flags = data[4];
    let segment_type = SegmentType::from_flags(flags);
    let page_association_size = if flags & 0x40 != 0 { 4 } else { 1 };

    let rtscarf = data[5];
    let (referred_count, retention_start, mut offset) = if (rtscarf & 0xe0) == 0xe0 {
        let long = u32::from_be_bytes([data[5], data[6], data[7], data[8]]);
        let count = long & 0x1fff_ffff;
        (count, 9, 5 + 4 + (count as usize + 8) / 8)
    } else {
        ((rtscarf >> 5) as u32, 5, 6)
    };

    let referred_size = if number <= 256 {
        1
    } else if number <= 65536 {
        2
    } else {
        4
    };

    let needed = offset
        .checked_add((referred_count as usize).checked_mul(referred_size)?)?
        .checked_add(page_association_size)?
        .checked_add(4)?;
    if needed > data.len() {
        return None;
    }

    let retention = if retention_start == 5 {
        (0..=referred_count as usize).map(|bit| rtscarf & (1 << bit) != 0).collect()
    } else {
        (0..=referred_count as usize).map(|bit| data[retention_start + bit / 8] & (1 << (bit % 8)) != 0).collect()
    };

    let mut referred_to = Vec::with_capacity(referred_count as usize);
    for _ in 0..referred_count {
        let value = match referred_size {
            1 => data[offset] as u32,
            2 => u16::from_be_bytes([data[offset], data[offset + 1]]) as u32,
            _ => u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]),
        };
        referred_to.push(value);
        offset += referred_size;
    }

    let page_association = if page_association_size == 4 {
        let value = u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
        offset += 4;
        value
    } else {
        let value = data[offset] as u32;
        offset += 1;
        value
    };

    let data_length = u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
    offset += 4;

    Some(SegmentHeader {
        number,
        flags,
        segment_type,
        retention,
        referred_to,
        page_association,
        data_length,
        header_length: offset,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct RegionInfo {
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
    pub flags: u8,
    pub op: ComposeOp,
}

pub fn parse_region_info(data: &[u8]) -> Option<RegionInfo> {
    if data.len() < 17 {
        return None;
    }
    Some(RegionInfo {
        width: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
        height: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
        x: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
        y: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
        flags: data[16],
        op: ComposeOp::from_bits(data[16] & 7),
    })
}
