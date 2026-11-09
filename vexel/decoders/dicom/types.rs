#![allow(dead_code)]

pub const fn tag(group: u16, element: u16) -> u32 {
    ((group as u32) << 16) | element as u32
}

pub const fn tag_group(tag: u32) -> u16 {
    (tag >> 16) as u16
}

pub const fn tag_element(tag: u32) -> u16 {
    (tag & 0xFFFF) as u16
}

pub mod tags {
    use super::tag;

    pub const FILE_META_GROUP_LENGTH: u32 = tag(0x0002, 0x0000);
    pub const TRANSFER_SYNTAX_UID: u32 = tag(0x0002, 0x0010);

    pub const SOP_CLASS_UID: u32 = tag(0x0008, 0x0016);
    pub const MODALITY: u32 = tag(0x0008, 0x0060);
    pub const RECOMMENDED_DISPLAY_FRAME_RATE: u32 = tag(0x0008, 0x2144);

    pub const FRAME_TIME: u32 = tag(0x0018, 0x1063);
    pub const FRAME_TIME_VECTOR: u32 = tag(0x0018, 0x1065);

    pub const SAMPLES_PER_PIXEL: u32 = tag(0x0028, 0x0002);
    pub const PHOTOMETRIC_INTERPRETATION: u32 = tag(0x0028, 0x0004);
    pub const PLANAR_CONFIGURATION: u32 = tag(0x0028, 0x0006);
    pub const NUMBER_OF_FRAMES: u32 = tag(0x0028, 0x0008);
    pub const ROWS: u32 = tag(0x0028, 0x0010);
    pub const COLUMNS: u32 = tag(0x0028, 0x0011);
    pub const BITS_ALLOCATED: u32 = tag(0x0028, 0x0100);
    pub const BITS_STORED: u32 = tag(0x0028, 0x0101);
    pub const HIGH_BIT: u32 = tag(0x0028, 0x0102);
    pub const PIXEL_REPRESENTATION: u32 = tag(0x0028, 0x0103);
    pub const SMALLEST_IMAGE_PIXEL_VALUE: u32 = tag(0x0028, 0x0106);
    pub const LARGEST_IMAGE_PIXEL_VALUE: u32 = tag(0x0028, 0x0107);

    pub const RED_PALETTE_DESCRIPTOR: u32 = tag(0x0028, 0x1101);
    pub const GREEN_PALETTE_DESCRIPTOR: u32 = tag(0x0028, 0x1102);
    pub const BLUE_PALETTE_DESCRIPTOR: u32 = tag(0x0028, 0x1103);
    pub const RED_PALETTE_DATA: u32 = tag(0x0028, 0x1201);
    pub const GREEN_PALETTE_DATA: u32 = tag(0x0028, 0x1202);
    pub const BLUE_PALETTE_DATA: u32 = tag(0x0028, 0x1203);
    pub const SEGMENTED_RED_PALETTE_DATA: u32 = tag(0x0028, 0x1221);
    pub const SEGMENTED_GREEN_PALETTE_DATA: u32 = tag(0x0028, 0x1222);
    pub const SEGMENTED_BLUE_PALETTE_DATA: u32 = tag(0x0028, 0x1223);

    pub const WINDOW_CENTER: u32 = tag(0x0028, 0x1050);
    pub const WINDOW_WIDTH: u32 = tag(0x0028, 0x1051);
    pub const RESCALE_INTERCEPT: u32 = tag(0x0028, 0x1052);
    pub const RESCALE_SLOPE: u32 = tag(0x0028, 0x1053);
    pub const VOI_LUT_FUNCTION: u32 = tag(0x0028, 0x1056);

    pub const MODALITY_LUT_SEQUENCE: u32 = tag(0x0028, 0x3000);
    pub const LUT_DESCRIPTOR: u32 = tag(0x0028, 0x3002);
    pub const LUT_DATA: u32 = tag(0x0028, 0x3006);
    pub const VOI_LUT_SEQUENCE: u32 = tag(0x0028, 0x3010);

    pub const FRAME_VOI_LUT_SEQUENCE: u32 = tag(0x0028, 0x9132);
    pub const PIXEL_VALUE_TRANSFORMATION_SEQUENCE: u32 = tag(0x0028, 0x9145);
    pub const SHARED_FUNCTIONAL_GROUPS_SEQUENCE: u32 = tag(0x5200, 0x9229);
    pub const PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE: u32 = tag(0x5200, 0x9230);

    pub const ICON_IMAGE_SEQUENCE: u32 = tag(0x0088, 0x0200);

    pub const FLOAT_PIXEL_DATA: u32 = tag(0x7FE0, 0x0008);
    pub const DOUBLE_FLOAT_PIXEL_DATA: u32 = tag(0x7FE0, 0x0009);
    pub const PIXEL_DATA: u32 = tag(0x7FE0, 0x0010);

    pub const ITEM: u32 = tag(0xFFFE, 0xE000);
    pub const ITEM_DELIMITER: u32 = tag(0xFFFE, 0xE00D);
    pub const SEQUENCE_DELIMITER: u32 = tag(0xFFFE, 0xE0DD);
}

pub fn dictionary(t: u32) -> Option<(&'static str, &'static str)> {
    let entry = match t {
        0x0002_0000 => ("File Meta Information Group Length", "UL"),
        0x0002_0001 => ("File Meta Information Version", "OB"),
        0x0002_0002 => ("Media Storage SOP Class UID", "UI"),
        0x0002_0003 => ("Media Storage SOP Instance UID", "UI"),
        0x0002_0010 => ("Transfer Syntax UID", "UI"),
        0x0002_0012 => ("Implementation Class UID", "UI"),
        0x0002_0013 => ("Implementation Version Name", "SH"),
        0x0002_0016 => ("Source Application Entity Title", "AE"),
        0x0002_0100 => ("Private Information Creator UID", "UI"),
        0x0002_0102 => ("Private Information", "OB"),
        0x0008_0005 => ("Specific Character Set", "CS"),
        0x0008_0008 => ("Image Type", "CS"),
        0x0008_0012 => ("Instance Creation Date", "DA"),
        0x0008_0013 => ("Instance Creation Time", "TM"),
        0x0008_0014 => ("Instance Creator UID", "UI"),
        0x0008_0016 => ("SOP Class UID", "UI"),
        0x0008_0018 => ("SOP Instance UID", "UI"),
        0x0008_0020 => ("Study Date", "DA"),
        0x0008_0021 => ("Series Date", "DA"),
        0x0008_0022 => ("Acquisition Date", "DA"),
        0x0008_0023 => ("Content Date", "DA"),
        0x0008_002A => ("Acquisition DateTime", "DT"),
        0x0008_0030 => ("Study Time", "TM"),
        0x0008_0031 => ("Series Time", "TM"),
        0x0008_0032 => ("Acquisition Time", "TM"),
        0x0008_0033 => ("Content Time", "TM"),
        0x0008_0050 => ("Accession Number", "SH"),
        0x0008_0060 => ("Modality", "CS"),
        0x0008_0064 => ("Conversion Type", "CS"),
        0x0008_0070 => ("Manufacturer", "LO"),
        0x0008_0080 => ("Institution Name", "LO"),
        0x0008_0090 => ("Referring Physician's Name", "PN"),
        0x0008_0100 => ("Code Value", "SH"),
        0x0008_0102 => ("Coding Scheme Designator", "SH"),
        0x0008_0104 => ("Code Meaning", "LO"),
        0x0008_0201 => ("Timezone Offset From UTC", "SH"),
        0x0008_1010 => ("Station Name", "SH"),
        0x0008_1030 => ("Study Description", "LO"),
        0x0008_103E => ("Series Description", "LO"),
        0x0008_1040 => ("Institutional Department Name", "LO"),
        0x0008_1070 => ("Operators' Name", "PN"),
        0x0008_1090 => ("Manufacturer's Model Name", "LO"),
        0x0008_1111 => ("Referenced Performed Procedure Step Sequence", "SQ"),
        0x0008_1140 => ("Referenced Image Sequence", "SQ"),
        0x0008_1150 => ("Referenced SOP Class UID", "UI"),
        0x0008_1155 => ("Referenced SOP Instance UID", "UI"),
        0x0008_2111 => ("Derivation Description", "ST"),
        0x0008_2112 => ("Source Image Sequence", "SQ"),
        0x0008_2144 => ("Recommended Display Frame Rate", "IS"),
        0x0008_9215 => ("Derivation Code Sequence", "SQ"),
        0x0010_0010 => ("Patient's Name", "PN"),
        0x0010_0020 => ("Patient ID", "LO"),
        0x0010_0030 => ("Patient's Birth Date", "DA"),
        0x0010_0040 => ("Patient's Sex", "CS"),
        0x0010_1010 => ("Patient's Age", "AS"),
        0x0010_1020 => ("Patient's Size", "DS"),
        0x0010_1030 => ("Patient's Weight", "DS"),
        0x0018_0010 => ("Contrast/Bolus Agent", "LO"),
        0x0018_0015 => ("Body Part Examined", "CS"),
        0x0018_0020 => ("Scanning Sequence", "CS"),
        0x0018_0022 => ("Scan Options", "CS"),
        0x0018_0023 => ("MR Acquisition Type", "CS"),
        0x0018_0050 => ("Slice Thickness", "DS"),
        0x0018_0060 => ("KVP", "DS"),
        0x0018_0080 => ("Repetition Time", "DS"),
        0x0018_0081 => ("Echo Time", "DS"),
        0x0018_0082 => ("Inversion Time", "DS"),
        0x0018_0083 => ("Number of Averages", "DS"),
        0x0018_0084 => ("Imaging Frequency", "DS"),
        0x0018_0085 => ("Imaged Nucleus", "SH"),
        0x0018_0086 => ("Echo Numbers", "IS"),
        0x0018_0087 => ("Magnetic Field Strength", "DS"),
        0x0018_0088 => ("Spacing Between Slices", "DS"),
        0x0018_0089 => ("Number of Phase Encoding Steps", "IS"),
        0x0018_0090 => ("Data Collection Diameter", "DS"),
        0x0018_0091 => ("Echo Train Length", "IS"),
        0x0018_0093 => ("Percent Sampling", "DS"),
        0x0018_0094 => ("Percent Phase Field of View", "DS"),
        0x0018_0095 => ("Pixel Bandwidth", "DS"),
        0x0018_1000 => ("Device Serial Number", "LO"),
        0x0018_1020 => ("Software Versions", "LO"),
        0x0018_1030 => ("Protocol Name", "LO"),
        0x0018_1063 => ("Frame Time", "DS"),
        0x0018_1065 => ("Frame Time Vector", "DS"),
        0x0018_1100 => ("Reconstruction Diameter", "DS"),
        0x0018_1120 => ("Gantry/Detector Tilt", "DS"),
        0x0018_1130 => ("Table Height", "DS"),
        0x0018_1140 => ("Rotation Direction", "CS"),
        0x0018_1150 => ("Exposure Time", "IS"),
        0x0018_1151 => ("X-Ray Tube Current", "IS"),
        0x0018_1152 => ("Exposure", "IS"),
        0x0018_1164 => ("Imager Pixel Spacing", "DS"),
        0x0018_1170 => ("Generator Power", "IS"),
        0x0018_1190 => ("Focal Spot(s)", "DS"),
        0x0018_1210 => ("Convolution Kernel", "SH"),
        0x0018_1250 => ("Receive Coil Name", "SH"),
        0x0018_1251 => ("Transmit Coil Name", "SH"),
        0x0018_1310 => ("Acquisition Matrix", "US"),
        0x0018_1312 => ("In-plane Phase Encoding Direction", "CS"),
        0x0018_1314 => ("Flip Angle", "DS"),
        0x0018_1316 => ("SAR", "DS"),
        0x0018_5100 => ("Patient Position", "CS"),
        0x0018_5101 => ("View Position", "CS"),
        0x0018_6011 => ("Sequence of Ultrasound Regions", "SQ"),
        0x0020_000D => ("Study Instance UID", "UI"),
        0x0020_000E => ("Series Instance UID", "UI"),
        0x0020_0010 => ("Study ID", "SH"),
        0x0020_0011 => ("Series Number", "IS"),
        0x0020_0012 => ("Acquisition Number", "IS"),
        0x0020_0013 => ("Instance Number", "IS"),
        0x0020_0020 => ("Patient Orientation", "CS"),
        0x0020_0032 => ("Image Position (Patient)", "DS"),
        0x0020_0037 => ("Image Orientation (Patient)", "DS"),
        0x0020_0052 => ("Frame of Reference UID", "UI"),
        0x0020_0060 => ("Laterality", "CS"),
        0x0020_0100 => ("Temporal Position Identifier", "IS"),
        0x0020_0105 => ("Number of Temporal Positions", "IS"),
        0x0020_1002 => ("Images in Acquisition", "IS"),
        0x0020_1040 => ("Position Reference Indicator", "LO"),
        0x0020_1041 => ("Slice Location", "DS"),
        0x0020_4000 => ("Image Comments", "LT"),
        0x0028_0002 => ("Samples per Pixel", "US"),
        0x0028_0004 => ("Photometric Interpretation", "CS"),
        0x0028_0006 => ("Planar Configuration", "US"),
        0x0028_0008 => ("Number of Frames", "IS"),
        0x0028_0009 => ("Frame Increment Pointer", "AT"),
        0x0028_0010 => ("Rows", "US"),
        0x0028_0011 => ("Columns", "US"),
        0x0028_0014 => ("Ultrasound Color Data Present", "US"),
        0x0028_0030 => ("Pixel Spacing", "DS"),
        0x0028_0034 => ("Pixel Aspect Ratio", "IS"),
        0x0028_0051 => ("Corrected Image", "CS"),
        0x0028_0100 => ("Bits Allocated", "US"),
        0x0028_0101 => ("Bits Stored", "US"),
        0x0028_0102 => ("High Bit", "US"),
        0x0028_0103 => ("Pixel Representation", "US"),
        0x0028_0106 => ("Smallest Image Pixel Value", "US"),
        0x0028_0107 => ("Largest Image Pixel Value", "US"),
        0x0028_0120 => ("Pixel Padding Value", "US"),
        0x0028_0301 => ("Burned In Annotation", "CS"),
        0x0028_1040 => ("Pixel Intensity Relationship", "CS"),
        0x0028_1050 => ("Window Center", "DS"),
        0x0028_1051 => ("Window Width", "DS"),
        0x0028_1052 => ("Rescale Intercept", "DS"),
        0x0028_1053 => ("Rescale Slope", "DS"),
        0x0028_1054 => ("Rescale Type", "LO"),
        0x0028_1055 => ("Window Center & Width Explanation", "LO"),
        0x0028_1056 => ("VOI LUT Function", "CS"),
        0x0028_1101 => ("Red Palette Color LUT Descriptor", "US"),
        0x0028_1102 => ("Green Palette Color LUT Descriptor", "US"),
        0x0028_1103 => ("Blue Palette Color LUT Descriptor", "US"),
        0x0028_1199 => ("Palette Color LUT UID", "UI"),
        0x0028_1201 => ("Red Palette Color LUT Data", "OW"),
        0x0028_1202 => ("Green Palette Color LUT Data", "OW"),
        0x0028_1203 => ("Blue Palette Color LUT Data", "OW"),
        0x0028_1221 => ("Segmented Red Palette Color LUT Data", "OW"),
        0x0028_1222 => ("Segmented Green Palette Color LUT Data", "OW"),
        0x0028_1223 => ("Segmented Blue Palette Color LUT Data", "OW"),
        0x0028_2000 => ("ICC Profile", "OB"),
        0x0028_2110 => ("Lossy Image Compression", "CS"),
        0x0028_2112 => ("Lossy Image Compression Ratio", "DS"),
        0x0028_2114 => ("Lossy Image Compression Method", "CS"),
        0x0028_3000 => ("Modality LUT Sequence", "SQ"),
        0x0028_3002 => ("LUT Descriptor", "US"),
        0x0028_3003 => ("LUT Explanation", "LO"),
        0x0028_3006 => ("LUT Data", "US"),
        0x0028_3010 => ("VOI LUT Sequence", "SQ"),
        0x0028_9132 => ("Frame VOI LUT Sequence", "SQ"),
        0x0028_9145 => ("Pixel Value Transformation Sequence", "SQ"),
        0x0040_9096 => ("Real World Value Mapping Sequence", "SQ"),
        0x0040_A170 => ("Purpose of Reference Code Sequence", "SQ"),
        0x0054_0081 => ("Number of Slices", "US"),
        0x0054_1001 => ("Units", "CS"),
        0x0088_0200 => ("Icon Image Sequence", "SQ"),
        0x5200_9229 => ("Shared Functional Groups Sequence", "SQ"),
        0x5200_9230 => ("Per-frame Functional Groups Sequence", "SQ"),
        0x7FE0_0001 => ("Extended Offset Table", "OV"),
        0x7FE0_0002 => ("Extended Offset Table Lengths", "OV"),
        0x7FE0_0008 => ("Float Pixel Data", "OF"),
        0x7FE0_0009 => ("Double Float Pixel Data", "OD"),
        0x7FE0_0010 => ("Pixel Data", "OW"),
        0xFFFE_E000 => ("Item", "--"),
        0xFFFE_E00D => ("Item Delimitation Item", "--"),
        0xFFFE_E0DD => ("Sequence Delimitation Item", "--"),
        _ => {
            let group = tag_group(t);
            let element = tag_element(t);
            return match (group % 2 == 1, element) {
                (_, 0x0000) => Some(("Group Length", "UL")),
                (true, 0x0010..=0x00FF) => Some(("Private Creator", "LO")),
                _ => None,
            };
        }
    };
    Some(entry)
}

pub fn uid_name(uid: &str) -> Option<&'static str> {
    let name = match uid {
        "1.2.840.10008.1.2" => "Implicit VR Little Endian",
        "1.2.840.10008.1.2.1" => "Explicit VR Little Endian",
        "1.2.840.10008.1.2.1.98" => "Encapsulated Uncompressed Explicit VR Little Endian",
        "1.2.840.10008.1.2.1.99" => "Deflated Explicit VR Little Endian",
        "1.2.840.10008.1.2.2" => "Explicit VR Big Endian",
        "1.2.840.10008.1.2.4.50" => "JPEG Baseline (Process 1)",
        "1.2.840.10008.1.2.4.51" => "JPEG Extended (Process 2 & 4)",
        "1.2.840.10008.1.2.4.52" => "JPEG Extended (Process 3 & 5)",
        "1.2.840.10008.1.2.4.53" => "JPEG Spectral Selection, Non-Hierarchical (Process 6 & 8)",
        "1.2.840.10008.1.2.4.54" => "JPEG Spectral Selection, Non-Hierarchical (Process 7 & 9)",
        "1.2.840.10008.1.2.4.55" => "JPEG Full Progression, Non-Hierarchical (Process 10 & 12)",
        "1.2.840.10008.1.2.4.56" => "JPEG Full Progression, Non-Hierarchical (Process 11 & 13)",
        "1.2.840.10008.1.2.4.57" => "JPEG Lossless, Non-Hierarchical (Process 14)",
        "1.2.840.10008.1.2.4.58" => "JPEG Lossless, Non-Hierarchical (Process 15)",
        "1.2.840.10008.1.2.4.59" => "JPEG Extended, Hierarchical (Process 16 & 18)",
        "1.2.840.10008.1.2.4.60" => "JPEG Extended, Hierarchical (Process 17 & 19)",
        "1.2.840.10008.1.2.4.61" => "JPEG Spectral Selection, Hierarchical (Process 20 & 22)",
        "1.2.840.10008.1.2.4.62" => "JPEG Spectral Selection, Hierarchical (Process 21 & 23)",
        "1.2.840.10008.1.2.4.63" => "JPEG Full Progression, Hierarchical (Process 24 & 26)",
        "1.2.840.10008.1.2.4.64" => "JPEG Full Progression, Hierarchical (Process 25 & 27)",
        "1.2.840.10008.1.2.4.65" => "JPEG Lossless, Hierarchical (Process 28)",
        "1.2.840.10008.1.2.4.66" => "JPEG Lossless, Hierarchical (Process 29)",
        "1.2.840.10008.1.2.4.70" => "JPEG Lossless, Non-Hierarchical, First-Order Prediction (Process 14, SV1)",
        "1.2.840.10008.1.2.4.80" => "JPEG-LS Lossless",
        "1.2.840.10008.1.2.4.81" => "JPEG-LS Near-Lossless",
        "1.2.840.10008.1.2.4.90" => "JPEG 2000 (Lossless Only)",
        "1.2.840.10008.1.2.4.91" => "JPEG 2000",
        "1.2.840.10008.1.2.4.92" => "JPEG 2000 Part 2 Multi-component (Lossless Only)",
        "1.2.840.10008.1.2.4.93" => "JPEG 2000 Part 2 Multi-component",
        "1.2.840.10008.1.2.4.94" => "JPIP Referenced",
        "1.2.840.10008.1.2.4.95" => "JPIP Referenced Deflate",
        "1.2.840.10008.1.2.4.100" => "MPEG2 Main Profile / Main Level",
        "1.2.840.10008.1.2.4.100.1" => "Fragmentable MPEG2 Main Profile / Main Level",
        "1.2.840.10008.1.2.4.101" => "MPEG2 Main Profile / High Level",
        "1.2.840.10008.1.2.4.101.1" => "Fragmentable MPEG2 Main Profile / High Level",
        "1.2.840.10008.1.2.4.102" => "MPEG-4 AVC/H.264 High Profile / Level 4.1",
        "1.2.840.10008.1.2.4.102.1" => "Fragmentable MPEG-4 AVC/H.264 High Profile / Level 4.1",
        "1.2.840.10008.1.2.4.103" => "MPEG-4 AVC/H.264 BD-compatible High Profile / Level 4.1",
        "1.2.840.10008.1.2.4.103.1" => "Fragmentable MPEG-4 AVC/H.264 BD-compatible High Profile / Level 4.1",
        "1.2.840.10008.1.2.4.104" => "MPEG-4 AVC/H.264 High Profile / Level 4.2 For 2D Video",
        "1.2.840.10008.1.2.4.105" => "MPEG-4 AVC/H.264 High Profile / Level 4.2 For 3D Video",
        "1.2.840.10008.1.2.4.106" => "MPEG-4 AVC/H.264 Stereo High Profile / Level 4.2",
        "1.2.840.10008.1.2.4.107" => "HEVC/H.265 Main Profile / Level 5.1",
        "1.2.840.10008.1.2.4.108" => "HEVC/H.265 Main 10 Profile / Level 5.1",
        "1.2.840.10008.1.2.4.110" => "JPEG XL Lossless",
        "1.2.840.10008.1.2.4.111" => "JPEG XL JPEG Recompression",
        "1.2.840.10008.1.2.4.112" => "JPEG XL",
        "1.2.840.10008.1.2.4.201" => "High-Throughput JPEG 2000 (Lossless Only)",
        "1.2.840.10008.1.2.4.202" => "High-Throughput JPEG 2000 with RPCL Options (Lossless Only)",
        "1.2.840.10008.1.2.4.203" => "High-Throughput JPEG 2000",
        "1.2.840.10008.1.2.5" => "RLE Lossless",
        "1.2.840.10008.1.2.8.1" => "Deflated Image Frame Compression",
        "1.2.840.113619.5.2" => "GE Private Implicit VR Little Endian, Big Endian Pixel Data",
        "1.2.840.10008.1.3.10" => "Media Storage Directory Storage",
        "1.2.840.10008.5.1.4.1.1.1" => "Computed Radiography Image Storage",
        "1.2.840.10008.5.1.4.1.1.1.1" => "Digital X-Ray Image Storage - For Presentation",
        "1.2.840.10008.5.1.4.1.1.1.1.1" => "Digital X-Ray Image Storage - For Processing",
        "1.2.840.10008.5.1.4.1.1.1.2" => "Digital Mammography X-Ray Image Storage - For Presentation",
        "1.2.840.10008.5.1.4.1.1.1.2.1" => "Digital Mammography X-Ray Image Storage - For Processing",
        "1.2.840.10008.5.1.4.1.1.1.3" => "Digital Intra-Oral X-Ray Image Storage - For Presentation",
        "1.2.840.10008.5.1.4.1.1.2" => "CT Image Storage",
        "1.2.840.10008.5.1.4.1.1.2.1" => "Enhanced CT Image Storage",
        "1.2.840.10008.5.1.4.1.1.3.1" => "Ultrasound Multi-frame Image Storage",
        "1.2.840.10008.5.1.4.1.1.4" => "MR Image Storage",
        "1.2.840.10008.5.1.4.1.1.4.1" => "Enhanced MR Image Storage",
        "1.2.840.10008.5.1.4.1.1.4.2" => "MR Spectroscopy Storage",
        "1.2.840.10008.5.1.4.1.1.4.3" => "Enhanced MR Color Image Storage",
        "1.2.840.10008.5.1.4.1.1.6.1" => "Ultrasound Image Storage",
        "1.2.840.10008.5.1.4.1.1.7" => "Secondary Capture Image Storage",
        "1.2.840.10008.5.1.4.1.1.7.1" => "Multi-frame Single Bit Secondary Capture Image Storage",
        "1.2.840.10008.5.1.4.1.1.7.2" => "Multi-frame Grayscale Byte Secondary Capture Image Storage",
        "1.2.840.10008.5.1.4.1.1.7.3" => "Multi-frame Grayscale Word Secondary Capture Image Storage",
        "1.2.840.10008.5.1.4.1.1.7.4" => "Multi-frame True Color Secondary Capture Image Storage",
        "1.2.840.10008.5.1.4.1.1.11.1" => "Grayscale Softcopy Presentation State Storage",
        "1.2.840.10008.5.1.4.1.1.12.1" => "X-Ray Angiographic Image Storage",
        "1.2.840.10008.5.1.4.1.1.12.2" => "X-Ray Radiofluoroscopic Image Storage",
        "1.2.840.10008.5.1.4.1.1.20" => "Nuclear Medicine Image Storage",
        "1.2.840.10008.5.1.4.1.1.30" => "Parametric Map Storage",
        "1.2.840.10008.5.1.4.1.1.66.4" => "Segmentation Storage",
        "1.2.840.10008.5.1.4.1.1.77.1.6" => "VL Whole Slide Microscopy Image Storage",
        "1.2.840.10008.5.1.4.1.1.88.11" => "Basic Text SR Storage",
        "1.2.840.10008.5.1.4.1.1.88.22" => "Enhanced SR Storage",
        "1.2.840.10008.5.1.4.1.1.88.33" => "Comprehensive SR Storage",
        "1.2.840.10008.5.1.4.1.1.128" => "Positron Emission Tomography Image Storage",
        "1.2.840.10008.5.1.4.1.1.481.1" => "RT Image Storage",
        "1.2.840.10008.5.1.4.1.1.481.2" => "RT Dose Storage",
        "1.2.840.10008.5.1.4.1.1.481.3" => "RT Structure Set Storage",
        _ => return None,
    };
    Some(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vr {
    Ae,
    As,
    At,
    Cs,
    Da,
    Ds,
    Dt,
    Fl,
    Fd,
    Is,
    Lo,
    Lt,
    Ob,
    Od,
    Of,
    Ol,
    Ov,
    Ow,
    Pn,
    Sh,
    Sl,
    Sq,
    Ss,
    St,
    Sv,
    Tm,
    Uc,
    Ui,
    Ul,
    Un,
    Ur,
    Us,
    Ut,
    Uv,
    Implicit,
}

impl Vr {
    pub fn from_bytes(b: [u8; 2]) -> Option<Vr> {
        let vr = match &b {
            b"AE" => Vr::Ae,
            b"AS" => Vr::As,
            b"AT" => Vr::At,
            b"CS" => Vr::Cs,
            b"DA" => Vr::Da,
            b"DS" => Vr::Ds,
            b"DT" => Vr::Dt,
            b"FL" => Vr::Fl,
            b"FD" => Vr::Fd,
            b"IS" => Vr::Is,
            b"LO" => Vr::Lo,
            b"LT" => Vr::Lt,
            b"OB" => Vr::Ob,
            b"OD" => Vr::Od,
            b"OF" => Vr::Of,
            b"OL" => Vr::Ol,
            b"OV" => Vr::Ov,
            b"OW" => Vr::Ow,
            b"PN" => Vr::Pn,
            b"SH" => Vr::Sh,
            b"SL" => Vr::Sl,
            b"SQ" => Vr::Sq,
            b"SS" => Vr::Ss,
            b"ST" => Vr::St,
            b"SV" => Vr::Sv,
            b"TM" => Vr::Tm,
            b"UC" => Vr::Uc,
            b"UI" => Vr::Ui,
            b"UL" => Vr::Ul,
            b"UN" => Vr::Un,
            b"UR" => Vr::Ur,
            b"US" => Vr::Us,
            b"UT" => Vr::Ut,
            b"UV" => Vr::Uv,
            _ => return None,
        };
        Some(vr)
    }

    pub fn has_long_length(&self) -> bool {
        matches!(
            self,
            Vr::Ob
                | Vr::Od
                | Vr::Of
                | Vr::Ol
                | Vr::Ov
                | Vr::Ow
                | Vr::Sq
                | Vr::Sv
                | Vr::Uc
                | Vr::Un
                | Vr::Ur
                | Vr::Ut
                | Vr::Uv
        )
    }

    pub fn is_string(&self) -> bool {
        matches!(
            self,
            Vr::Ae
                | Vr::As
                | Vr::Cs
                | Vr::Da
                | Vr::Ds
                | Vr::Dt
                | Vr::Is
                | Vr::Lo
                | Vr::Lt
                | Vr::Pn
                | Vr::Sh
                | Vr::St
                | Vr::Tm
                | Vr::Uc
                | Vr::Ui
                | Vr::Ur
                | Vr::Ut
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Vr::Ae => "AE",
            Vr::As => "AS",
            Vr::At => "AT",
            Vr::Cs => "CS",
            Vr::Da => "DA",
            Vr::Ds => "DS",
            Vr::Dt => "DT",
            Vr::Fl => "FL",
            Vr::Fd => "FD",
            Vr::Is => "IS",
            Vr::Lo => "LO",
            Vr::Lt => "LT",
            Vr::Ob => "OB",
            Vr::Od => "OD",
            Vr::Of => "OF",
            Vr::Ol => "OL",
            Vr::Ov => "OV",
            Vr::Ow => "OW",
            Vr::Pn => "PN",
            Vr::Sh => "SH",
            Vr::Sl => "SL",
            Vr::Sq => "SQ",
            Vr::Ss => "SS",
            Vr::St => "ST",
            Vr::Sv => "SV",
            Vr::Tm => "TM",
            Vr::Uc => "UC",
            Vr::Ui => "UI",
            Vr::Ul => "UL",
            Vr::Un => "UN",
            Vr::Ur => "UR",
            Vr::Us => "US",
            Vr::Ut => "UT",
            Vr::Uv => "UV",
            Vr::Implicit => "??",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferSyntax {
    ImplicitLittleEndian,
    ExplicitLittleEndian,
    ExplicitBigEndian,
    PrivateGeBigEndian,
    DeflatedExplicitLittleEndian,
    EncapsulatedUncompressed,
    DeflatedImageFrameCompression,
    JpegDct,
    JpegLossless,
    JpegLs,
    Rle,
    Jpeg2000,
    JpegXl,
    Video,
    Unknown,
}

impl TransferSyntax {
    pub fn from_uid(uid: &str) -> TransferSyntax {
        match uid {
            "1.2.840.10008.1.2" => TransferSyntax::ImplicitLittleEndian,
            "1.2.840.10008.1.2.1" => TransferSyntax::ExplicitLittleEndian,
            "1.2.840.10008.1.2.2" => TransferSyntax::ExplicitBigEndian,
            "1.2.840.10008.1.2.1.98" => TransferSyntax::EncapsulatedUncompressed,
            "1.2.840.10008.1.2.1.99" => TransferSyntax::DeflatedExplicitLittleEndian,
            "1.2.840.10008.1.2.8.1" => TransferSyntax::DeflatedImageFrameCompression,
            "1.2.840.10008.1.2.4.57"
            | "1.2.840.10008.1.2.4.58"
            | "1.2.840.10008.1.2.4.65"
            | "1.2.840.10008.1.2.4.66"
            | "1.2.840.10008.1.2.4.70" => TransferSyntax::JpegLossless,
            "1.2.840.10008.1.2.4.80" | "1.2.840.10008.1.2.4.81" => TransferSyntax::JpegLs,
            "1.2.840.10008.1.2.4.90"
            | "1.2.840.10008.1.2.4.91"
            | "1.2.840.10008.1.2.4.92"
            | "1.2.840.10008.1.2.4.93"
            | "1.2.840.10008.1.2.4.201"
            | "1.2.840.10008.1.2.4.202"
            | "1.2.840.10008.1.2.4.203" => TransferSyntax::Jpeg2000,
            "1.2.840.10008.1.2.4.110" | "1.2.840.10008.1.2.4.111" | "1.2.840.10008.1.2.4.112" => TransferSyntax::JpegXl,
            "1.2.840.10008.1.2.5" => TransferSyntax::Rle,
            "1.2.840.113619.5.2" => TransferSyntax::PrivateGeBigEndian,
            _ => {
                if let Some(rest) = uid.strip_prefix("1.2.840.10008.1.2.4.") {
                    if let Ok(n) = rest.split('.').next().unwrap_or("").parse::<u32>() {
                        return match n {
                            50..=66 => TransferSyntax::JpegDct,
                            100..=108 => TransferSyntax::Video,
                            _ => TransferSyntax::Unknown,
                        };
                    }
                }
                TransferSyntax::Unknown
            }
        }
    }

    pub fn is_explicit_vr(&self) -> bool {
        !matches!(self, TransferSyntax::ImplicitLittleEndian | TransferSyntax::PrivateGeBigEndian)
    }

    pub fn is_big_endian(&self) -> bool {
        matches!(self, TransferSyntax::ExplicitBigEndian)
    }

    pub fn pixel_data_big_endian(&self) -> bool {
        matches!(self, TransferSyntax::PrivateGeBigEndian)
    }

    pub fn is_encapsulated(&self) -> bool {
        matches!(
            self,
            TransferSyntax::EncapsulatedUncompressed
                | TransferSyntax::DeflatedImageFrameCompression
                | TransferSyntax::JpegDct
                | TransferSyntax::JpegLossless
                | TransferSyntax::JpegLs
                | TransferSyntax::Rle
                | TransferSyntax::Jpeg2000
                | TransferSyntax::JpegXl
                | TransferSyntax::Video
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            TransferSyntax::ImplicitLittleEndian => "Implicit VR Little Endian",
            TransferSyntax::ExplicitLittleEndian => "Explicit VR Little Endian",
            TransferSyntax::ExplicitBigEndian => "Explicit VR Big Endian",
            TransferSyntax::PrivateGeBigEndian => "Private GE Implicit VR Little Endian with Big Endian Pixel Data",
            TransferSyntax::DeflatedExplicitLittleEndian => "Deflated Explicit VR Little Endian",
            TransferSyntax::EncapsulatedUncompressed => "Encapsulated Uncompressed",
            TransferSyntax::DeflatedImageFrameCompression => "Deflated Image Frame Compression",
            TransferSyntax::JpegDct => "JPEG (DCT)",
            TransferSyntax::JpegLossless => "JPEG Lossless",
            TransferSyntax::JpegLs => "JPEG-LS",
            TransferSyntax::Rle => "RLE Lossless",
            TransferSyntax::Jpeg2000 => "JPEG 2000",
            TransferSyntax::JpegXl => "JPEG XL",
            TransferSyntax::Video => "Video (MPEG/HEVC)",
            TransferSyntax::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Photometric {
    Monochrome1,
    Monochrome2,
    PaletteColor,
    Rgb,
    Hsv,
    Argb,
    Cmyk,
    YbrFull,
    YbrFull422,
    YbrPartial422,
    YbrPartial420,
    YbrIct,
    YbrRct,
    Unknown(String),
}

impl Photometric {
    pub fn from_str(s: &str) -> Photometric {
        match s.trim() {
            "MONOCHROME1" => Photometric::Monochrome1,
            "MONOCHROME2" => Photometric::Monochrome2,
            "PALETTE COLOR" => Photometric::PaletteColor,
            "RGB" => Photometric::Rgb,
            "HSV" => Photometric::Hsv,
            "ARGB" => Photometric::Argb,
            "CMYK" => Photometric::Cmyk,
            "YBR_FULL" => Photometric::YbrFull,
            "YBR_FULL_422" => Photometric::YbrFull422,
            "YBR_PARTIAL_422" => Photometric::YbrPartial422,
            "YBR_PARTIAL_420" => Photometric::YbrPartial420,
            "YBR_ICT" => Photometric::YbrIct,
            "YBR_RCT" => Photometric::YbrRct,
            other => Photometric::Unknown(other.to_string()),
        }
    }

    pub fn is_monochrome(&self) -> bool {
        matches!(self, Photometric::Monochrome1 | Photometric::Monochrome2)
    }

    pub fn is_ybr(&self) -> bool {
        matches!(
            self,
            Photometric::YbrFull | Photometric::YbrFull422 | Photometric::YbrPartial422 | Photometric::YbrPartial420
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiFunction {
    Linear,
    LinearExact,
    Sigmoid,
}
