use std::fmt::{Debug, Display, Formatter};
use crate::decoders::jpeg::{ArithmeticCodingTable, ExifHeader, HuffmanTable, JFIFHeader, QuantizationTable};

fn print_matrix<T: Display + Debug>(f: &mut Formatter<'_>, values: &Vec<T>, width: usize) -> std::fmt::Result {
    if values.is_empty() {
        writeln!(f, "[]")?;
        return Ok(());
    }

    let str_values: Vec<String> = values
        .iter()
        .map(|x| format!("{}", x))
        .collect();

    let max_width = str_values
        .iter()
        .map(|s| s.len())
        .max()
        .unwrap_or(0);

    for chunk in str_values.chunks(width) {
        for (i, value) in chunk.iter().enumerate() {
            if i == 0 {
                write!(f, "{:>width$}", value, width = max_width)?;
            } else {
                write!(f, " {:>width$}", value, width = max_width)?;
            }
        }

        writeln!(f)?;
    }

    Ok(())
}

pub enum ImageInfo {
    Jpeg(JpegInfo),
    Gif,
    Netpbm,
}

impl Debug for ImageInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageInfo::Jpeg(jpeg_info) => {
                writeln!(f, "{:?}", jpeg_info)
            }
            _ => {
                writeln!(f, "This image format is not supported yet")
            }
        }
    }
}

pub struct JpegInfo {
    pub width: u32,
    pub height: u32,
    pub color_depth: u8,
    pub number_of_components: u8,
    pub jfif_header: Option<JFIFHeader>,
    pub exif_header: Option<ExifHeader>,
    pub quantization_tables: Vec<QuantizationTable>,
    pub ac_huffman_tables: Vec<HuffmanTable>,
    pub dc_huffman_tables: Vec<HuffmanTable>,
    pub ac_arithmetic_tables: Vec<ArithmeticCodingTable>,
    pub dc_arithmetic_tables: Vec<ArithmeticCodingTable>,
    pub spectral_selection: (u8, u8),
    pub successive_approximation: (u8, u8),
    pub horizontal_sampling_factor: u8,
    pub vertical_sampling_factor: u8,
    pub restart_interval: u16,
    pub comments: Vec<String>,
}

impl Debug for JpegInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Spectral selection: {}-{}", self.spectral_selection.0, self.spectral_selection.1)?;
        writeln!(f, "Successive approximation: {}-{}", self.successive_approximation.0, self.successive_approximation.1)?;
        writeln!(f, "Horizontal sampling factor: {}", self.horizontal_sampling_factor)?;
        writeln!(f, "Vertical sampling factor: {}", self.vertical_sampling_factor)?;
        writeln!(f, "Restart interval: {}", self.restart_interval)?;

        writeln!(f)?;

        writeln!(f, "====================")?;

        for comment in &self.comments {
            writeln!(f, "Comment: {}", comment)?;
        }

        match &self.jfif_header {
            Some(jfif_header) => {
                writeln!(f, "JFIF header:")?;
                writeln!(f, "  Identifier: {}", jfif_header.identifier)?;
                writeln!(f, "  Version: {}.{}", jfif_header.version_major, jfif_header.version_minor)?;
                writeln!(f, "  Density units: {}", jfif_header.density_units)?;
                writeln!(f, "  X density: {}", jfif_header.x_density)?;
                writeln!(f, "  Y density: {}", jfif_header.y_density)?;
                writeln!(f, "  Thumbnail dimensions: {}x{}", jfif_header.thumbnail_width, jfif_header.thumbnail_height)?;
                writeln!(f, "  Thumbnail data: {} bytes", jfif_header.thumbnail_data.len())?;
            }
            None => {
                writeln!(f, "JFIF header: None")?;
            }
        }

        writeln!(f, "====================")?;

        match &self.exif_header {
            Some(exif_header) => {
                writeln!(f, "Exif header:")?;
                writeln!(f, "  Identifier: {}", exif_header.identifier)?;
                writeln!(f, "  Byte order: {:?}", exif_header.byte_order)?;
                writeln!(f, "  First IFD offset: {}", exif_header.first_ifd_offset)?;
                writeln!(f, "  IFD entries: {}", exif_header.ifd_entries.len())?;
            }
            None => {
                writeln!(f, "Exif header: None")?;
            }
        }

        writeln!(f, "====================")?;

        writeln!(f, "Quantization tables:")?;

        for table in &self.quantization_tables {
            writeln!(f, "  ID: {}", table.id)?;
            writeln!(f, "  Precision: {}", table.precision)?;
            writeln!(f, "  Length: {}", table.length)?;
            writeln!(f, "  Table:")?;

            print_matrix(f, &table.table, 8)?;
        }

        writeln!(f, "====================")?;

        writeln!(f, "AC Huffman tables:")?;

        for table in &self.ac_huffman_tables {
            writeln!(f, "  ID: {}", table.id)?;
            writeln!(f, "  Class: {}", table.class)?;

            writeln!(f, "  Offsets:")?;
            print_matrix(f, &table.offsets, 8)?;

            writeln!(f, "  Symbols:")?;
            print_matrix(f, &table.symbols, 8)?;

            writeln!(f, "  Codes:")?;
            print_matrix(f, &table.codes, 8)?;
        }

        writeln!(f, "====================")?;

        writeln!(f, "DC Huffman tables:")?;

        for table in &self.dc_huffman_tables {
            writeln!(f, "  ID: {}", table.id)?;
            writeln!(f, "  Class: {}", table.class)?;

            writeln!(f, "  Offsets:")?;
            print_matrix(f, &table.offsets, 8)?;

            writeln!(f, "  Symbols:")?;
            print_matrix(f, &table.symbols, 8)?;

            writeln!(f, "  Codes:")?;
            print_matrix(f, &table.codes, 8)?;
        }

        writeln!(f, "====================")?;

        writeln!(f, "AC Arithmetic tables:")?;

        for table in &self.ac_arithmetic_tables {
            writeln!(f, "  Class: {}", table.table_class)?;
            writeln!(f, "  Identifier: {}", table.identifier)?;

            writeln!(f, "  Values:")?;
            for value in &table.values {
                writeln!(f, "    Value: {}, Length: {}", value.value, value.length)?;
            }
        }

        writeln!(f, "====================")?;

        writeln!(f, "DC Arithmetic tables:")?;

        for table in &self.dc_arithmetic_tables {
            writeln!(f, "  Class: {}", table.table_class)?;
            writeln!(f, "  Identifier: {}", table.identifier)?;

            writeln!(f, "  Values:")?;
            for value in &table.values {
                writeln!(f, "    Value: {}, Length: {}", value.value, value.length)?;
            }
        }

        Ok(())
    }
}
