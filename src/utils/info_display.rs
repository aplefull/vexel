use std::fmt::{Debug, Display, Formatter};
use crate::decoders::jpeg::{ArithmeticCodingTable, ExifHeader, HuffmanTable, JFIFHeader, QuantizationTable, ScanInfo};

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

fn print_huffman_table(f: &mut Formatter<'_>, table: &HuffmanTable) -> std::fmt::Result {
    writeln!(f, "Table ID: {}", table.id)?;

    for j in 0..16 {
        write!(f, "{}: ", j + 1)?;

        let start = table.offsets[j];
        let end = table.offsets[j + 1];

        for k in start..end {
            write!(f, "0x{:02X}, ", table.symbols[k as usize])?;
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
    pub ac_arithmetic_tables: Vec<ArithmeticCodingTable>,
    pub dc_arithmetic_tables: Vec<ArithmeticCodingTable>,
    pub scans: Vec<ScanInfo>,
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

        writeln!(f, "====================")?;

        for comment in &self.comments {
            writeln!(f, "Comment: {}", comment)?;
        }

        writeln!(f, "====================")?;

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

        for table in &self.quantization_tables {
            writeln!(f, "Quantization table:")?;
            writeln!(f, "  Precision: {}", table.precision)?;
            writeln!(f, "  ID: {}", table.id)?;

            print_matrix(f, &table.table, 8)?;
            writeln!(f, "-----------------")?;
        }

        writeln!(f, "====================")?;

        for scan in &self.scans {
            writeln!(f, "Scan:")?;
            writeln!(f, "  Components:")?;

            writeln!(f, "  Spectral selection: {}-{}", scan.start_spectral, scan.end_spectral)?;
            writeln!(f, "  Successive approximation: {}-{}", scan.successive_low, scan.successive_high)?;
            writeln!(f, "  Data length: {}", scan.data_length)?;

            for component in &scan.components {
                writeln!(f, "    Component: {}", component.component_id)?;
                writeln!(f, "    AC table selector: {}", component.ac_table_selector)?;
                writeln!(f, "    DC table selector: {}", component.dc_table_selector)?;
            }

            writeln!(f, "====================")?;

            writeln!(f, "AC Huffman tables:")?;
            for table in scan.ac_tables.iter() {
                writeln!(f, "  ID: {}", table.id)?;
                writeln!(f, "  Class: {}", table.class)?;

                writeln!(f, "  Values:")?;
                print_huffman_table(f, table)?;
                writeln!(f, "-----------------")?;
            }

            writeln!(f, "====================")?;

            writeln!(f, "DC Huffman tables:")?;
            for table in scan.dc_tables.iter() {
                writeln!(f, "  ID: {}", table.id)?;
                writeln!(f, "  Class: {}", table.class)?;

                writeln!(f, "  Values:")?;
                print_huffman_table(f, table)?;

                writeln!(f, "-----------------")?;
            }

            writeln!(f, "====================")?;
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
