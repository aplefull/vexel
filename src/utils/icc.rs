use std::collections::HashMap;

#[derive(Debug)]
pub struct ICCProfile {
    header: ICCHeader,
    tag_table: HashMap<TagSignature, TagData>,
}

#[derive(Debug)]
pub struct ICCHeader {
    profile_size: u32,
    preferred_cmm_type: u32,
    version: Version,
    profile_class: ProfileClass,
    color_space: ColorSpace,
    pcs: ColorSpace,  // Profile Connection Space
    creation_date: DateTime,
    profile_flags: ProfileFlags,
    device_manufacturer: u32,
    device_model: u32,
    device_attributes: u64,
    rendering_intent: RenderingIntent,
    pcs_illuminant: XYZNumber,
    profile_creator: u32,
    profile_id: [u8; 16],  // MD5 hash
}

#[derive(Debug)]
pub struct Version {
    major: u8,
    minor: u8,
    bugfix: u8,
}

#[derive(Debug)]
pub struct DateTime {
    year: u16,
    month: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
}

#[derive(Debug)]
pub struct XYZNumber {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum ProfileClass {
    Input = 0x73636E72,           // 'scnr'
    Display = 0x6D6E7472,         // 'mntr'
    Output = 0x70727472,          // 'prtr'
    DeviceLink = 0x6C696E6B,      // 'link'
    ColorSpace = 0x73706163,      // 'spac'
    Abstract = 0x61627374,        // 'abst'
    NamedColor = 0x6E6D636C,      // 'nmcl'
}

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum ColorSpace {
    XYZ = 0x58595A20,            // 'XYZ '
    Lab = 0x4C616220,            // 'Lab '
    Luv = 0x4C757620,            // 'Luv '
    YCbCr = 0x59436272,          // 'YCbr'
    Yxy = 0x59787920,            // 'Yxy '
    RGB = 0x52474220,            // 'RGB '
    Gray = 0x47524159,           // 'GRAY'
    HSV = 0x48535620,            // 'HSV '
    CMYK = 0x434D594B,           // 'CMYK'
}

#[derive(Debug)]
pub struct ProfileFlags {
    embedded: bool,
    independent: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum RenderingIntent {
    Perceptual = 0,
    RelativeColorimetric = 1,
    Saturation = 2,
    AbsoluteColorimetric = 3,
}

// Tag Data Types
#[derive(Debug)]
pub enum TagData {
    Text(String),
    XYZ(XYZNumber),
    Curve(Vec<u16>),
    ParametricCurve(ParametricCurveData),
    LUT(LookupTable),
    Measurement(MeasurementData),
    MultiLocalizedUnicode(Vec<LocalizedString>),
    ChromaticityType(ChromaticityData),
}

#[derive(Debug)]
pub struct ParametricCurveData {
    function_type: u16,
    parameters: Vec<f32>,
}

#[derive(Debug)]
pub struct LookupTable {
    input_channels: u8,
    output_channels: u8,
    grid_points: Vec<u8>,
    matrix: Option<Vec<f32>>,
    data: Vec<u8>,
}

#[derive(Debug)]
pub struct MeasurementData {
    observer: u32,
    backing_xyz: XYZNumber,
    geometry: u32,
    flare: u32,
    illuminant: u32,
}

#[derive(Debug)]
pub struct LocalizedString {
    language_code: String,
    country_code: String,
    text: String,
}

#[derive(Debug)]
pub struct ChromaticityData {
    channels: u16,
    coordinates: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum TagSignature {
    // Required tags
    ProfileDescription = 0x64657363,  // 'desc'
    Copyright = 0x63707274,           // 'cprt'
    MediaWhitePoint = 0x77747074,     // 'wtpt'
    MediaBlackPoint = 0x626B7074,     // 'bkpt'

    // Display and Input Profile tags
    RedColorant = 0x7258595A,         // 'rXYZ'
    GreenColorant = 0x6758595A,       // 'gXYZ'
    BlueColorant = 0x6258595A,        // 'bXYZ'
    RedTRC = 0x72545243,              // 'rTRC'
    GreenTRC = 0x67545243,            // 'gTRC'
    BlueTRC = 0x62545243,             // 'bTRC'

    // Output Profile tags
    AToB0 = 0x41324230,               // 'A2B0'
    AToB1 = 0x41324231,               // 'A2B1'
    AToB2 = 0x41324232,               // 'A2B2'
    BToA0 = 0x42324130,               // 'B2A0'
    BToA1 = 0x42324131,               // 'B2A1'
    BToA2 = 0x42324132,               // 'B2A2'
    Gamut = 0x67616D74,               // 'gamt'

    // Color Space tags
    ColorantTable = 0x636C7274,       // 'clrt'
    ColorantTableOut = 0x636C6F74,    // 'clot'

    // Device Settings
    DeviceMfgDesc = 0x646D6E64,       // 'dmnd'
    DeviceModelDesc = 0x646D6464,     // 'dmdd'
    DeviceSettings = 0x64657673,      // 'devs'

    // Named Color tags
    NamedColor2 = 0x6E636C32,         // 'ncl2'

    // Measurement tags
    Measurement = 0x6D656173,         // 'meas'
    MeasurementBacking = 0x6D626B67,  // 'mbkg'

    // Preview tags
    Preview0 = 0x70726530,            // 'pre0'
    Preview1 = 0x70726531,            // 'pre1'
    Preview2 = 0x70726532,            // 'pre2'

    // Profile Info tags
    ProfileSequenceDesc = 0x70736571,  // 'pseq'
    ProfileSequenceInfo = 0x70736964,  // 'psid'
    Technology = 0x74656368,          // 'tech'

    // Viewing Conditions
    ViewingCondDesc = 0x76756564,     // 'vued'
    ViewingConditions = 0x76696577,   // 'view'

    // Characterization tags
    CharTarget = 0x74617267,          // 'targ'
    ChromaticAdaptation = 0x63686164, // 'chad'
    ChromaticityType = 0x6368726D,    // 'chrm'

    // Misc tags
    CalibrationDateTime = 0x63616C74,  // 'calt'
    LuminanceTag = 0x6C756D69,        // 'lumi'
    MetaData = 0x6D657461,            // 'meta'
    PerceptualRenderingIntentGamut = 0x72696730, // 'rig0'
    SaturationRenderingIntentGamut = 0x72696732, // 'rig2'
}

impl TagSignature {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 4 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           "Tag signature too short"));
        }

        let signature = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);

        match signature {
            0x64657363 => Ok(TagSignature::ProfileDescription),
            0x63707274 => Ok(TagSignature::Copyright),
            0x77747074 => Ok(TagSignature::MediaWhitePoint),
            // TODO ... match all other signatures
            _ => {
                eprintln!("Unknown tag signature: 0x{:X}", signature);
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                        "Unknown tag signature"))
            }
        }
    }
}

impl ICCProfile {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        // Read header (first 128 bytes)
        let header = ICCHeader::from_bytes(&data[0..128])?;

        // Read tag count
        let tag_count = u32::from_be_bytes([data[128], data[129], data[130], data[131]]);

        // Read tag table
        let mut tag_table = HashMap::new();
        let mut offset = 132;

        for _ in 0..tag_count {
            let signature = TagSignature::from_bytes(&data[offset..offset + 4])?;
            let data_offset = u32::from_be_bytes([data[offset + 4], data[offset + 5],
                data[offset + 6], data[offset + 7]]);
            let data_size = u32::from_be_bytes([data[offset + 8], data[offset + 9],
                data[offset + 10], data[offset + 11]]);

            let tag_data = TagData::from_bytes(&data[data_offset as usize..
                (data_offset + data_size) as usize])?;

            tag_table.insert(signature, tag_data);
            offset += 12;
        }

        Ok(ICCProfile {
            header,
            tag_table,
        })
    }
}

impl ICCHeader {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 128 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           "ICC header too short"));
        }

        Ok(ICCHeader {
            profile_size: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            preferred_cmm_type: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            version: Version::from_bytes(&data[8..12])?,
            profile_class: ProfileClass::from_bytes(&data[12..16])?,
            color_space: ColorSpace::from_bytes(&data[16..20])?,
            pcs: ColorSpace::from_bytes(&data[20..24])?,
            creation_date: DateTime::from_bytes(&data[24..36])?,
            profile_flags: ProfileFlags::from_bytes(&data[44..48])?,
            device_manufacturer: u32::from_be_bytes([data[48], data[49], data[50], data[51]]),
            device_model: u32::from_be_bytes([data[52], data[53], data[54], data[55]]),
            device_attributes: u64::from_be_bytes([data[56], data[57], data[58], data[59],
                data[60], data[61], data[62], data[63]]),
            rendering_intent: RenderingIntent::from_bytes(&data[64..68])?,
            pcs_illuminant: XYZNumber::from_bytes(&data[68..80])?,
            profile_creator: u32::from_be_bytes([data[80], data[81], data[82], data[83]]),
            profile_id: {
                let mut id = [0u8; 16];
                id.copy_from_slice(&data[84..100]);
                id
            },
        })
    }
}

impl Version {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        Ok(Version {
            major: data[0],
            minor: data[1],
            bugfix: data[2],
        })
    }
}

impl DateTime {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        Ok(DateTime {
            year: u16::from_be_bytes([data[0], data[1]]),
            month: u16::from_be_bytes([data[2], data[3]]),
            day: u16::from_be_bytes([data[4], data[5]]),
            hour: u16::from_be_bytes([data[6], data[7]]),
            minute: u16::from_be_bytes([data[8], data[9]]),
            second: u16::from_be_bytes([data[10], data[11]]),
        })
    }
}

impl XYZNumber {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        Ok(XYZNumber {
            x: f32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            y: f32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            z: f32::from_be_bytes([data[8], data[9], data[10], data[11]]),
        })
    }
}

impl ProfileClass {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 4 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           "Profile class data too short"));
        }

        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        match value {
            0x73636E72 => Ok(ProfileClass::Input),
            0x6D6E7472 => Ok(ProfileClass::Display),
            0x70727472 => Ok(ProfileClass::Output),
            0x6C696E6B => Ok(ProfileClass::DeviceLink),
            0x73706163 => Ok(ProfileClass::ColorSpace),
            0x61627374 => Ok(ProfileClass::Abstract),
            0x6E6D636C => Ok(ProfileClass::NamedColor),
            _ => {
                eprintln!("Unknown profile class: 0x{:X}", value);
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                        "Unknown profile class"))
            }
        }
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        (*self as u32).to_be_bytes()
    }
}

impl ColorSpace {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 4 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           "Color space data too short"));
        }

        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        match value {
            0x58595A20 => Ok(ColorSpace::XYZ),
            0x4C616220 => Ok(ColorSpace::Lab),
            0x4C757620 => Ok(ColorSpace::Luv),
            0x59436272 => Ok(ColorSpace::YCbCr),
            0x59787920 => Ok(ColorSpace::Yxy),
            0x52474220 => Ok(ColorSpace::RGB),
            0x47524159 => Ok(ColorSpace::Gray),
            0x48535620 => Ok(ColorSpace::HSV),
            0x434D594B => Ok(ColorSpace::CMYK),
            _ => {
                eprintln!("Unknown color space: 0x{:X}", value);
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                        "Unknown color space"))
            }
        }
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        (*self as u32).to_be_bytes()
    }
}

impl RenderingIntent {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 4 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           "Rendering intent data too short"));
        }

        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        match value {
            0 => Ok(RenderingIntent::Perceptual),
            1 => Ok(RenderingIntent::RelativeColorimetric),
            2 => Ok(RenderingIntent::Saturation),
            3 => Ok(RenderingIntent::AbsoluteColorimetric),
            _ => {
                eprintln!("Unknown rendering intent: 0x{:X}", value);
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                        "Unknown rendering intent"))
            }
        }
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        (*self as u32).to_be_bytes()
    }
}

impl ProfileFlags {
    pub fn from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 4 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           "Profile flags data too short"));
        }

        let flags = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        Ok(ProfileFlags {
            embedded: (flags & 0x1) != 0,
            independent: (flags & 0x2) != 0,
        })
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        let mut flags: u32 = 0;
        if self.embedded { flags |= 0x1; }
        if self.independent { flags |= 0x2; }
        flags.to_be_bytes()
    }
}

impl TagData {
    pub fn from_bytes(_data: &[u8]) -> Result<Self, std::io::Error> {
        // TODO
        
        Ok(TagData::Text("".to_string()))
    }
}
