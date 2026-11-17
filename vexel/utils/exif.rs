use std::io::{Cursor, Read, Seek, SeekFrom};

use serde::Serialize;

use crate::utils::bytes::u64_at;
use crate::utils::info::InfoWriter;
use crate::utils::limits::Limits;
use crate::utils::stream::StreamReader;
use crate::utils::tiff::ifd::{read_file_header, type_size, Ifd, IfdCycleGuard, IfdEntry, IfdFormat};
use crate::utils::types::ByteOrder;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[non_exhaustive]
pub enum ExifValue {
    Byte(Vec<u8>),
    Ascii(Vec<String>),
    Short(Vec<u16>),
    Long(Vec<u32>),
    Rational(Vec<(u32, u32)>),
    SByte(Vec<i8>),
    Undefined(Vec<u8>),
    SShort(Vec<i16>),
    SLong(Vec<i32>),
    Long8(Vec<u64>),
    SLong8(Vec<i64>),
    SRational(Vec<(i32, i32)>),
    Float(Vec<f32>),
    Double(Vec<f64>),
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(missing_as_null))]
pub struct ExifEntry {
    pub tag: u16,
    pub tag_name: Option<String>,
    pub value: ExifValue,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct ExifIfd {
    pub entries: Vec<ExifEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(missing_as_null))]
pub struct ExifData {
    pub byte_order: ByteOrder,
    pub ifd0: ExifIfd,
    pub exif_ifd: Option<ExifIfd>,
    pub gps_ifd: Option<ExifIfd>,
    pub interop_ifd: Option<ExifIfd>,
    pub ifd1: Option<ExifIfd>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum IfdType {
    Ifd0,
    ExifIfd,
    GpsIfd,
    InteropIfd,
    Ifd1,
}

const EXIF_IFD_POINTER: u16 = 0x8769;
const GPS_IFD_POINTER: u16 = 0x8825;
const INTEROP_IFD_POINTER: u16 = 0xA005;
const MAX_IFDS: usize = 8;

pub(crate) struct ExifReader;

impl ExifReader {
    pub fn parse(tiff_data: &[u8]) -> Option<ExifData> {
        let mut cursor = Cursor::new(tiff_data);
        let mut reader = StreamReader::new(&mut cursor);
        let header = read_file_header(&mut reader).ok()?;
        let byte_order = header.byte_order;
        let format = header.format;

        let mut limits = Limits {
            max_alloc: Some((tiff_data.len() as u64).saturating_mul(2).saturating_add(4096)),
            ..Limits::no_limits()
        };
        let mut guard = IfdCycleGuard::new(MAX_IFDS);

        let ifd0_raw = read_guarded(&mut reader, header.first_ifd_offset, format, &mut limits, &mut guard)?;
        let ifd1_raw = read_guarded(&mut reader, ifd0_raw.next_ifd_offset, format, &mut limits, &mut guard);

        let ifd0 = materialize_ifd(&ifd0_raw, IfdType::Ifd0, &mut reader, &mut limits);
        let ifd1 = ifd1_raw.map(|raw| materialize_ifd(&raw, IfdType::Ifd1, &mut reader, &mut limits));

        let mut exif_ifd = None;
        let mut gps_ifd = None;
        for ifd in [Some(&ifd0), ifd1.as_ref()].into_iter().flatten() {
            if exif_ifd.is_none() {
                exif_ifd =
                    load_sub_ifd(ifd, EXIF_IFD_POINTER, IfdType::ExifIfd, format, &mut reader, &mut limits, &mut guard);
            }
            if gps_ifd.is_none() {
                gps_ifd =
                    load_sub_ifd(ifd, GPS_IFD_POINTER, IfdType::GpsIfd, format, &mut reader, &mut limits, &mut guard);
            }
        }

        let mut interop_ifd = None;
        for ifd in [exif_ifd.as_ref(), Some(&ifd0), ifd1.as_ref()].into_iter().flatten() {
            if interop_ifd.is_none() {
                interop_ifd = load_sub_ifd(
                    ifd,
                    INTEROP_IFD_POINTER,
                    IfdType::InteropIfd,
                    format,
                    &mut reader,
                    &mut limits,
                    &mut guard,
                );
            }
        }

        Some(ExifData { byte_order, ifd0, exif_ifd, gps_ifd, interop_ifd, ifd1 })
    }
}

pub(crate) fn exif_thumbnail(tiff_data: &[u8]) -> Option<&[u8]> {
    const JPEG_INTERCHANGE_FORMAT: u16 = 0x0201;
    const JPEG_INTERCHANGE_FORMAT_LENGTH: u16 = 0x0202;

    let exif = ExifReader::parse(tiff_data)?;
    let ifd1 = exif.ifd1?;

    let value_u32 = |tag: u16| {
        ifd1.entries.iter().find(|e| e.tag == tag).and_then(|e| match &e.value {
            ExifValue::Long(v) => v.first().copied(),
            ExifValue::Short(v) => v.first().map(|&v| v as u32),
            _ => None,
        })
    };

    let offset = value_u32(JPEG_INTERCHANGE_FORMAT)? as usize;
    let length = value_u32(JPEG_INTERCHANGE_FORMAT_LENGTH)? as usize;

    tiff_data.get(offset..offset.checked_add(length)?)
}

fn read_guarded<R: Read + Seek>(
    reader: &mut StreamReader<R>,
    offset: u64,
    format: IfdFormat,
    limits: &mut Limits,
    guard: &mut IfdCycleGuard,
) -> Option<Ifd> {
    if !guard.enter(offset) {
        return None;
    }
    Ifd::read(reader, offset, format, limits).ok()
}

fn load_sub_ifd<R: Read + Seek>(
    parent: &ExifIfd,
    pointer_tag: u16,
    ifd_type: IfdType,
    format: IfdFormat,
    reader: &mut StreamReader<R>,
    limits: &mut Limits,
    guard: &mut IfdCycleGuard,
) -> Option<ExifIfd> {
    let entry = parent.entries.iter().find(|e| e.tag == pointer_tag)?;
    let offset = ifd_pointer_offset(&entry.value)?;
    let raw = read_guarded(reader, offset, format, limits, guard)?;
    Some(materialize_ifd(&raw, ifd_type, reader, limits))
}

fn materialize_ifd<R: Read + Seek>(
    ifd: &Ifd,
    ifd_type: IfdType,
    reader: &mut StreamReader<R>,
    limits: &mut Limits,
) -> ExifIfd {
    let mut entries = Vec::new();
    for entry in &ifd.entries {
        if let Some(e) = materialize_entry(ifd, entry, ifd_type, reader, limits) {
            entries.push(e);
        }
    }
    ExifIfd { entries }
}

fn materialize_entry<R: Read + Seek>(
    ifd: &Ifd,
    entry: &IfdEntry,
    ifd_type: IfdType,
    reader: &mut StreamReader<R>,
    limits: &mut Limits,
) -> Option<ExifEntry> {
    let size = type_size(entry.field_type)?;
    let count = entry.count as usize;
    let total = entry.count.saturating_mul(size as u64);

    let bytes = if total <= ifd.format.inline_capacity() as u64 {
        ifd.inline_bytes(entry)[..total as usize].to_vec()
    } else {
        let total = usize::try_from(total).ok()?;
        limits.reserve_usize(total).ok()?;
        let read = reader.seek(SeekFrom::Start(entry.value_offset)).and_then(|_| reader.read_bytes(total));
        match read {
            Ok(bytes) => bytes,
            Err(_) => {
                limits.free_usize(total);
                return None;
            }
        }
    };

    let value = parse_value(entry.field_type, count, &bytes, ifd.byte_order);
    let tag_name = tag_name(entry.tag, ifd_type).map(|s| s.to_string());

    Some(ExifEntry { tag: entry.tag, tag_name, value })
}

pub(crate) fn parse_value(data_type: u16, count: usize, bytes: &[u8], byte_order: ByteOrder) -> ExifValue {
    match data_type {
        1 => ExifValue::Byte(bytes.iter().take(count).copied().collect()),
        2 | 129 => {
            let strings = bytes[..count.min(bytes.len())]
                .split(|&b| b == 0)
                .filter(|chunk| !chunk.is_empty())
                .map(|chunk| match std::str::from_utf8(chunk) {
                    Ok(text) => text.to_string(),
                    Err(e) if e.error_len().is_none() && e.valid_up_to() > 0 => {
                        String::from_utf8_lossy(&chunk[..e.valid_up_to()]).into_owned()
                    }
                    Err(_) if data_type == 129 => String::from_utf8_lossy(chunk).into_owned(),
                    Err(_) => chunk.iter().map(|&b| b as char).collect(),
                })
                .collect();
            ExifValue::Ascii(strings)
        }
        3 => ExifValue::Short(
            (0..count)
                .filter_map(|i| {
                    let b = bytes.get(i * 2..i * 2 + 2)?;
                    Some(match byte_order {
                        ByteOrder::LittleEndian => u16::from_le_bytes([b[0], b[1]]),
                        ByteOrder::BigEndian => u16::from_be_bytes([b[0], b[1]]),
                    })
                })
                .collect(),
        ),
        4 | 13 => ExifValue::Long(
            (0..count)
                .filter_map(|i| {
                    let b = bytes.get(i * 4..i * 4 + 4)?;
                    Some(match byte_order {
                        ByteOrder::LittleEndian => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                        ByteOrder::BigEndian => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                    })
                })
                .collect(),
        ),
        5 => ExifValue::Rational(
            (0..count)
                .filter_map(|i| {
                    let b = bytes.get(i * 8..i * 8 + 8)?;
                    let num = match byte_order {
                        ByteOrder::LittleEndian => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                        ByteOrder::BigEndian => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                    };
                    let den = match byte_order {
                        ByteOrder::LittleEndian => u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                        ByteOrder::BigEndian => u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
                    };
                    Some((num, den))
                })
                .collect(),
        ),
        6 => ExifValue::SByte(bytes.iter().take(count).map(|&b| b as i8).collect()),
        7 => ExifValue::Undefined(bytes.iter().take(count).copied().collect()),
        8 => ExifValue::SShort(
            (0..count)
                .filter_map(|i| {
                    let b = bytes.get(i * 2..i * 2 + 2)?;
                    Some(match byte_order {
                        ByteOrder::LittleEndian => i16::from_le_bytes([b[0], b[1]]),
                        ByteOrder::BigEndian => i16::from_be_bytes([b[0], b[1]]),
                    })
                })
                .collect(),
        ),
        9 => ExifValue::SLong(
            (0..count)
                .filter_map(|i| {
                    let b = bytes.get(i * 4..i * 4 + 4)?;
                    Some(match byte_order {
                        ByteOrder::LittleEndian => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                        ByteOrder::BigEndian => i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                    })
                })
                .collect(),
        ),
        10 => ExifValue::SRational(
            (0..count)
                .filter_map(|i| {
                    let b = bytes.get(i * 8..i * 8 + 8)?;
                    let num = match byte_order {
                        ByteOrder::LittleEndian => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                        ByteOrder::BigEndian => i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                    };
                    let den = match byte_order {
                        ByteOrder::LittleEndian => i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                        ByteOrder::BigEndian => i32::from_be_bytes([b[4], b[5], b[6], b[7]]),
                    };
                    Some((num, den))
                })
                .collect(),
        ),
        11 => ExifValue::Float(
            (0..count)
                .filter_map(|i| {
                    let b = bytes.get(i * 4..i * 4 + 4)?;
                    let bits = match byte_order {
                        ByteOrder::LittleEndian => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                        ByteOrder::BigEndian => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                    };
                    Some(f32::from_bits(bits))
                })
                .collect(),
        ),
        12 => ExifValue::Double(
            (0..count)
                .filter_map(|i| {
                    let b = bytes.get(i * 8..i * 8 + 8)?;
                    let bits = match byte_order {
                        ByteOrder::LittleEndian => u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
                        ByteOrder::BigEndian => u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
                    };
                    Some(f64::from_bits(bits))
                })
                .collect(),
        ),
        16 | 18 => ExifValue::Long8((0..count).filter_map(|i| u64_at(bytes, i * 8, byte_order)).collect()),
        17 => {
            ExifValue::SLong8((0..count).filter_map(|i| u64_at(bytes, i * 8, byte_order).map(|v| v as i64)).collect())
        }
        _ => ExifValue::Undefined(bytes.iter().take(count).copied().collect()),
    }
}

fn ifd_pointer_offset(value: &ExifValue) -> Option<u64> {
    match value {
        ExifValue::Long(v) => v.first().map(|&x| u64::from(x)),
        ExifValue::Short(v) => v.first().map(|&x| u64::from(x)),
        ExifValue::Long8(v) => v.first().copied(),
        _ => None,
    }
}

pub(crate) fn tag_name(tag: u16, ifd_type: IfdType) -> Option<&'static str> {
    match ifd_type {
        IfdType::GpsIfd => match tag {
            0x0000 => Some("GPSVersionID"),
            0x0001 => Some("GPSLatitudeRef"),
            0x0002 => Some("GPSLatitude"),
            0x0003 => Some("GPSLongitudeRef"),
            0x0004 => Some("GPSLongitude"),
            0x0005 => Some("GPSAltitudeRef"),
            0x0006 => Some("GPSAltitude"),
            0x0007 => Some("GPSTimeStamp"),
            0x0008 => Some("GPSSatellites"),
            0x0009 => Some("GPSStatus"),
            0x000A => Some("GPSMeasureMode"),
            0x000B => Some("GPSDOP"),
            0x000C => Some("GPSSpeedRef"),
            0x000D => Some("GPSSpeed"),
            0x000E => Some("GPSTrackRef"),
            0x000F => Some("GPSTrack"),
            0x0010 => Some("GPSImgDirectionRef"),
            0x0011 => Some("GPSImgDirection"),
            0x0012 => Some("GPSMapDatum"),
            0x0013 => Some("GPSDestLatitudeRef"),
            0x0014 => Some("GPSDestLatitude"),
            0x0015 => Some("GPSDestLongitudeRef"),
            0x0016 => Some("GPSDestLongitude"),
            0x0017 => Some("GPSDestBearingRef"),
            0x0018 => Some("GPSDestBearing"),
            0x0019 => Some("GPSDestDistanceRef"),
            0x001A => Some("GPSDestDistance"),
            0x001B => Some("GPSProcessingMethod"),
            0x001C => Some("GPSAreaInformation"),
            0x001D => Some("GPSDateStamp"),
            0x001E => Some("GPSDifferential"),
            0x001F => Some("GPSHPositioningError"),
            _ => None,
        },
        IfdType::ExifIfd => match tag {
            0x829A => Some("ExposureTime"),
            0x829D => Some("FNumber"),
            0x8822 => Some("ExposureProgram"),
            0x8824 => Some("SpectralSensitivity"),
            0x8827 => Some("ISOSpeedRatings"),
            0x8828 => Some("OECF"),
            0x882A => Some("TimeZoneOffset"),
            0x8830 => Some("SensitivityType"),
            0x8831 => Some("StandardOutputSensitivity"),
            0x8832 => Some("RecommendedExposureIndex"),
            0x8833 => Some("ISOSpeed"),
            0x8834 => Some("ISOSpeedLatitudeyyy"),
            0x8835 => Some("ISOSpeedLatitudezzz"),
            0x9000 => Some("ExifVersion"),
            0x9003 => Some("DateTimeOriginal"),
            0x9004 => Some("DateTimeDigitized"),
            0x9010 => Some("OffsetTime"),
            0x9011 => Some("OffsetTimeOriginal"),
            0x9012 => Some("OffsetTimeDigitized"),
            0x9101 => Some("ComponentsConfiguration"),
            0x9102 => Some("CompressedBitsPerPixel"),
            0x9201 => Some("ShutterSpeedValue"),
            0x9202 => Some("ApertureValue"),
            0x9203 => Some("BrightnessValue"),
            0x9204 => Some("ExposureBiasValue"),
            0x9205 => Some("MaxApertureValue"),
            0x9206 => Some("SubjectDistance"),
            0x9207 => Some("MeteringMode"),
            0x9208 => Some("LightSource"),
            0x9209 => Some("Flash"),
            0x920A => Some("FocalLength"),
            0x9214 => Some("SubjectArea"),
            0x9216 => Some("TIFFEPStandardID"),
            0x927C => Some("MakerNote"),
            0x9286 => Some("UserComment"),
            0x9290 => Some("SubSecTime"),
            0x9291 => Some("SubSecTimeOriginal"),
            0x9292 => Some("SubSecTimeDigitized"),
            0x9C9B => Some("XPTitle"),
            0x9C9C => Some("XPComment"),
            0x9C9D => Some("XPAuthor"),
            0x9C9E => Some("XPKeywords"),
            0x9C9F => Some("XPSubject"),
            0xA000 => Some("FlashPixVersion"),
            0xA001 => Some("ColorSpace"),
            0xA002 => Some("PixelXDimension"),
            0xA003 => Some("PixelYDimension"),
            0xA004 => Some("RelatedSoundFile"),
            0xA005 => Some("InteroperabilityIFDPointer"),
            0xA20B => Some("FlashEnergy"),
            0xA20C => Some("SpatialFrequencyResponse"),
            0xA20E => Some("FocalPlaneXResolution"),
            0xA20F => Some("FocalPlaneYResolution"),
            0xA210 => Some("FocalPlaneResolutionUnit"),
            0xA214 => Some("SubjectLocation"),
            0xA215 => Some("ExposureIndex"),
            0xA217 => Some("SensingMethod"),
            0xA300 => Some("FileSource"),
            0xA301 => Some("SceneType"),
            0xA302 => Some("CFAPattern"),
            0xA401 => Some("CustomRendered"),
            0xA402 => Some("ExposureMode"),
            0xA403 => Some("WhiteBalance"),
            0xA404 => Some("DigitalZoomRatio"),
            0xA405 => Some("FocalLengthIn35mmFilm"),
            0xA406 => Some("SceneCaptureType"),
            0xA407 => Some("GainControl"),
            0xA408 => Some("Contrast"),
            0xA409 => Some("Saturation"),
            0xA40A => Some("Sharpness"),
            0xA40B => Some("DeviceSettingDescription"),
            0xA40C => Some("SubjectDistanceRange"),
            0xA420 => Some("ImageUniqueID"),
            0xA430 => Some("CameraOwnerName"),
            0xA431 => Some("BodySerialNumber"),
            0xA432 => Some("LensSpecification"),
            0xA433 => Some("LensMake"),
            0xA434 => Some("LensModel"),
            0xA435 => Some("LensSerialNumber"),
            0xA460 => Some("CompositeImage"),
            0xA461 => Some("SourceImageNumberOfCompositeImage"),
            0xA462 => Some("SourceExposureTimesOfCompositeImage"),
            0xA500 => Some("Gamma"),
            _ => None,
        },
        IfdType::InteropIfd => match tag {
            0x0001 => Some("InteroperabilityIndex"),
            0x0002 => Some("InteroperabilityVersion"),
            0x1000 => Some("RelatedImageFileFormat"),
            0x1001 => Some("RelatedImageWidth"),
            0x1002 => Some("RelatedImageLength"),
            _ => None,
        },
        IfdType::Ifd0 | IfdType::Ifd1 => match tag {
            0x00FE => Some("NewSubfileType"),
            0x0100 => Some("ImageWidth"),
            0x0101 => Some("ImageLength"),
            0x0102 => Some("BitsPerSample"),
            0x0103 => Some("Compression"),
            0x0106 => Some("PhotometricInterpretation"),
            0x010A => Some("FillOrder"),
            0x010D => Some("DocumentName"),
            0x010E => Some("ImageDescription"),
            0x010F => Some("Make"),
            0x0110 => Some("Model"),
            0x0111 => Some("StripOffsets"),
            0x0112 => Some("Orientation"),
            0x0115 => Some("SamplesPerPixel"),
            0x0116 => Some("RowsPerStrip"),
            0x0117 => Some("StripByteCounts"),
            0x011A => Some("XResolution"),
            0x011B => Some("YResolution"),
            0x011C => Some("PlanarConfiguration"),
            0x0128 => Some("ResolutionUnit"),
            0x012D => Some("TransferFunction"),
            0x0131 => Some("Software"),
            0x0132 => Some("DateTime"),
            0x013B => Some("Artist"),
            0x013E => Some("WhitePoint"),
            0x013F => Some("PrimaryChromaticities"),
            0x014A => Some("SubIFDs"),
            0x0156 => Some("TransferRange"),
            0x0200 => Some("JPEGProc"),
            0x0201 => Some("JPEGInterchangeFormat"),
            0x0202 => Some("JPEGInterchangeFormatLength"),
            0x0211 => Some("YCbCrCoefficients"),
            0x0212 => Some("YCbCrSubSampling"),
            0x0213 => Some("YCbCrPositioning"),
            0x0214 => Some("ReferenceBlackWhite"),
            0x02BC => Some("XMLPacket"),
            0x80E5 => Some("ImageDepth"),
            0x828D => Some("CFARepeatPatternDim"),
            0x828E => Some("CFAPattern"),
            0x828F => Some("BatteryLevel"),
            0x8298 => Some("Copyright"),
            0x829A => Some("ExposureTime"),
            0x829D => Some("FNumber"),
            0x83BB => Some("IPTCNAA"),
            0x8649 => Some("ImageResources"),
            0x8769 => Some("ExifIFDPointer"),
            0x8773 => Some("InterColorProfile"),
            0x8822 => Some("ExposureProgram"),
            0x8824 => Some("SpectralSensitivity"),
            0x8825 => Some("GPSInfoIFDPointer"),
            0x8827 => Some("ISOSpeedRatings"),
            0x8828 => Some("OECF"),
            0x9C9B => Some("XPTitle"),
            0x9C9C => Some("XPComment"),
            0x9C9D => Some("XPAuthor"),
            0x9C9E => Some("XPKeywords"),
            0x9C9F => Some("XPSubject"),
            0xC4A5 => Some("PrintImageMatching"),
            0xEA1C => Some("Padding"),
            _ => None,
        },
    }
}

fn join_values<T: std::fmt::Display>(values: &[T]) -> String {
    values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")
}

fn join_rationals<T: std::fmt::Display>(values: &[(T, T)]) -> String {
    values.iter().map(|(n, d)| format!("{}/{}", n, d)).collect::<Vec<_>>().join(", ")
}

impl std::fmt::Display for ExifValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExifValue::Ascii(strings) => f.write_str(&strings.join(" / ")),
            ExifValue::Short(v) => f.write_str(&join_values(v)),
            ExifValue::Long(v) => f.write_str(&join_values(v)),
            ExifValue::Rational(v) => f.write_str(&join_rationals(v)),
            ExifValue::SByte(v) => f.write_str(&join_values(v)),
            ExifValue::SShort(v) => f.write_str(&join_values(v)),
            ExifValue::SLong(v) => f.write_str(&join_values(v)),
            ExifValue::Long8(v) => f.write_str(&join_values(v)),
            ExifValue::SLong8(v) => f.write_str(&join_values(v)),
            ExifValue::SRational(v) => f.write_str(&join_rationals(v)),
            ExifValue::Float(v) => f.write_str(&join_values(v)),
            ExifValue::Double(v) => f.write_str(&join_values(v)),
            ExifValue::Byte(v) => write!(f, "{} bytes", v.len()),
            ExifValue::Undefined(v) => write!(f, "{} bytes (undefined)", v.len()),
        }
    }
}

fn describe_ifd(name: &str, ifd: &ExifIfd, out: &mut InfoWriter) {
    out.group(format!("{} ({} entries)", name, ifd.entries.len()), |out| {
        for entry in &ifd.entries {
            let label = entry.tag_name.clone().unwrap_or_else(|| format!("0x{:04X}", entry.tag));
            out.text(&label, &entry.value.to_string());
        }
    });
}

impl ExifData {
    pub(crate) fn describe(&self, out: &mut InfoWriter) {
        let byte_order = match self.byte_order {
            ByteOrder::LittleEndian => "Little-endian (II)",
            ByteOrder::BigEndian => "Big-endian (MM)",
        };
        out.field("Byte order", byte_order);
        describe_ifd("IFD0", &self.ifd0, out);
        for (name, ifd) in [
            ("Exif IFD", &self.exif_ifd),
            ("GPS IFD", &self.gps_ifd),
            ("Interoperability IFD", &self.interop_ifd),
            ("IFD1 (thumbnail)", &self.ifd1),
        ] {
            if let Some(ifd) = ifd {
                describe_ifd(name, ifd, out);
            }
        }
    }
}

fn rational_value((numerator, denominator): (f64, f64)) -> Option<f64> {
    (denominator != 0.0).then(|| numerator / denominator)
}

fn trim_number(value: f64) -> String {
    let text = format!("{:.2}", value);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl ExifValue {
    pub fn first_number(&self) -> Option<u32> {
        match self {
            ExifValue::Byte(v) | ExifValue::Undefined(v) => v.first().map(|&x| u32::from(x)),
            ExifValue::Short(v) => v.first().map(|&x| u32::from(x)),
            ExifValue::Long(v) => v.first().copied(),
            ExifValue::Long8(v) => v.first().and_then(|&x| u32::try_from(x).ok()),
            _ => None,
        }
    }

    pub fn reals(&self) -> Vec<f64> {
        match self {
            ExifValue::Rational(v) => {
                v.iter().filter_map(|&(n, d)| rational_value((f64::from(n), f64::from(d)))).collect()
            }
            ExifValue::SRational(v) => {
                v.iter().filter_map(|&(n, d)| rational_value((f64::from(n), f64::from(d)))).collect()
            }
            ExifValue::Short(v) => v.iter().map(|&x| f64::from(x)).collect(),
            ExifValue::Long(v) => v.iter().map(|&x| f64::from(x)).collect(),
            ExifValue::Float(v) => v.iter().map(|&x| f64::from(x)).collect(),
            ExifValue::Double(v) => v.clone(),
            _ => Vec::new(),
        }
    }

    pub fn text(&self) -> Option<String> {
        let text = match self {
            ExifValue::Ascii(strings) => strings.join(" / "),
            ExifValue::Byte(bytes) | ExifValue::Undefined(bytes) => {
                let text = std::str::from_utf8(bytes).ok()?.trim_end_matches('\0');
                if text.chars().any(char::is_control) {
                    return None;
                }
                text.to_string()
            }
            _ => return None,
        };
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

impl ExifIfd {
    pub fn get(&self, tag: u16) -> Option<&ExifValue> {
        self.entries.iter().find(|entry| entry.tag == tag).map(|entry| &entry.value)
    }

    pub fn number(&self, tag: u16) -> Option<u32> {
        self.get(tag)?.first_number()
    }

    pub fn text(&self, tag: u16) -> Option<String> {
        self.get(tag)?.text()
    }

    pub fn reals(&self, tag: u16) -> Vec<f64> {
        self.get(tag).map(ExifValue::reals).unwrap_or_default()
    }
}

fn joined_name(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) if second.to_lowercase().starts_with(&first.to_lowercase()) => Some(second),
        (Some(first), Some(second)) => Some(format!("{} {}", first, second)),
        (first, second) => first.or(second),
    }
}

fn gps_coordinate(gps: &ExifIfd, value_tag: u16, ref_tag: u16) -> Option<String> {
    let parts = gps.reals(value_tag);
    let degrees = match parts[..] {
        [d, m, s, ..] => d + m / 60.0 + s / 3600.0,
        [d, m] => d + m / 60.0,
        [d] => d,
        [] => return None,
    };
    let reference = gps.text(ref_tag).unwrap_or_default();
    Some(format!("{:.6}° {}", degrees, reference).trim_end().to_string())
}

impl ExifData {
    pub fn ifds(&self) -> impl Iterator<Item = (&'static str, &ExifIfd)> {
        [
            ("IFD0", Some(&self.ifd0)),
            ("Exif", self.exif_ifd.as_ref()),
            ("GPS", self.gps_ifd.as_ref()),
            ("Interop", self.interop_ifd.as_ref()),
            ("IFD1", self.ifd1.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, ifd)| ifd.map(|ifd| (name, ifd)))
    }

    pub fn entry_count(&self) -> usize {
        self.ifds().map(|(_, ifd)| ifd.entries.len()).sum()
    }

    pub fn highlights(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        let exif = self.exif_ifd.as_ref();
        let exif_text = |tag: u16| exif.and_then(|ifd| ifd.text(tag));
        let exif_reals = |tag: u16| exif.map(|ifd| ifd.reals(tag)).unwrap_or_default();

        if let Some(camera) = joined_name(self.ifd0.text(0x010F), self.ifd0.text(0x0110)) {
            out.push(("Camera", camera));
        }
        if let Some(lens) = joined_name(exif_text(0xA433), exif_text(0xA434)) {
            out.push(("Lens", lens));
        }
        match exif_text(0x9003) {
            Some(date) => out.push(("Date taken", date)),
            None => {
                if let Some(date) = self.ifd0.text(0x0132) {
                    out.push(("Date", date));
                }
            }
        }

        let mut exposure = Vec::new();
        if let Some(&time) = exif_reals(0x829A).first() {
            if time > 0.0 && time < 1.0 {
                exposure.push(format!("1/{} s", trim_number(1.0 / time)));
            } else {
                exposure.push(format!("{} s", trim_number(time)));
            }
        }
        if let Some(&f_number) = exif_reals(0x829D).first() {
            exposure.push(format!("f/{}", trim_number(f_number)));
        }
        if let Some(iso) = exif.and_then(|ifd| ifd.number(0x8827)) {
            exposure.push(format!("ISO {}", iso));
        }
        if let Some(&focal) = exif_reals(0x920A).first() {
            exposure.push(format!("{} mm", trim_number(focal)));
        }
        if !exposure.is_empty() {
            out.push(("Exposure", exposure.join(", ")));
        }

        if let Some(gps) = &self.gps_ifd {
            let latitude = gps_coordinate(gps, 0x0002, 0x0001);
            let longitude = gps_coordinate(gps, 0x0004, 0x0003);
            if let (Some(latitude), Some(longitude)) = (latitude, longitude) {
                let mut position = format!("{}, {}", latitude, longitude);
                if let Some(&altitude) = gps.reals(0x0006).first() {
                    let below = gps.number(0x0005) == Some(1);
                    position = format!("{}, {} m", position, trim_number(if below { -altitude } else { altitude }));
                }
                out.push(("GPS position", position));
            }
        }

        if let Some(software) = self.ifd0.text(0x0131) {
            out.push(("Software", software));
        }

        out
    }
}
