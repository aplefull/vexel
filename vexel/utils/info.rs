use crate::decoders::bmp::BmpSectionInfo;
use crate::decoders::gif::GifSectionInfo;
use crate::decoders::hdr::HdrSectionInfo;
use crate::decoders::ico::{IconDirEntry, IcoType};
use crate::decoders::jpeg::types::JpegSegmentInfo;
use crate::decoders::netpbm::NetpbmSectionInfo;
use crate::decoders::png::PngChunkInfo;
use crate::utils::exif::{ExifIfd, ExifValue};
use serde::Serialize;
use std::fmt;
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum ImageInfo {
    Jpeg(JpegInfo),
    Png(PngInfo),
    Bmp(BmpInfo),
    Gif(GifInfo),
    Netpbm(NetpbmInfo),
    Hdr(HdrInfo),
    Jbig1(Jbig1Info),
    Ico(IcoInfo),
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegInfo {
    pub sections: Vec<JpegSegmentInfo>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct PngInfo {
    pub sections: Vec<PngChunkInfo>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct BmpInfo {
    pub sections: Vec<BmpSectionInfo>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct GifInfo {
    pub sections: Vec<GifSectionInfo>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct NetpbmInfo {
    pub sections: Vec<NetpbmSectionInfo>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct HdrInfo {
    pub sections: Vec<HdrSectionInfo>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Jbig1Info {
    pub width: u32,
    pub height: u32,
    pub planes: u8,
    pub dl: u8,
    pub d: u8,
    pub l0: u32,
    pub mx: u8,
    pub my: u8,
    pub order: u8,
    pub options: u8,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct IcoInfo {
    pub width: u32,
    pub height: u32,
    pub ico_type: IcoType,
    pub entries: Vec<IconDirEntry>,
}

impl fmt::Display for ImageInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageInfo::Jpeg(info) => write!(f, "{}", info),
            ImageInfo::Png(info) => write!(f, "{}", info),
            ImageInfo::Bmp(info) => write!(f, "{}", info),
            ImageInfo::Gif(info) => write!(f, "{}", info),
            ImageInfo::Netpbm(info) => write!(f, "{}", info),
            ImageInfo::Hdr(info) => write!(f, "{}", info),
            ImageInfo::Jbig1(info) => write!(f, "{}", info),
            ImageInfo::Ico(info) => write!(f, "{}", info),
        }
    }
}

impl fmt::Display for PngInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "PNG Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Total chunks: {}", self.sections.len())?;
        writeln!(f)?;

        for (idx, chunk) in self.sections.iter().enumerate() {
            writeln!(f, "Chunk #{}: {} (offset: 0x{:08X}, length: {} bytes)",
                idx + 1, chunk.chunk_type, chunk.start_offset, chunk.length)?;

            match &chunk.data {
                crate::decoders::png::PngChunkData::IHDR(data) => {
                    writeln!(f, "  Width: {}", data.width)?;
                    writeln!(f, "  Height: {}", data.height)?;
                    writeln!(f, "  Bit Depth: {}", data.bit_depth)?;
                    writeln!(f, "  Color Type: {:?}", data.color_type)?;
                    writeln!(f, "  Compression: {}", data.compression_method)?;
                    writeln!(f, "  Filter: {}", data.filter_method)?;
                    writeln!(f, "  Interlace: {}", data.interlace_method)?;
                }
                crate::decoders::png::PngChunkData::PLTE(data) => {
                    writeln!(f, "  Palette entries: {}", data.entries.len())?;
                    let hex: Vec<String> = data.entries.iter().map(|[r, g, b]| format!("#{:02X}{:02X}{:02X}", r, g, b)).collect();
                    writeln!(f, "  Colors: {}", hex.join(", "))?;
                }
                crate::decoders::png::PngChunkData::IDAT(data) => {
                    writeln!(f, "  Data length: {} bytes", data.data_length)?;
                }
                crate::decoders::png::PngChunkData::GAMA(data) => {
                    writeln!(f, "  Gamma: {} (raw: {})", data.gamma, data.gamma_raw)?;
                }
                crate::decoders::png::PngChunkData::CHRM(data) => {
                    writeln!(f, "  White point: ({}, {})", data.chromaticities.white_point_x, data.chromaticities.white_point_y)?;
                    writeln!(f, "  Red: ({}, {})", data.chromaticities.red_x, data.chromaticities.red_y)?;
                    writeln!(f, "  Green: ({}, {})", data.chromaticities.green_x, data.chromaticities.green_y)?;
                    writeln!(f, "  Blue: ({}, {})", data.chromaticities.blue_x, data.chromaticities.blue_y)?;
                }
                crate::decoders::png::PngChunkData::TRNS(data) => {
                    match &data.transparency {
                        crate::decoders::png::TransparencyData::Grayscale(v) => {
                            writeln!(f, "  Transparent value: {}", v)?;
                        }
                        crate::decoders::png::TransparencyData::RGB(r, g, b) => {
                            writeln!(f, "  Transparent color: RGB({}, {}, {})", r, g, b)?;
                        }
                        crate::decoders::png::TransparencyData::Palette(vals) => {
                            writeln!(f, "  Palette alpha values: {} entries", vals.len())?;
                        }
                    }
                }
                crate::decoders::png::PngChunkData::BKGD(data) => {
                    match &data.background {
                        crate::decoders::png::BackgroundData::Grayscale(v) => {
                            writeln!(f, "  Background value: {}", v)?;
                        }
                        crate::decoders::png::BackgroundData::RGB(r, g, b) => {
                            writeln!(f, "  Background color: RGB({}, {}, {})", r, g, b)?;
                        }
                        crate::decoders::png::BackgroundData::PaletteIndex(idx) => {
                            writeln!(f, "  Background palette index: {}", idx)?;
                        }
                    }
                }
                crate::decoders::png::PngChunkData::PHYS(data) => {
                    writeln!(f, "  Pixels per unit X: {}", data.physical_dimensions.pixels_per_unit_x)?;
                    writeln!(f, "  Pixels per unit Y: {}", data.physical_dimensions.pixels_per_unit_y)?;
                    writeln!(f, "  Unit: {:?}", data.physical_dimensions.unit)?;
                }
                crate::decoders::png::PngChunkData::SBIT(data) => {
                    writeln!(f, "  Significant bits: {:?}", data.significant_bits)?;
                }
                crate::decoders::png::PngChunkData::TIME(data) => {
                    writeln!(f, "  Last modified: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                        data.time.year, data.time.month, data.time.day,
                        data.time.hour, data.time.minute, data.time.second)?;
                }
                crate::decoders::png::PngChunkData::TEXT(data) |
                crate::decoders::png::PngChunkData::ZTXT(data) |
                crate::decoders::png::PngChunkData::ITXT(data) => {
                    match &data.text {
                        crate::decoders::png::PngText::Basic { keyword, text } => {
                            writeln!(f, "  Keyword: {}", keyword)?;
                            if text.len() > 60 {
                                writeln!(f, "  Text: {}... ({} chars)", &text[..60], text.len())?;
                            } else {
                                writeln!(f, "  Text: {}", text)?;
                            }
                        }
                        crate::decoders::png::PngText::Compressed { keyword, text } => {
                            writeln!(f, "  Keyword: {}", keyword)?;
                            if text.len() > 60 {
                                writeln!(f, "  Text (compressed): {}... ({} chars)", &text[..60], text.len())?;
                            } else {
                                writeln!(f, "  Text (compressed): {}", text)?;
                            }
                        }
                        crate::decoders::png::PngText::International { keyword, language_tag, translated_keyword, text } => {
                            writeln!(f, "  Keyword: {}", keyword)?;
                            writeln!(f, "  Language: {}", language_tag)?;
                            writeln!(f, "  Translated keyword: {}", translated_keyword)?;
                            if text.len() > 60 {
                                writeln!(f, "  Text: {}... ({} chars)", &text[..60], text.len())?;
                            } else {
                                writeln!(f, "  Text: {}", text)?;
                            }
                        }
                    }
                }
                crate::decoders::png::PngChunkData::SRGB(data) => {
                    writeln!(f, "  Rendering intent: {:?}", data.rendering_intent)?;
                }
                crate::decoders::png::PngChunkData::ICCP(data) => {
                    writeln!(f, "  Profile name: {}", data.profile_name)?;
                    writeln!(f, "  Profile size: {} bytes", data.profile.header.size)?;
                }
                crate::decoders::png::PngChunkData::SPLT(data) => {
                    writeln!(f, "  Palette name: {}", data.palette.name)?;
                    writeln!(f, "  Sample depth: {} bits", data.palette.sample_depth)?;
                    writeln!(f, "  Samples: {}", data.palette.samples.len())?;
                    let entries: Vec<String> = data.palette.samples.iter()
                        .map(|s| format!("#{:04X}{:04X}{:04X}{:04X}(f={})", s.red, s.green, s.blue, s.alpha, s.frequency))
                        .collect();
                    writeln!(f, "  Colors: {}", entries.join(", "))?;
                }
                crate::decoders::png::PngChunkData::HIST(data) => {
                    writeln!(f, "  Histogram entries: {}", data.frequencies.len())?;
                    let freqs: Vec<String> = data.frequencies.iter().map(|n| n.to_string()).collect();
                    writeln!(f, "  Frequencies: {}", freqs.join(", "))?;
                }
                crate::decoders::png::PngChunkData::ACTL(data) => {
                    writeln!(f, "  Number of frames: {}", data.actl.num_frames)?;
                    writeln!(f, "  Number of plays: {}", data.actl.num_plays)?;
                }
                crate::decoders::png::PngChunkData::FCTL(data) => {
                    writeln!(f, "  Sequence: {}", data.fctl.sequence_number)?;
                    writeln!(f, "  Dimensions: {}x{}", data.fctl.width, data.fctl.height)?;
                    writeln!(f, "  Offset: ({}, {})", data.fctl.x_offset, data.fctl.y_offset)?;
                    writeln!(f, "  Delay: {}/{} seconds", data.fctl.delay_num, data.fctl.delay_den)?;
                    writeln!(f, "  Dispose: {}, Blend: {}", data.fctl.dispose_op, data.fctl.blend_op)?;
                }
                crate::decoders::png::PngChunkData::FDAT(data) => {
                    writeln!(f, "  Sequence: {}", data.sequence_number)?;
                    writeln!(f, "  Data length: {} bytes", data.data_length)?;
                }
                crate::decoders::png::PngChunkData::IEND { .. } => {
                    writeln!(f, "  End of image")?;
                }
                crate::decoders::png::PngChunkData::Signature => {}
                crate::decoders::png::PngChunkData::Unknown { chunk_type, length, .. } => {
                    writeln!(f, "  Unknown chunk type: {}", chunk_type)?;
                    writeln!(f, "  Length: {} bytes", length)?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

fn fmt_exif_value(value: &ExifValue) -> String {
    match value {
        ExifValue::Ascii(s) => s.clone(),
        ExifValue::Short(v) => v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "),
        ExifValue::Long(v) => v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "),
        ExifValue::Rational(v) => v
            .iter()
            .map(|(n, d)| if *d == 0 { format!("{}/0", n) } else { format!("{}/{}", n, d) })
            .collect::<Vec<_>>()
            .join(", "),
        ExifValue::SLong(v) => v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "),
        ExifValue::SRational(v) => v
            .iter()
            .map(|(n, d)| if *d == 0 { format!("{}/0", n) } else { format!("{}/{}", n, d) })
            .collect::<Vec<_>>()
            .join(", "),
        ExifValue::Byte(v) => format!("{} bytes", v.len()),
        ExifValue::Undefined(v) => format!("{} bytes (undefined)", v.len()),
    }
}

fn fmt_exif_ifd(f: &mut fmt::Formatter<'_>, name: &str, ifd: &ExifIfd) -> fmt::Result {
    writeln!(f, "  {} ({} entries):", name, ifd.entries.len())?;
    for entry in &ifd.entries {
        let label = entry.tag_name.as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("0x{:04X}", entry.tag));
        writeln!(f, "    {}: {}", label, fmt_exif_value(&entry.value))?;
    }
    Ok(())
}

impl fmt::Display for JpegInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::decoders::jpeg::types::JpegSegmentData;

        writeln!(f, "JPEG Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Segments: {}", self.sections.len())?;
        writeln!(f)?;

        for segment in &self.sections {
            writeln!(f, "Offset 0x{:08X}  {}", segment.start_offset, segment.marker)?;

            match &segment.data {
                JpegSegmentData::SOI => {
                    writeln!(f, "  Start of image")?;
                }
                JpegSegmentData::EOI => {
                    writeln!(f, "  End of image")?;
                }
                JpegSegmentData::APP0(jfif) => {
                    writeln!(f, "  Length: {} bytes", jfif.length)?;
                    writeln!(f, "  Identifier: {}", jfif.identifier.trim_end_matches('\0'))?;
                    writeln!(f, "  Version: {}.{:02}", jfif.version_major, jfif.version_minor)?;
                    writeln!(f, "  Density units: {}", jfif.density_units)?;
                    writeln!(f, "  Density: {}x{}", jfif.x_density, jfif.y_density)?;
                    if jfif.thumbnail_width > 0 || jfif.thumbnail_height > 0 {
                        writeln!(f, "  Thumbnail: {}x{}", jfif.thumbnail_width, jfif.thumbnail_height)?;
                    }
                }
                JpegSegmentData::APP1 { length, exif } => {
                    writeln!(f, "  Length: {} bytes", length)?;
                    if let Some(exif) = exif {
                        writeln!(f, "  Byte order: {:?}", exif.byte_order)?;
                        fmt_exif_ifd(f, "IFD0", &exif.ifd0)?;
                        if let Some(ifd) = &exif.exif_ifd {
                            fmt_exif_ifd(f, "ExifIFD", ifd)?;
                        }
                        if let Some(ifd) = &exif.gps_ifd {
                            fmt_exif_ifd(f, "GPSIFD", ifd)?;
                        }
                        if let Some(ifd) = &exif.ifd1 {
                            fmt_exif_ifd(f, "IFD1 (thumbnail)", ifd)?;
                        }
                    }
                }
                JpegSegmentData::APP2(app2) => {
                    writeln!(f, "  Length: {} bytes", app2.length)?;
                    writeln!(f, "  Identifier: {}", app2.identifier)?;
                    if let Some(icc) = &app2.icc_profile_sequence {
                        writeln!(f, "  ICC chunk: {}/{}", icc.chunk_sequence, icc.total_chunks)?;
                        writeln!(f, "  ICC profile data: {} bytes", icc.profile_data_length)?;
                    }
                }
                JpegSegmentData::APP14(adobe) => {
                    writeln!(f, "  Length: {} bytes", adobe.length)?;
                    writeln!(f, "  Version: {}", adobe.version)?;
                    writeln!(f, "  Flags0: 0x{:04X}", adobe.flags0)?;
                    writeln!(f, "  Flags1: 0x{:04X}", adobe.flags1)?;
                    writeln!(f, "  Color transform: {}", adobe.color_transform)?;
                }
                JpegSegmentData::APP { marker, length, identifier } => {
                    writeln!(f, "  Marker: {}", marker)?;
                    writeln!(f, "  Length: {} bytes", length)?;
                    if let Some(id) = identifier {
                        writeln!(f, "  Identifier: {}", id)?;
                    }
                }
                JpegSegmentData::EXP { expand_horizontal, expand_vertical } => {
                    writeln!(f, "  Expand horizontal: {}", expand_horizontal)?;
                    writeln!(f, "  Expand vertical: {}", expand_vertical)?;
                }
                JpegSegmentData::SOF(sof) => {
                    writeln!(f, "  Length: {} bytes", sof.length)?;
                    writeln!(f, "  Marker: {}", sof.marker)?;
                    writeln!(f, "  Precision: {} bits", sof.precision)?;
                    writeln!(f, "  Width: {} pixels", sof.width)?;
                    writeln!(f, "  Height: {} pixels", sof.height)?;
                    writeln!(f, "  Components: {}", sof.component_count)?;
                    for comp in &sof.components {
                        writeln!(f, "    Component {}: sampling {}x{}, quant table {}", comp.id, comp.horizontal_sampling_factor, comp.vertical_sampling_factor, comp.quantization_table_id)?;
                    }
                }
                JpegSegmentData::DHT(dht) => {
                    writeln!(f, "  Length: {} bytes", dht.length)?;
                    writeln!(f, "  Tables: {}", dht.tables.len())?;
                    for table in &dht.tables {
                        let class = if table.class == 0 { "DC" } else { "AC" };
                        writeln!(f, "    {} table id={}", class, table.id)?;
                        writeln!(f, "      Counts (per length 1-16): {:?}", table.counts)?;
                        writeln!(f, "      Symbols ({}): {:?}", table.symbols.len(), table.symbols)?;
                    }
                }
                JpegSegmentData::DAC(dac) => {
                    writeln!(f, "  Length: {} bytes", dac.length)?;
                    for table in &dac.dc_tables {
                        writeln!(f, "    DC table id={}, Kx={}, U/L={}", table.identifier, table.values[0].value, table.values[0].length)?;
                    }
                    for table in &dac.ac_tables {
                        writeln!(f, "    AC table id={}, Kx={}", table.identifier, table.values[0].value)?;
                    }
                }
                JpegSegmentData::DQT(dqt) => {
                    writeln!(f, "  Length: {} bytes", dqt.length)?;
                    writeln!(f, "  Tables: {}", dqt.tables.len())?;
                    for table in &dqt.tables {
                        let bits = if table.precision == 0 { 8u8 } else { 16u8 };
                        writeln!(f, "    Table id={}, precision={} bits", table.id, bits)?;
                        for row in 0..8 {
                            let row_vals: Vec<u16> = (0..8).map(|col| table.table.get(row * 8 + col).copied().unwrap_or(0)).collect();
                            writeln!(f, "      {:?}", row_vals)?;
                        }
                    }
                }
                JpegSegmentData::DRI { restart_interval } => {
                    writeln!(f, "  Restart interval: {}", restart_interval)?;
                }
                JpegSegmentData::SOS(sos) => {
                    writeln!(f, "  Length: {} bytes", sos.length)?;
                    writeln!(f, "  Components: {}", sos.component_count)?;
                    for comp in &sos.components {
                        writeln!(f, "    Component {}: DC table {}, AC table {}", comp.component_id, comp.dc_table_selector, comp.ac_table_selector)?;
                    }
                    writeln!(f, "  Spectral selection: {}-{}", sos.start_spectral, sos.end_spectral)?;
                    writeln!(f, "  Successive approximation: {}/{}", sos.successive_high, sos.successive_low)?;
                    writeln!(f, "  Scan data: {} bytes", sos.data_length)?;
                }
                JpegSegmentData::COM { text } => {
                    writeln!(f, "  Comment: {}", text)?;
                }
                JpegSegmentData::Unknown { marker, length } => {
                    writeln!(f, "  Marker: {}", marker)?;
                    writeln!(f, "  Length: {} bytes", length)?;
                }
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

impl fmt::Display for BmpInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::decoders::bmp::{BmpSectionData, DibHeader};

        writeln!(f, "BMP Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Total sections: {}", self.sections.len())?;
        writeln!(f)?;

        for section in &self.sections {
            match &section.data {
                BmpSectionData::BitmapArrayHeader(ba) => {
                    writeln!(f, "Offset 0x{:08X}  Bitmap Array Header", section.start_offset)?;
                    writeln!(f, "  File size: {} bytes", ba.file_size)?;
                    writeln!(f, "  Next offset: {}", ba.next_offset)?;
                    writeln!(f, "  Screen size: {}x{}", ba.screen_width, ba.screen_height)?;
                }
                BmpSectionData::FileHeader(h) => {
                    writeln!(f, "Offset 0x{:08X}  File Header", section.start_offset)?;
                    writeln!(f, "  File size: {} bytes", h.file_size)?;
                    writeln!(f, "  Reserved1: {}", h.reserved1)?;
                    writeln!(f, "  Reserved2: {}", h.reserved2)?;
                    writeln!(f, "  Pixel data offset: {} bytes", h.pixel_offset)?;
                }
                BmpSectionData::DibHeader(dib) => {
                    writeln!(f, "Offset 0x{:08X}  DIB Header", section.start_offset)?;
                    match dib {
                        DibHeader::Core(h) => {
                            writeln!(f, "  Type: BITMAPCOREHEADER (12 bytes)")?;
                            writeln!(f, "  Width: {}", h.width)?;
                            writeln!(f, "  Height: {}", h.height)?;
                            writeln!(f, "  Planes: {}", h.planes)?;
                            writeln!(f, "  Bits per pixel: {}", h.bits_per_pixel)?;
                        }
                        DibHeader::OS2V2(h) => {
                            writeln!(f, "  Type: OS22XBITMAPHEADER")?;
                            writeln!(f, "  Width: {}", h.width)?;
                            writeln!(f, "  Height: {}", h.height)?;
                            writeln!(f, "  Planes: {}", h.planes)?;
                            writeln!(f, "  Bits per pixel: {}", h.bits_per_pixel)?;
                            writeln!(f, "  Compression: {:?}", h.compression)?;
                            writeln!(f, "  Image size: {} bytes", h.image_size)?;
                            writeln!(f, "  X pixels per meter: {}", h.x_pixels_per_meter)?;
                            writeln!(f, "  Y pixels per meter: {}", h.y_pixels_per_meter)?;
                            writeln!(f, "  Colors used: {}", h.colors_used)?;
                            writeln!(f, "  Important colors: {}", h.important_colors)?;
                        }
                        DibHeader::Info(h) => {
                            writeln!(f, "  Type: BITMAPINFOHEADER (40 bytes)")?;
                            writeln!(f, "  Width: {}", h.width)?;
                            writeln!(f, "  Height: {}", h.height)?;
                            writeln!(f, "  Planes: {}", h.planes)?;
                            writeln!(f, "  Bits per pixel: {}", h.bits_per_pixel)?;
                            writeln!(f, "  Compression: {:?}", h.compression)?;
                            writeln!(f, "  Image size: {} bytes", h.image_size)?;
                            writeln!(f, "  X pixels per meter: {}", h.x_pixels_per_meter)?;
                            writeln!(f, "  Y pixels per meter: {}", h.y_pixels_per_meter)?;
                            writeln!(f, "  Colors used: {}", h.colors_used)?;
                            writeln!(f, "  Important colors: {}", h.important_colors)?;
                        }
                        DibHeader::V2(h) => {
                            writeln!(f, "  Type: BITMAPV2INFOHEADER (52 bytes)")?;
                            writeln!(f, "  Width: {}", h.info.width)?;
                            writeln!(f, "  Height: {}", h.info.height)?;
                            writeln!(f, "  Planes: {}", h.info.planes)?;
                            writeln!(f, "  Bits per pixel: {}", h.info.bits_per_pixel)?;
                            writeln!(f, "  Compression: {:?}", h.info.compression)?;
                            writeln!(f, "  Image size: {} bytes", h.info.image_size)?;
                            writeln!(f, "  X pixels per meter: {}", h.info.x_pixels_per_meter)?;
                            writeln!(f, "  Y pixels per meter: {}", h.info.y_pixels_per_meter)?;
                            writeln!(f, "  Colors used: {}", h.info.colors_used)?;
                            writeln!(f, "  Important colors: {}", h.info.important_colors)?;
                            writeln!(f, "  Red mask: 0x{:08X}", h.red_mask)?;
                            writeln!(f, "  Green mask: 0x{:08X}", h.green_mask)?;
                            writeln!(f, "  Blue mask: 0x{:08X}", h.blue_mask)?;
                        }
                        DibHeader::V3(h) => {
                            writeln!(f, "  Type: BITMAPV3INFOHEADER (56 bytes)")?;
                            writeln!(f, "  Width: {}", h.v2.info.width)?;
                            writeln!(f, "  Height: {}", h.v2.info.height)?;
                            writeln!(f, "  Planes: {}", h.v2.info.planes)?;
                            writeln!(f, "  Bits per pixel: {}", h.v2.info.bits_per_pixel)?;
                            writeln!(f, "  Compression: {:?}", h.v2.info.compression)?;
                            writeln!(f, "  Image size: {} bytes", h.v2.info.image_size)?;
                            writeln!(f, "  X pixels per meter: {}", h.v2.info.x_pixels_per_meter)?;
                            writeln!(f, "  Y pixels per meter: {}", h.v2.info.y_pixels_per_meter)?;
                            writeln!(f, "  Colors used: {}", h.v2.info.colors_used)?;
                            writeln!(f, "  Important colors: {}", h.v2.info.important_colors)?;
                            writeln!(f, "  Red mask: 0x{:08X}", h.v2.red_mask)?;
                            writeln!(f, "  Green mask: 0x{:08X}", h.v2.green_mask)?;
                            writeln!(f, "  Blue mask: 0x{:08X}", h.v2.blue_mask)?;
                            writeln!(f, "  Alpha mask: 0x{:08X}", h.alpha_mask)?;
                        }
                        DibHeader::V4(h) => {
                            writeln!(f, "  Type: BITMAPV4HEADER (108 bytes)")?;
                            writeln!(f, "  Width: {}", h.v3.v2.info.width)?;
                            writeln!(f, "  Height: {}", h.v3.v2.info.height)?;
                            writeln!(f, "  Planes: {}", h.v3.v2.info.planes)?;
                            writeln!(f, "  Bits per pixel: {}", h.v3.v2.info.bits_per_pixel)?;
                            writeln!(f, "  Compression: {:?}", h.v3.v2.info.compression)?;
                            writeln!(f, "  Image size: {} bytes", h.v3.v2.info.image_size)?;
                            writeln!(f, "  X pixels per meter: {}", h.v3.v2.info.x_pixels_per_meter)?;
                            writeln!(f, "  Y pixels per meter: {}", h.v3.v2.info.y_pixels_per_meter)?;
                            writeln!(f, "  Colors used: {}", h.v3.v2.info.colors_used)?;
                            writeln!(f, "  Important colors: {}", h.v3.v2.info.important_colors)?;
                            writeln!(f, "  Red mask: 0x{:08X}", h.v3.v2.red_mask)?;
                            writeln!(f, "  Green mask: 0x{:08X}", h.v3.v2.green_mask)?;
                            writeln!(f, "  Blue mask: 0x{:08X}", h.v3.v2.blue_mask)?;
                            writeln!(f, "  Alpha mask: 0x{:08X}", h.v3.alpha_mask)?;
                            writeln!(f, "  CS type: 0x{:08X}", h.cs_type)?;
                            writeln!(f, "  Gamma red: {}", h.gamma_red)?;
                            writeln!(f, "  Gamma green: {}", h.gamma_green)?;
                            writeln!(f, "  Gamma blue: {}", h.gamma_blue)?;
                        }
                        DibHeader::V5(h) => {
                            writeln!(f, "  Type: BITMAPV5HEADER (124 bytes)")?;
                            writeln!(f, "  Width: {}", h.v4.v3.v2.info.width)?;
                            writeln!(f, "  Height: {}", h.v4.v3.v2.info.height)?;
                            writeln!(f, "  Planes: {}", h.v4.v3.v2.info.planes)?;
                            writeln!(f, "  Bits per pixel: {}", h.v4.v3.v2.info.bits_per_pixel)?;
                            writeln!(f, "  Compression: {:?}", h.v4.v3.v2.info.compression)?;
                            writeln!(f, "  Image size: {} bytes", h.v4.v3.v2.info.image_size)?;
                            writeln!(f, "  X pixels per meter: {}", h.v4.v3.v2.info.x_pixels_per_meter)?;
                            writeln!(f, "  Y pixels per meter: {}", h.v4.v3.v2.info.y_pixels_per_meter)?;
                            writeln!(f, "  Colors used: {}", h.v4.v3.v2.info.colors_used)?;
                            writeln!(f, "  Important colors: {}", h.v4.v3.v2.info.important_colors)?;
                            writeln!(f, "  Red mask: 0x{:08X}", h.v4.v3.v2.red_mask)?;
                            writeln!(f, "  Green mask: 0x{:08X}", h.v4.v3.v2.green_mask)?;
                            writeln!(f, "  Blue mask: 0x{:08X}", h.v4.v3.v2.blue_mask)?;
                            writeln!(f, "  Alpha mask: 0x{:08X}", h.v4.v3.alpha_mask)?;
                            writeln!(f, "  CS type: 0x{:08X}", h.v4.cs_type)?;
                            writeln!(f, "  Gamma red: {}", h.v4.gamma_red)?;
                            writeln!(f, "  Gamma green: {}", h.v4.gamma_green)?;
                            writeln!(f, "  Gamma blue: {}", h.v4.gamma_blue)?;
                            writeln!(f, "  Intent: {}", h.intent)?;
                            writeln!(f, "  Profile data offset: {}", h.profile_data)?;
                            writeln!(f, "  Profile size: {} bytes", h.profile_size)?;
                        }
                    }
                }
                BmpSectionData::ExtraMasks(m) => {
                    writeln!(f, "Offset 0x{:08X}  Extra Color Masks", section.start_offset)?;
                    writeln!(f, "  Red mask: 0x{:08X}", m.red_mask)?;
                    writeln!(f, "  Green mask: 0x{:08X}", m.green_mask)?;
                    writeln!(f, "  Blue mask: 0x{:08X}", m.blue_mask)?;
                    writeln!(f, "  Alpha mask: 0x{:08X}", m.alpha_mask)?;
                }
                BmpSectionData::ColorTable(entries) => {
                    writeln!(f, "Offset 0x{:08X}  Color Table ({} entries)", section.start_offset, entries.len())?;
                    let hex: Vec<String> = entries.iter().map(|e| format!("#{:02X}{:02X}{:02X}", e.red, e.green, e.blue)).collect();
                    writeln!(f, "  Colors: {}", hex.join(", "))?;
                }
                BmpSectionData::PixelData(pd) => {
                    writeln!(f, "Offset 0x{:08X}  Pixel Data", section.start_offset)?;
                    writeln!(f, "  Length: {} bytes", pd.length)?;
                }
                BmpSectionData::IccProfile(profile) => {
                    writeln!(f, "Offset 0x{:08X}  ICC Profile", section.start_offset)?;
                    writeln!(f, "  Size: {} bytes", profile.header.size)?;
                    writeln!(f, "  Class: {}", profile.header.profile_class)?;
                    writeln!(f, "  Color space: {}", profile.header.color_space)?;
                    writeln!(f, "  PCS: {}", profile.header.pcs)?;
                    writeln!(f, "  Tags: {}", profile.tag_table.tag_count)?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

impl fmt::Display for GifInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::decoders::gif::GifSectionData;

        writeln!(f, "GIF Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Total sections: {}", self.sections.len())?;
        writeln!(f)?;

        for section in &self.sections {
            match &section.data {
                GifSectionData::Header(h) => {
                    writeln!(f, "Offset 0x{:08X}  Header", section.start_offset)?;
                    writeln!(f, "  Version: {}", h.version)?;
                    writeln!(f, "  Canvas width: {}", h.canvas_width)?;
                    writeln!(f, "  Canvas height: {}", h.canvas_height)?;
                    writeln!(f, "  Global color table flag: {}", h.global_color_table_flag)?;
                    writeln!(f, "  Color resolution: {}", h.color_resolution)?;
                    writeln!(f, "  Sort flag: {}", h.sort_flag)?;
                    writeln!(f, "  Size of global color table: {}", h.size_of_global_color_table)?;
                    writeln!(f, "  Background color index: {}", h.background_color_index)?;
                    writeln!(f, "  Pixel aspect ratio: {}", h.pixel_aspect_ratio)?;
                }
                GifSectionData::GlobalColorTable(ct) => {
                    writeln!(f, "Offset 0x{:08X}  Global Color Table", section.start_offset)?;
                    writeln!(f, "  Entries: {}", ct.entry_count)?;
                    writeln!(f, "  Length: {} bytes", ct.length)?;
                    let hex: Vec<String> = ct.entries.iter().map(|[r, g, b]| format!("#{:02X}{:02X}{:02X}", r, g, b)).collect();
                    writeln!(f, "  Colors: {}", hex.join(", "))?;
                }
                GifSectionData::GraphicsControlExtension(gce) => {
                    writeln!(f, "Offset 0x{:08X}  Graphics Control Extension", section.start_offset)?;
                    writeln!(f, "  Disposal method: {:?}", gce.disposal_method)?;
                    writeln!(f, "  User input: {}", gce.user_input)?;
                    writeln!(f, "  Transparent color index: {:?}", gce.transparent_color_index)?;
                    writeln!(f, "  Delay: {} ms", gce.delay)?;
                }
                GifSectionData::ImageDescriptor(id) => {
                    writeln!(f, "Offset 0x{:08X}  Image Descriptor", section.start_offset)?;
                    writeln!(f, "  Position: ({}, {})", id.left, id.top)?;
                    writeln!(f, "  Dimensions: {}x{}", id.width, id.height)?;
                    writeln!(f, "  Local color table flag: {}", id.local_color_table_flag)?;
                    writeln!(f, "  Interlace flag: {}", id.interlace_flag)?;
                    writeln!(f, "  Sort flag: {}", id.sort_flag)?;
                    writeln!(f, "  Size of local color table: {}", id.size_of_local_color_table)?;
                }
                GifSectionData::LocalColorTable(ct) => {
                    writeln!(f, "Offset 0x{:08X}  Local Color Table", section.start_offset)?;
                    writeln!(f, "  Entries: {}", ct.entry_count)?;
                    writeln!(f, "  Length: {} bytes", ct.length)?;
                    let hex: Vec<String> = ct.entries.iter().map(|[r, g, b]| format!("#{:02X}{:02X}{:02X}", r, g, b)).collect();
                    writeln!(f, "  Colors: {}", hex.join(", "))?;
                }
                GifSectionData::ImageData(id) => {
                    writeln!(f, "Offset 0x{:08X}  Image Data", section.start_offset)?;
                    writeln!(f, "  LZW minimum code size: {}", id.lzw_minimum_code_size)?;
                    writeln!(f, "  Data length: {} bytes", id.data_length)?;
                }
                GifSectionData::CommentExtension(c) => {
                    writeln!(f, "Offset 0x{:08X}  Comment Extension", section.start_offset)?;
                    if c.text.len() > 60 {
                        writeln!(f, "  Text: {}... ({} chars)", &c.text[..60], c.text.len())?;
                    } else {
                        writeln!(f, "  Text: {}", c.text)?;
                    }
                }
                GifSectionData::ApplicationExtension(ae) => {
                    writeln!(f, "Offset 0x{:08X}  Application Extension", section.start_offset)?;
                    writeln!(f, "  Identifier: {}", ae.identifier)?;
                    writeln!(f, "  Auth code: {}", ae.auth_code)?;
                    if let Some(loop_count) = ae.loop_count {
                        writeln!(f, "  Loop count: {}", loop_count)?;
                    }
                    if let Some(buffer_size) = ae.buffer_size {
                        writeln!(f, "  Buffer size: {}", buffer_size)?;
                    }
                    writeln!(f, "  Data length: {} bytes", ae.data_length)?;
                }
                GifSectionData::PlainTextExtension(pt) => {
                    writeln!(f, "Offset 0x{:08X}  Plain Text Extension", section.start_offset)?;
                    writeln!(f, "  Position: ({}, {})", pt.left, pt.top)?;
                    writeln!(f, "  Dimensions: {}x{}", pt.width, pt.height)?;
                    writeln!(f, "  Cell: {}x{}", pt.cell_width, pt.cell_height)?;
                    writeln!(f, "  Foreground color: {}", pt.foreground_color)?;
                    writeln!(f, "  Background color: {}", pt.background_color)?;
                    if pt.text.len() > 60 {
                        writeln!(f, "  Text: {}... ({} chars)", &pt.text[..60], pt.text.len())?;
                    } else {
                        writeln!(f, "  Text: {}", pt.text)?;
                    }
                }
                GifSectionData::Trailer => {
                    writeln!(f, "Offset 0x{:08X}  Trailer", section.start_offset)?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

impl fmt::Display for NetpbmInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::decoders::netpbm::{NetpbmSectionData};

        writeln!(f, "Netpbm Image Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Total sections: {}", self.sections.len())?;
        writeln!(f)?;

        for section in &self.sections {
            match &section.data {
                NetpbmSectionData::Comment(text) => {
                    writeln!(f, "Offset 0x{:08X}  Comment", section.start_offset)?;
                    writeln!(f, "  Text: {}", text)?;
                }
                NetpbmSectionData::Header(h) => {
                    writeln!(f, "Offset 0x{:08X}  Header", section.start_offset)?;
                    writeln!(f, "  Format: {:?}", h.format)?;
                    writeln!(f, "  Width: {}", h.width)?;
                    writeln!(f, "  Height: {}", h.height)?;
                    writeln!(f, "  Max value: {}", h.max_value)?;
                    if let Some(depth) = h.depth {
                        writeln!(f, "  Depth: {}", depth)?;
                    }
                    if let Some(tuple_type) = &h.tuple_type {
                        writeln!(f, "  Tuple type: {:?}", tuple_type)?;
                    } else if let Some(raw) = &h.tuple_type_raw {
                        writeln!(f, "  Tuple type (raw): {}", raw)?;
                    }
                }
                NetpbmSectionData::PixelData(pd) => {
                    writeln!(f, "Offset 0x{:08X}  Pixel Data", section.start_offset)?;
                    writeln!(f, "  Length: {} bytes", pd.length)?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

impl fmt::Display for HdrInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::decoders::hdr::HdrSectionData;

        writeln!(f, "HDR Image Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Total sections: {}", self.sections.len())?;
        writeln!(f)?;

        for section in &self.sections {
            match &section.data {
                HdrSectionData::Header(h) => {
                    writeln!(f, "Offset 0x{:08X}  Header", section.start_offset)?;
                    writeln!(f, "  Format: {:?}", h.format)?;
                    writeln!(f, "  Width: {}", h.width)?;
                    writeln!(f, "  Height: {}", h.height)?;
                    if let Some(gamma) = h.gamma {
                        writeln!(f, "  Gamma: {}", gamma)?;
                    }
                    if let Some(exposure) = h.exposure {
                        writeln!(f, "  Exposure: {}", exposure)?;
                    }
                    if let Some(ratio) = h.pixel_aspect_ratio {
                        writeln!(f, "  Pixel aspect ratio: {}", ratio)?;
                    }
                    if let Some(cc) = h.color_correction {
                        writeln!(f, "  Color correction: R={} G={} B={}", cc[0], cc[1], cc[2])?;
                    }
                    if let Some(p) = h.primaries {
                        writeln!(f, "  Primaries red: ({}, {})", p[0], p[1])?;
                        writeln!(f, "  Primaries green: ({}, {})", p[2], p[3])?;
                        writeln!(f, "  Primaries blue: ({}, {})", p[4], p[5])?;
                        writeln!(f, "  Primaries white: ({}, {})", p[6], p[7])?;
                    }
                    for comment in &h.comments {
                        writeln!(f, "  Comment: {}", comment)?;
                    }
                }
                HdrSectionData::PixelData(pd) => {
                    writeln!(f, "Offset 0x{:08X}  Pixel Data", section.start_offset)?;
                    writeln!(f, "  Length: {} bytes", pd.length)?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

impl fmt::Display for Jbig1Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "JBIG1 Image Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Planes: {}", self.planes)?;
        writeln!(f, "Resolution layers: {} to {}", self.dl, self.d)?;
        writeln!(f, "Lines per stripe (L0): {}", self.l0)?;
        writeln!(f, "Max ATMOVE x: {}", self.mx)?;
        writeln!(f, "Max ATMOVE y: {}", self.my)?;
        writeln!(f, "Order flags: 0x{:02X}", self.order)?;
        writeln!(f, "Options: 0x{:02X}", self.options)?;
        Ok(())
    }
}

impl fmt::Display for IcoInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ICO/CUR Image Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Type: {:?}", self.ico_type)?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Images: {}", self.entries.len())?;
        for (i, entry) in self.entries.iter().enumerate() {
            writeln!(f, "  Image #{}: {}x{} {:?} {} bpp", i + 1, entry.width, entry.height, entry.image_format, entry.bit_count)?;
        }
        Ok(())
    }
}
