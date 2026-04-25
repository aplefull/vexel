use crate::utils::types::ByteOrder;
use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum ExifValue {
    Byte(Vec<u8>),
    Ascii(String),
    Short(Vec<u16>),
    Long(Vec<u32>),
    Rational(Vec<(u32, u32)>),
    Undefined(Vec<u8>),
    SLong(Vec<i32>),
    SRational(Vec<(i32, i32)>),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExifEntry {
    pub tag: u16,
    pub tag_name: Option<String>,
    pub value: ExifValue,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExifIfd {
    pub entries: Vec<ExifEntry>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExifData {
    pub byte_order: ByteOrder,
    pub ifd0: ExifIfd,
    pub exif_ifd: Option<ExifIfd>,
    pub gps_ifd: Option<ExifIfd>,
    pub ifd1: Option<ExifIfd>,
}

#[derive(Clone, Copy, PartialEq)]
enum IfdType {
    Ifd0,
    ExifIfd,
    GpsIfd,
    Ifd1,
}

pub struct ExifReader<'a> {
    data: &'a [u8],
    byte_order: ByteOrder,
}

impl<'a> ExifReader<'a> {
    pub fn parse(tiff_data: &'a [u8]) -> Option<ExifData> {
        if tiff_data.len() < 8 {
            return None;
        }

        let byte_order = match &tiff_data[0..2] {
            b"II" => ByteOrder::LittleEndian,
            b"MM" => ByteOrder::BigEndian,
            _ => return None,
        };

        let reader = ExifReader { data: tiff_data, byte_order };

        let magic = reader.read_u16_at(2)?;
        if magic != 0x002A {
            return None;
        }

        let ifd0_offset = reader.read_u32_at(4)? as usize;
        let (ifd0, next_ifd_offset) = reader.read_ifd(ifd0_offset, IfdType::Ifd0);

        let exif_ifd = ifd0.entries.iter()
            .find(|e| e.tag == 0x8769)
            .and_then(|e| ifd_pointer_offset(&e.value))
            .map(|offset| reader.read_ifd(offset, IfdType::ExifIfd).0);

        let gps_ifd = ifd0.entries.iter()
            .find(|e| e.tag == 0x8825)
            .and_then(|e| ifd_pointer_offset(&e.value))
            .map(|offset| reader.read_ifd(offset, IfdType::GpsIfd).0);

        let ifd1 = next_ifd_offset
            .filter(|&o| o != 0)
            .map(|offset| reader.read_ifd(offset, IfdType::Ifd1).0);

        Some(ExifData { byte_order, ifd0, exif_ifd, gps_ifd, ifd1 })
    }

    fn read_u16_at(&self, offset: usize) -> Option<u16> {
        let bytes = self.data.get(offset..offset + 2)?;
        Some(match self.byte_order {
            ByteOrder::LittleEndian => u16::from_le_bytes([bytes[0], bytes[1]]),
            ByteOrder::BigEndian => u16::from_be_bytes([bytes[0], bytes[1]]),
        })
    }

    fn read_u32_at(&self, offset: usize) -> Option<u32> {
        let bytes = self.data.get(offset..offset + 4)?;
        Some(match self.byte_order {
            ByteOrder::LittleEndian => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            ByteOrder::BigEndian => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        })
    }

    fn read_ifd(&self, offset: usize, ifd_type: IfdType) -> (ExifIfd, Option<usize>) {
        let entry_count = match self.read_u16_at(offset) {
            Some(n) => n as usize,
            None => return (ExifIfd { entries: Vec::new() }, None),
        };

        let mut entries = Vec::with_capacity(entry_count);

        for i in 0..entry_count {
            let entry_base = offset + 2 + i * 12;
            if let Some(entry) = self.read_entry(entry_base, ifd_type) {
                entries.push(entry);
            }
        }

        let next_offset_pos = offset + 2 + entry_count * 12;
        let next_ifd_offset = self.read_u32_at(next_offset_pos).map(|v| v as usize);

        (ExifIfd { entries }, next_ifd_offset)
    }

    fn read_entry(&self, base: usize, ifd_type: IfdType) -> Option<ExifEntry> {
        let tag = self.read_u16_at(base)?;
        let data_type = self.read_u16_at(base + 2)?;
        let count = self.read_u32_at(base + 4)? as usize;

        let type_size: usize = match data_type {
            1 | 2 | 7 => 1,
            3 => 2,
            4 | 9 => 4,
            5 | 10 => 8,
            _ => 1,
        };

        let total_size = count.saturating_mul(type_size);

        let value_bytes: &[u8] = if total_size <= 4 {
            self.data.get(base + 8..base + 8 + total_size)?
        } else {
            let offset = self.read_u32_at(base + 8)? as usize;
            self.data.get(offset..offset + total_size)?
        };

        let value = self.parse_value(data_type, count, value_bytes);
        let tag_name = tag_name(tag, ifd_type).map(|s| s.to_string());

        Some(ExifEntry { tag, tag_name, value })
    }

    fn parse_value(&self, data_type: u16, count: usize, bytes: &[u8]) -> ExifValue {
        match data_type {
            1 => ExifValue::Byte(bytes.iter().take(count).copied().collect()),
            2 => {
                let s = bytes.iter().take(count)
                    .take_while(|&&b| b != 0)
                    .map(|&b| b as char)
                    .collect();
                ExifValue::Ascii(s)
            }
            3 => ExifValue::Short(
                (0..count)
                    .filter_map(|i| {
                        let b = bytes.get(i * 2..i * 2 + 2)?;
                        Some(match self.byte_order {
                            ByteOrder::LittleEndian => u16::from_le_bytes([b[0], b[1]]),
                            ByteOrder::BigEndian => u16::from_be_bytes([b[0], b[1]]),
                        })
                    })
                    .collect(),
            ),
            4 => ExifValue::Long(
                (0..count)
                    .filter_map(|i| {
                        let b = bytes.get(i * 4..i * 4 + 4)?;
                        Some(match self.byte_order {
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
                        let num = match self.byte_order {
                            ByteOrder::LittleEndian => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                            ByteOrder::BigEndian => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                        };
                        let den = match self.byte_order {
                            ByteOrder::LittleEndian => u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                            ByteOrder::BigEndian => u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
                        };
                        Some((num, den))
                    })
                    .collect(),
            ),
            7 => ExifValue::Undefined(bytes.iter().take(count).copied().collect()),
            9 => ExifValue::SLong(
                (0..count)
                    .filter_map(|i| {
                        let b = bytes.get(i * 4..i * 4 + 4)?;
                        Some(match self.byte_order {
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
                        let num = match self.byte_order {
                            ByteOrder::LittleEndian => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                            ByteOrder::BigEndian => i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                        };
                        let den = match self.byte_order {
                            ByteOrder::LittleEndian => i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                            ByteOrder::BigEndian => i32::from_be_bytes([b[4], b[5], b[6], b[7]]),
                        };
                        Some((num, den))
                    })
                    .collect(),
            ),
            _ => ExifValue::Undefined(bytes.iter().take(count).copied().collect()),
        }
    }
}

fn ifd_pointer_offset(value: &ExifValue) -> Option<usize> {
    match value {
        ExifValue::Long(v) => v.first().map(|&x| x as usize),
        ExifValue::Short(v) => v.first().map(|&x| x as usize),
        _ => None,
    }
}

fn tag_name(tag: u16, ifd_type: IfdType) -> Option<&'static str> {
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
            0x001D => Some("GPSDateStamp"),
            _ => None,
        },
        IfdType::ExifIfd => match tag {
            0x829A => Some("ExposureTime"),
            0x829D => Some("FNumber"),
            0x8822 => Some("ExposureProgram"),
            0x8827 => Some("ISOSpeedRatings"),
            0x9000 => Some("ExifVersion"),
            0x9003 => Some("DateTimeOriginal"),
            0x9004 => Some("DateTimeDigitized"),
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
            0x927C => Some("MakerNote"),
            0x9286 => Some("UserComment"),
            0x9290 => Some("SubSecTime"),
            0x9291 => Some("SubSecTimeOriginal"),
            0x9292 => Some("SubSecTimeDigitized"),
            0xA000 => Some("FlashPixVersion"),
            0xA001 => Some("ColorSpace"),
            0xA002 => Some("PixelXDimension"),
            0xA003 => Some("PixelYDimension"),
            0xA004 => Some("RelatedSoundFile"),
            0xA005 => Some("InteroperabilityIFDPointer"),
            0xA20B => Some("FlashEnergy"),
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
            0xA40C => Some("SubjectDistanceRange"),
            0xA420 => Some("ImageUniqueID"),
            _ => None,
        },
        IfdType::Ifd0 | IfdType::Ifd1 => match tag {
            0x00FE => Some("NewSubfileType"),
            0x0100 => Some("ImageWidth"),
            0x0101 => Some("ImageLength"),
            0x0102 => Some("BitsPerSample"),
            0x0103 => Some("Compression"),
            0x0106 => Some("PhotometricInterpretation"),
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
            0x0140 => Some("ColorMap"),
            0x0213 => Some("YCbCrPositioning"),
            0x0214 => Some("ReferenceBlackWhite"),
            0x8298 => Some("Copyright"),
            0x8769 => Some("ExifIFDPointer"),
            0x8825 => Some("GPSInfoIFDPointer"),
            _ => None,
        },
    }
}
