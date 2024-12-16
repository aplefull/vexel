use std::io::{Read, Seek, SeekFrom};
use crate::{Image, PixelData};
use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::types::ByteOrder;

// TODO some tags are commented out since they are duplicates, but with different values
// This probably requires a different approach to handle them
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum TiffTags {
    NewSubfileType = 254,
    SubfileType = 255,
    ImageWidth = 256,
    ImageLength = 257,
    BitsPerSample = 258,
    Compression = 259,
    PhotometricInterpretation = 262,
    Threshholding = 263,
    CellWidth = 264,
    CellLength = 265,
    FillOrder = 266,
    DocumentName = 269,
    ImageDescription = 270,
    Make = 271,
    Model = 272,
    StripOffsets = 273,
    Orientation = 274,
    SamplesPerPixel = 277,
    RowsPerStrip = 278,
    StripByteCounts = 279,
    MinSampleValue = 280,
    MaxSampleValue = 281,
    XResolution = 282,
    YResolution = 283,
    PlanarConfiguration = 284,
    PageName = 285,
    XPosition = 286,
    YPosition = 287,
    FreeOffsets = 288,
    FreeByteCounts = 289,
    GrayResponseUnit = 290,
    GrayResponseCurve = 291,
    T4Options = 292,
    T6Options = 293,
    ResolutionUnit = 296,
    PageNumber = 297,
    TransferFunction = 301,
    Software = 305,
    DateTime = 306,
    Artist = 315,
    HostComputer = 316,
    Predictor = 317,
    WhitePoint = 318,
    PrimaryChromaticities = 319,
    ColorMap = 320,
    HalftoneHints = 321,
    TileWidth = 322,
    TileLength = 323,
    TileOffsets = 324,
    TileByteCounts = 325,
    BadFaxLines = 326,
    CleanFaxData = 327,
    ConsecutiveBadFaxLines = 328,
    SubIFDs = 330,
    InkSet = 332,
    InkNames = 333,
    NumberOfInks = 334,
    DotRange = 336,
    TargetPrinter = 337,
    ExtraSamples = 338,
    SampleFormat = 339,
    SMinSampleValue = 340,
    SMaxSampleValue = 341,
    TransferRange = 342,
    ClipPath = 343,
    XClipPathUnits = 344,
    YClipPathUnits = 345,
    Indexed = 346,
    JPEGTables = 347,
    OPIProxy = 351,
    GlobalParametersIFD = 400,
    ProfileType = 401,
    FaxProfile = 402,
    CodingMethods = 403,
    VersionYear = 404,
    ModeNumber = 405,
    Decode = 433,
    DefaultImageColor = 434,
    JPEGProc = 512,
    JPEGInterchangeFormat = 513,
    JPEGInterchangeFormatLength = 514,
    JPEGRestartInterval = 515,
    JPEGLosslessPredictors = 517,
    JPEGPointTransforms = 518,
    JPEGQTables = 519,
    JPEGDCTables = 520,
    JPEGACTables = 521,
    YCbCrCoefficients = 529,
    YCbCrSubSampling = 530,
    YCbCrPositioning = 531,
    ReferenceBlackWhite = 532,
    StripRowCounts = 559,
    XMP = 700,
    ImageRating = 18246,
    ImageRatingPercent = 18249,
    ImageID = 32781,
    WangAnnotation = 32932,
    CFARepeatPatternDim = 33421,
    CFAPattern = 33422,
    BatteryLevel = 33423,
    Copyright = 33432,
    ExposureTime = 33434,
    FNumber = 33437,
    MDFileTag = 33445,
    MDScalePixel = 33446,
    MDColorTable = 33447,
    MDLabName = 33448,
    MDSampleInfo = 33449,
    MDPrepDate = 33450,
    MDPrepTime = 33451,
    MDFileUnits = 33452,
    ModelPixelScaleTag = 33550,
    IPTCNAA = 33723,
    INGRPacketDataTag = 33918,
    INGRFlagRegisters = 33919,
    IrasBTransformationMatrix = 33920,
    ModelTiepointTag = 33922,
    Site = 34016,
    ColorSequence = 34017,
    IT8Header = 34018,
    RasterPadding = 34019,
    BitsPerRunLength = 34020,
    BitsPerExtendedRunLength = 34021,
    ColorTable = 34022,
    ImageColorIndicator = 34023,
    BackgroundColorIndicator = 34024,
    ImageColorValue = 34025,
    BackgroundColorValue = 34026,
    PixelIntensityRange = 34027,
    TransparencyIndicator = 34028,
    ColorCharacterization = 34029,
    HCUsage = 34030,
    TrapIndicator = 34031,
    CMYKEquivalent = 34032,
    Reserved = 34033,
    //Reserved = 34034,
    //Reserved = 34035,
    ModelTransformationTag = 34264,
    Photoshop = 34377,
    ExifIFD = 34665,
    InterColorProfile = 34675,
    ImageLayer = 34732,
    GeoKeyDirectoryTag = 34735,
    GeoDoubleParamsTag = 34736,
    GeoAsciiParamsTag = 34737,
    ExposureProgram = 34850,
    SpectralSensitivity = 34852,
    GPSInfo = 34853,
    ISOSpeedRatings = 34855,
    OECF = 34856,
    Interlace = 34857,
    TimeZoneOffset = 34858,
    SelfTimeMode = 34859,
    SensitivityType = 34864,
    StandardOutputSensitivity = 34865,
    RecommendedExposureIndex = 34866,
    ISOSpeed = 34867,
    ISOSpeedLatitudeyyy = 34868,
    ISOSpeedLatitudezzz = 34869,
    HylaFAXFaxRecvParams = 34908,
    HylaFAXFaxSubAddress = 34909,
    HylaFAXFaxRecvTime = 34910,
    ExifVersion = 36864,
    DateTimeOriginal = 36867,
    DateTimeDigitized = 36868,
    ComponentsConfiguration = 37121,
    CompressedBitsPerPixel = 37122,
    ShutterSpeedValue = 37377,
    ApertureValue = 37378,
    BrightnessValue = 37379,
    ExposureBiasValue = 37380,
    MaxApertureValue = 37381,
    SubjectDistance = 37382,
    MeteringMode = 37383,
    LightSource = 37384,
    Flash = 37385,
    FocalLength = 37386,
    FlashEnergy = 37387,
    SpatialFrequencyResponse = 37388,
    Noise = 37389,
    FocalPlaneXResolution = 37390,
    FocalPlaneYResolution = 37391,
    FocalPlaneResolutionUnit = 37392,
    ImageNumber = 37393,
    SecurityClassification = 37394,
    ImageHistory = 37395,
    SubjectLocation = 37396,
    ExposureIndex = 37397,
    TIFFEPStandardID = 37398,
    SensingMethod = 37399,
    MakerNote = 37500,
    UserComment = 37510,
    SubsecTime = 37520,
    SubsecTimeOriginal = 37521,
    SubsecTimeDigitized = 37522,
    ImageSourceData = 37724,
    XPTitle = 40091,
    XPComment = 40092,
    XPAuthor = 40093,
    XPKeywords = 40094,
    XPSubject = 40095,
    FlashpixVersion = 40960,
    ColorSpace = 40961,
    PixelXDimension = 40962,
    PixelYDimension = 40963,
    RelatedSoundFile = 40964,
    InteroperabilityIFD = 40965,
    //FlashEnergy = 41483,
    //SpatialFrequencyResponse = 41484,
    //FocalPlaneXResolution = 41486,
    //FocalPlaneYResolution = 41487,
    //FocalPlaneResolutionUnit = 41488,
    //SubjectLocation = 41492,
    //ExposureIndex = 41493,
    //SensingMethod = 41495,
    FileSource = 41728,
    SceneType = 41729,
    //CFAPattern = 41730,
    CustomRendered = 41985,
    ExposureMode = 41986,
    WhiteBalance = 41987,
    DigitalZoomRatio = 41988,
    FocalLengthIn35mmFilm = 41989,
    SceneCaptureType = 41990,
    GainControl = 41991,
    Contrast = 41992,
    Saturation = 41993,
    Sharpness = 41994,
    DeviceSettingDescription = 41995,
    SubjectDistanceRange = 41996,
    ImageUniqueID = 42016,
    CameraOwnerName = 42032,
    BodySerialNumber = 42033,
    LensSpecification = 42034,
    LensMake = 42035,
    LensModel = 42036,
    LensSerialNumber = 42037,
    GdalMetadata = 42112,
    GdalNodata = 42113,
    PixelFormat = 48129,
    Transformation = 48130,
    Uncompressed = 48131,
    ImageType = 48132,
    //ImageWidth = 48256
    ImageHeight = 48257,
    WidthResolution = 48258,
    HeightResolution = 48259,
    ImageOffset = 48320,
    ImageByteCount = 48321,
    AlphaOffset = 48322,
    AlphaByteCount = 48323,
    ImageDataDiscard = 48324,
    AlphaDataDiscard = 48325,
    OceScanJobDescription = 50215,
    OceApplicationSelector = 50216,
    OceIdentificationNumber = 50217,
    OceImageLogicCharacteristics = 50218,
    PrintImageMatching = 50341,
    DNGVersion = 50706,
    DNGBackwardVersion = 50707,
    UniqueCameraModel = 50708,
    LocalizedCameraModel = 50709,
    CFAPlaneColor = 50710,
    CFALayout = 50711,
    LinearizationTable = 50712,
    BlackLevelRepeatDim = 50713,
    BlackLevel = 50714,
    BlackLevelDeltaH = 50715,
    BlackLevelDeltaV = 50716,
    WhiteLevel = 50717,
    DefaultScale = 50718,
    DefaultCropOrigin = 50719,
    DefaultCropSize = 50720,
    ColorMatrix1 = 50721,
    ColorMatrix2 = 50722,
    CameraCalibration1 = 50723,
    CameraCalibration2 = 50724,
    ReductionMatrix1 = 50725,
    ReductionMatrix2 = 50726,
    AnalogBalance = 50727,
    AsShotNeutral = 50728,
    AsShotWhiteXY = 50729,
    BaselineExposure = 50730,
    BaselineNoise = 50731,
    BaselineSharpness = 50732,
    BayerGreenSplit = 50733,
    LinearResponseLimit = 50734,
    CameraSerialNumber = 50735,
    LensInfo = 50736,
    ChromaBlurRadius = 50737,
    AntiAliasStrength = 50738,
    ShadowScale = 50739,
    DNGPrivateData = 50740,
    MakerNoteSafety = 50741,
    CalibrationIlluminant1 = 50778,
    CalibrationIlluminant2 = 50779,
    BestQualityScale = 50780,
    RawDataUniqueID = 50781,
    AliasLayerMetadata = 50784,
    OriginalRawFileName = 50827,
    OriginalRawFileData = 50828,
    ActiveArea = 50829,
    MaskedAreas = 50830,
    AsShotICCProfile = 50831,
    AsShotPreProfileMatrix = 50832,
    CurrentICCProfile = 50833,
    CurrentPreProfileMatrix = 50834,
    ColorimetricReference = 50879,
    CameraCalibrationSignature = 50931,
    ProfileCalibrationSignature = 50932,
    ExtraCameraProfiles = 50933,
    AsShotProfileName = 50934,
    NoiseReductionApplied = 50935,
    ProfileName = 50936,
    ProfileHueSatMapDims = 50937,
    ProfileHueSatMapData1 = 50938,
    ProfileHueSatMapData2 = 50939,
    ProfileToneCurve = 50940,
    ProfileEmbedPolicy = 50941,
    ProfileCopyright = 50942,
    ForwardMatrix1 = 50964,
    ForwardMatrix2 = 50965,
    PreviewApplicationName = 50966,
    PreviewApplicationVersion = 50967,
    PreviewSettingsName = 50968,
    PreviewSettingsDigest = 50969,
    PreviewColorSpace = 50970,
    PreviewDateTime = 50971,
    RawImageDigest = 50972,
    OriginalRawFileDigest = 50973,
    SubTileBlockSize = 50974,
    RowInterleaveFactor = 50975,
    ProfileLookTableDims = 50981,
    ProfileLookTableData = 50982,
    OpcodeList1 = 51008,
    OpcodeList2 = 51009,
    OpcodeList3 = 51022,
    NoiseProfile = 51041,
    OriginalDefaultFinalSize = 51089,
    OriginalBestQualityFinalSize = 51090,
    OriginalDefaultCropSize = 51091,
    ProfileHueSatMapEncoding = 51107,
    ProfileLookTableEncoding = 51108,
    BaselineExposureOffset = 51109,
    DefaultBlackRender = 51110,
    NewRawImageDigest = 51111,
    RawToPreviewGain = 51112,
    DefaultUserCrop = 51125,
}

#[derive(Debug, Clone, Copy)]
pub enum Compression {
    None = 1,
    CCITT1D = 2,
    Group3Fax = 3,
    Group4Fax = 4,
    LZW = 5,
    JPEG = 6,
    PackBits = 32773,
}

impl TryFrom<u16> for Compression {
    type Error = VexelError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::None),
            2 => Ok(Self::CCITT1D),
            3 => Ok(Self::Group3Fax),
            4 => Ok(Self::Group4Fax),
            5 => Ok(Self::LZW),
            6 => Ok(Self::JPEG),
            32773 => Ok(Self::PackBits),
            _ => Err(VexelError::Custom(format!("Invalid compression value: {}", value)))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PhotometricInterpretation {
    WhiteIsZero = 0,      // For bilevel and grayscale images: 0 is white
    BlackIsZero = 1,      // For bilevel and grayscale images: 0 is black
    RGB = 2,              // RGB color model
    Palette = 3,          // Color map indexed
    TransparencyMask = 4, // Transparency mask
    CMYK = 5,            // CMYK color model
    YCbCr = 6,           // YCbCr color model
    CIELab = 8,          // CIE L*a*b* color model
}

impl TryFrom<u32> for PhotometricInterpretation {
    type Error = VexelError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::WhiteIsZero),
            1 => Ok(Self::BlackIsZero),
            2 => Ok(Self::RGB),
            3 => Ok(Self::Palette),
            4 => Ok(Self::TransparencyMask),
            5 => Ok(Self::CMYK),
            6 => Ok(Self::YCbCr),
            8 => Ok(Self::CIELab),
            _ => Err(VexelError::Custom(format!("Invalid photometric interpretation value: {}", value)))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolutionUnit {
    NoUnit = 1,
    Inch = 2,
    Centimeter = 3,
}

impl TryFrom<u32> for ResolutionUnit {
    type Error = VexelError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::NoUnit),
            2 => Ok(Self::Inch),
            3 => Ok(Self::Centimeter),
            _ => Err(VexelError::Custom(format!("Invalid resolution unit value: {}", value)))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Orientation {
    TopLeft = 1,
    TopRight = 2,
    BottomRight = 3,
    BottomLeft = 4,
    LeftTop = 5,
    RightTop = 6,
    RightBottom = 7,
    LeftBottom = 8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlanarConfiguration {
    Chunky = 1,  // RGB RGB RGB ...
    Planar = 2,  // RRR... GGG... BBB...
}

impl TryFrom<u32> for PlanarConfiguration {
    type Error = VexelError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Chunky),
            2 => Ok(Self::Planar),
            _ => Err(VexelError::Custom(format!("Invalid planar configuration value: {}", value)))
        }
    }
}

#[derive(Debug)]
struct TiffHeader {
    image_width: u32,
    image_length: u32,
    bits_per_sample: Vec<u16>,
    compression: Compression,
    photometric_interpretation: PhotometricInterpretation,
    strip_offsets: Vec<u32>,
    samples_per_pixel: u16,
    rows_per_strip: u32,
    strip_byte_counts: Vec<u32>,
    x_resolution: f32,
    y_resolution: f32,
    planar_configuration: PlanarConfiguration,
    resolution_unit: ResolutionUnit,
}

impl Default for TiffHeader {
    fn default() -> Self {
        Self {
            image_width: 0,
            image_length: 0,
            bits_per_sample: vec![1],
            compression: Compression::None,
            photometric_interpretation: PhotometricInterpretation::WhiteIsZero,
            strip_offsets: Vec::new(),
            samples_per_pixel: 1,
            rows_per_strip: u32::MAX,
            strip_byte_counts: Vec::new(),
            x_resolution: 0.0,
            y_resolution: 0.0,
            planar_configuration: PlanarConfiguration::Chunky,
            resolution_unit: ResolutionUnit::Inch,
        }
    }
}

fn read_single_value<T, R: Read + Seek>(type_: u16, value_offset: u32, reader: &mut BitReader<R>) -> VexelResult<T>
where
    T: TryFrom<u32>,
{
    let value = match type_ {
        1 => value_offset & 0xFF,
        3 => value_offset & 0xFFFF,
        4 => value_offset,
        _ => {
            reader.seek(SeekFrom::Start(value_offset as u64))?;
            reader.read_u32()?
        }
    };

    T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))
}

fn read_multiple_values<T, R: Read + Seek>(
    type_: u16,
    count: u32,
    value_offset: u32,
    reader: &mut BitReader<R>,
) -> VexelResult<Vec<T>>
where
    T: TryFrom<u32>,
{
    let mut values = Vec::with_capacity(count as usize);

    if count == 1 {
        values.push(read_single_value(type_, value_offset, reader)?);
        return Ok(values);
    }

    reader.seek(SeekFrom::Start(value_offset as u64))?;

    for _ in 0..count {
        let value = match type_ {
            1 => reader.read_u8()? as u32,
            3 => reader.read_u16()? as u32,
            4 => reader.read_u32()?,
            _ => return Err(VexelError::Custom("Unsupported type".to_string()))
        };

        values.push(T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
    }

    Ok(values)
}

fn read_rational<R: Read + Seek>(value_offset: u32, reader: &mut BitReader<R>) -> VexelResult<f32> {
    reader.seek(SeekFrom::Start(value_offset as u64))?;
    let numerator = reader.read_u32()?;
    let denominator = reader.read_u32()?;

    if denominator == 0 {
        return Err(VexelError::Custom("Division by zero in rational".to_string()));
    }

    Ok(numerator as f32 / denominator as f32)
}

pub struct TiffDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    header: TiffHeader,
    reader: BitReader<R>,
}

impl<R: Read + Seek> TiffDecoder<R> {
    pub fn new(reader: R) -> TiffDecoder<R> {
        TiffDecoder {
            width: 0,
            height: 0,
            header: TiffHeader::default(),
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn read_value<T>(&mut self, type_: u16, count: u32, value_offset: u32) -> VexelResult<Vec<T>>
    where
        T: TryFrom<u32>,
    {
        // Calculate total bytes needed
        let bytes_per_value = match type_ {
            1 | 2 => 1,
            3 => 2,
            4 => 4,
            5 => 8,
            _ => return Err(VexelError::Custom(format!("Unsupported type: {}", type_)))
        };

        let total_bytes = bytes_per_value * count;
        let mut values = Vec::with_capacity(count as usize);

        // If total size is 4 bytes or fewer, values are stored in value_offset
        if total_bytes <= 4 {
            match type_ {
                1 => {
                    for i in 0..count {
                        let value = (value_offset >> (i * 8)) & 0xFF;
                        values.push(T::try_from(value)
                            .map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
                    }
                }
                3 => {
                    for i in 0..count {
                        let value = (value_offset >> (i * 16)) & 0xFFFF;
                        values.push(T::try_from(value)
                            .map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
                    }
                }
                4 => {
                    values.push(T::try_from(value_offset)
                        .map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
                }
                _ => return Err(VexelError::Custom("Invalid type for inline value".to_string()))
            }
        } else {
            // Values are stored at the offset
            self.reader.seek(SeekFrom::Start(value_offset as u64))?;

            for _ in 0..count {
                let value = match type_ {
                    1 => self.reader.read_u8()? as u32,
                    3 => self.reader.read_u16()? as u32,
                    4 => self.reader.read_u32()?,
                    5 => {
                        let numerator = self.reader.read_u32()?;
                        let denominator = self.reader.read_u32()?;
                        if denominator == 0 {
                            return Err(VexelError::Custom("Division by zero in rational".to_string()));
                        }
                        numerator
                    }
                    _ => return Err(VexelError::Custom("Unsupported type".to_string()))
                };

                values.push(T::try_from(value)
                    .map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
            }
        }

        Ok(values)
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let mut byte_order_marker = [0u8; 2];
        self.reader.read_exact(&mut byte_order_marker)?;

        let byte_order = match &byte_order_marker {
            b"II" => ByteOrder::LittleEndian,
            b"MM" => ByteOrder::BigEndian,
            _ => return Err(VexelError::Custom("Invalid byte order marker".to_string())),
        };

        self.reader.set_endianness(byte_order);

        let magic = self.reader.read_u16()?;
        if magic != 42 {
            return Err(VexelError::Custom("Not a TIFF file".to_string()));
        }

        let ifd_offset = self.reader.read_u32()?;

        self.reader.seek(SeekFrom::Start(ifd_offset as u64))?;

        let num_entries = self.reader.read_u16()?;

        for _ in 0..num_entries {
            let tag = self.reader.read_u16()?;
            let type_ = self.reader.read_u16()?;
            let count = self.reader.read_u32()?;
            let value_offset = self.reader.read_u32()?;

            let current_pos = self.reader.stream_position()?;

            match tag {
                256 => self.header.image_width = read_single_value(type_, value_offset, &mut self.reader)?,
                257 => self.header.image_length = read_single_value(type_, value_offset, &mut self.reader)?,
                258 => self.header.bits_per_sample = read_multiple_values(type_, count, value_offset, &mut self.reader)?,
                259 => self.header.compression = match read_single_value(type_, value_offset, &mut self.reader)? {
                    1 => Compression::None,
                    2 => Compression::CCITT1D,
                    3 => Compression::Group3Fax,
                    4 => Compression::Group4Fax,
                    5 => Compression::LZW,
                    6 => Compression::JPEG,
                    32773 => Compression::PackBits,
                    _ => Compression::None,
                },
                262 => self.header.photometric_interpretation = read_single_value(type_, value_offset, &mut self.reader)?,
                273 => self.header.strip_offsets = read_multiple_values(type_, count, value_offset, &mut self.reader)?,
                277 => self.header.samples_per_pixel = read_single_value(type_, value_offset, &mut self.reader)?,
                278 => self.header.rows_per_strip = read_single_value(type_, value_offset, &mut self.reader)?,
                279 => self.header.strip_byte_counts = read_multiple_values(type_, count, value_offset, &mut self.reader)?,
                282 => self.header.x_resolution = read_rational(value_offset, &mut self.reader)?,
                283 => self.header.y_resolution = read_rational(value_offset, &mut self.reader)?,
                284 => self.header.planar_configuration = read_single_value(type_, value_offset, &mut self.reader)?,
                296 => self.header.resolution_unit = read_single_value(type_, value_offset, &mut self.reader)?,
                _ => {}
            }

            self.reader.seek(SeekFrom::Start(current_pos))?;
        }

        self.width = self.header.image_width;
        self.height = self.header.image_length;

        Ok(())
    }

    fn convert_to_pixel_data(&self, data: Vec<u8>, header: &TiffHeader) -> VexelResult<PixelData> {
        match header.photometric_interpretation {
            PhotometricInterpretation::WhiteIsZero | PhotometricInterpretation::BlackIsZero => {
                // 0 = WhiteIsZero, 1 = BlackIsZero (Grayscale)
                match (header.samples_per_pixel, header.bits_per_sample.get(0).copied().unwrap_or(1)) {
                    (1, 1) => Ok(PixelData::L1(data)),
                    (1, 8) => Ok(PixelData::L8(data)),
                    (1, 16) => {
                        let pixels = data.chunks_exact(2)
                            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                            .collect();
                        Ok(PixelData::L16(pixels))
                    }
                    (2, 8) => Ok(PixelData::LA8(data)),  // Grayscale with alpha
                    (2, 16) => {
                        let pixels = data.chunks_exact(4)
                            .map(|chunk| {
                                let gray = u16::from_le_bytes([chunk[0], chunk[1]]);
                                let alpha = u16::from_le_bytes([chunk[2], chunk[3]]);
                                vec![gray, alpha]
                            })
                            .flatten()
                            .collect();
                        Ok(PixelData::LA16(pixels))
                    }
                    _ => Err(VexelError::Custom(format!(
                        "Unsupported grayscale format: {} samples with {} bits",
                        header.samples_per_pixel,
                        header.bits_per_sample.get(0).unwrap_or(&0)
                    )))
                }
            }
            PhotometricInterpretation::RGB => {
                // RGB
                match (header.samples_per_pixel, header.bits_per_sample.get(0).copied().unwrap_or(8)) {
                    (3, 8) => Ok(PixelData::RGB8(data)),
                    (4, 8) => Ok(PixelData::RGBA8(data)),
                    (3, 16) => {
                        let pixels = data.chunks_exact(6)
                            .map(|chunk| {
                                vec![
                                    u16::from_le_bytes([chunk[0], chunk[1]]),
                                    u16::from_le_bytes([chunk[2], chunk[3]]),
                                    u16::from_le_bytes([chunk[4], chunk[5]]),
                                ]
                            })
                            .flatten()
                            .collect();
                        Ok(PixelData::RGB16(pixels))
                    }
                    (4, 16) => {
                        let pixels = data.chunks_exact(8)
                            .map(|chunk| {
                                vec![
                                    u16::from_le_bytes([chunk[0], chunk[1]]),
                                    u16::from_le_bytes([chunk[2], chunk[3]]),
                                    u16::from_le_bytes([chunk[4], chunk[5]]),
                                    u16::from_le_bytes([chunk[6], chunk[7]]),
                                ]
                            })
                            .flatten()
                            .collect();
                        Ok(PixelData::RGBA16(pixels))
                    }
                    _ => Err(VexelError::Custom(format!(
                        "Unsupported RGB format: {} samples with {} bits",
                        header.samples_per_pixel,
                        header.bits_per_sample.get(0).unwrap_or(&0)
                    )))
                }
            }
            PhotometricInterpretation::Palette => {
                unimplemented!();
            }
            PhotometricInterpretation::TransparencyMask => {
                // Mask (transparency mask)
                Ok(PixelData::L1(data))
            }
            PhotometricInterpretation::CMYK => {
                // Separated (usually CMYK)
                unimplemented!();
            }
            PhotometricInterpretation::YCbCr => {
                // YCbCr
                unimplemented!();
            }
            PhotometricInterpretation::CIELab => {
                // CIELab
                unimplemented!();
            }
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_header()?;

        let mut bytes = Vec::new();

        for (offset, byte_count) in self.header.strip_offsets.iter().zip(self.header.strip_byte_counts.iter()) {
            self.reader.seek(SeekFrom::Start(*offset as u64))?;

            let mut strip_data = vec![0u8; *byte_count as usize];
            self.reader.read_exact(&mut strip_data)?;

            let decompressed = match self.header.compression {
                Compression::None => strip_data,
                _ => return Err(VexelError::Custom("Unsupported compression".to_string()))
            };

            bytes.extend_from_slice(&decompressed);
        }

        let mut pixel_data = self.convert_to_pixel_data(bytes, &self.header)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
