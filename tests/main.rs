extern crate core;

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use vexel::{bitreader::BitReader, Vexel};

    const PATH_JPEG_RED: &str = "./tests/images/jpeg/1x1_red.jpg";
    const PATH_JPEG_CAT: &str = "./tests/images/jpeg/cat.jpg";
    const PATH_JPEG_SUBSAMPLED: &str = "./tests/images/jpeg/frog.jpg";
    const PATH_JPEG_LS_1: &str = "./tests/images/jpeg-ls/test_4x4.jls";
    const PATH_GIF_1: &str = "./tests/images/gif/still_transparent.gif";
    const PATH_PPM_1: &str = "./tests/images/netpbm/p5_16bit.pgm";

    #[test]
    pub fn test_bitreader() -> Result<(), Box<dyn std::error::Error>> {
        // Test reading individual bits
        let data = vec![0b10101010];
        let mut reader = BitReader::new(Cursor::new(data));

        assert_eq!(reader.read_bit()?, true);
        assert_eq!(reader.read_bit()?, false);
        assert_eq!(reader.read_bit()?, true);
        assert_eq!(reader.read_bit()?, false);
        assert_eq!(reader.read_bit()?, true);
        assert_eq!(reader.read_bit()?, false);
        assert_eq!(reader.read_bit()?, true);
        assert_eq!(reader.read_bit()?, false);

        // Test reading multiple bits at once
        let data = vec![0b10101010, 0b11001100];
        let mut reader = BitReader::new(Cursor::new(data));

        assert_eq!(reader.read_bits(3)?, 0b101);
        assert_eq!(reader.read_bits(7)?, 0b0101011);
        assert_eq!(reader.read_bits(6)?, 0b001100);

        // Test reading a mix of individual bits and multiple bits
        let data = vec![0b10101010, 0b11001100, 0b01010101];
        let mut reader = BitReader::new(Cursor::new(data));

        // Read 5 bits
        assert_eq!(reader.read_bits(5).unwrap(), 0b10101);

        // Read 3 individual bits
        assert_eq!(reader.read_bit()?, false);
        assert_eq!(reader.read_bit()?, true);
        assert_eq!(reader.read_bit()?, false);

        // Read 7 more bits
        assert_eq!(reader.read_bits(7).unwrap(), 0b1100110);

        // Read 2 more individual bits
        assert_eq!(reader.read_bit()?, false);
        assert_eq!(reader.read_bit()?, false);

        // Read the 6 more bits
        assert_eq!(reader.read_bits(6)?, 0b101010);

        // Read the last bit
        assert_eq!(reader.read_bit()?, true);

        Ok(())
    }

    #[test]
    pub fn test_reading_image_header() -> Result<(), Box<dyn std::error::Error>> {
        let mut decoder = Vexel::open(PATH_JPEG_CAT)?;

        match decoder.decode() {
            Ok(_) => {
                let decoder = match decoder.decoder() {
                    vexel::Decoders::Jpeg(jpeg_decoder) => jpeg_decoder,
                    _ => panic!("Invalid decoder"),
                };

                assert_eq!(decoder.width(), 680);
                assert_eq!(decoder.height(), 453);

                let components = decoder.components();
                assert_eq!(components.len(), 3);

                let component_1 = components.get(0).unwrap();
                assert_eq!(component_1.id, 1);
                assert_eq!(component_1.horizontal_sampling_factor, 1);
                assert_eq!(component_1.vertical_sampling_factor, 1);
                assert_eq!(component_1.quantization_table_id, 0);
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_reading_quantization_tables() -> Result<(), Box<dyn std::error::Error>> {
        let mut decoder = Vexel::open(PATH_JPEG_CAT)?;

        let table_1 = [6, 4, 4, 6, 10, 16, 20, 24, 5, 5, 6, 8, 10, 23, 24, 22, 6, 5, 6, 10, 16, 23, 28, 22, 6, 7, 9, 12, 20, 35, 32, 25, 7, 9, 15, 22, 27, 44, 41, 31, 10, 14, 22, 26, 32, 42, 45, 37, 20, 26, 31, 35, 41, 48, 48, 40, 29, 37, 38, 39, 45, 40, 41, 40];
        let table_2 = [7, 7, 10, 19, 40, 40, 40, 40, 7, 8, 10, 26, 40, 40, 40, 40, 10, 10, 22, 40, 40, 40, 40, 40, 19, 26, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40];

        match decoder.decode() {
            Ok(_) => {
                let decoder = match decoder.decoder() {
                    vexel::Decoders::Jpeg(jpeg_decoder) => jpeg_decoder,
                    _ => panic!("Invalid decoder"),
                };

                let tables = decoder.quantization_tables();

                assert_eq!(tables[0].table.len(), table_1.len());
                for i in 0..table_1.len() {
                    assert_eq!(tables[0].table[i], table_1[i]);
                }

                assert_eq!(tables[1].table.len(), table_2.len());
                for i in 0..table_2.len() {
                    assert_eq!(tables[1].table[i], table_2[i]);
                }
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_reading_huffman_tables() -> Result<(), Box<dyn std::error::Error>> {
        let mut decoder = Vexel::open(PATH_JPEG_CAT)?;

        let ac_1_offsets = [0, 0, 2, 3, 6, 9, 11, 15, 18, 23, 28, 32, 36, 36, 36, 37, 162];
        let ac_2_offsets = [0, 0, 2, 3, 5, 9, 13, 16, 20, 27, 32, 36, 40, 40, 41, 43, 162];
        let dc_1_offsets = [0, 0, 1, 6, 7, 8, 9, 10, 11, 12, 12, 12, 12, 12, 12, 12, 12];
        let dc_2_offsets = [0, 0, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 12, 12, 12, 12, 12];

        match decoder.decode() {
            Ok(_) => {
                let decoder = match decoder.decoder() {
                    vexel::Decoders::Jpeg(jpeg_decoder) => jpeg_decoder,
                    _ => panic!("Invalid decoder"),
                };

                let tables = decoder.huffman_tables();
                assert_eq!(tables.len(), 2);

                let ac_tables = tables.get(0).unwrap();
                assert_eq!(ac_tables.len(), 2);

                let ac_table_1 = ac_tables.get(0).unwrap();
                assert_eq!(ac_table_1.offsets.len(), ac_1_offsets.len());
                for i in 0..ac_1_offsets.len() {
                    assert_eq!(ac_table_1.offsets[i], ac_1_offsets[i]);
                }

                assert_eq!(ac_table_1.symbols.len(), 162);

                assert_eq!(ac_table_1.symbols[0], 1);
                assert_eq!(ac_table_1.symbols[10], 65);

                let ac_table_2 = ac_tables.get(1).unwrap();
                assert_eq!(ac_table_2.offsets.len(), ac_2_offsets.len());
                for i in 0..ac_2_offsets.len() {
                    assert_eq!(ac_table_2.offsets[i], ac_2_offsets[i]);
                }

                assert_eq!(ac_table_2.symbols.len(), 162);

                let dc_tables = tables.get(1).unwrap();
                assert_eq!(dc_tables.len(), 2);

                let dc_table_1 = dc_tables.get(0).unwrap();
                assert_eq!(dc_table_1.offsets.len(), dc_1_offsets.len());
                for i in 0..dc_1_offsets.len() {
                    assert_eq!(dc_table_1.offsets[i], dc_1_offsets[i]);
                }

                assert_eq!(dc_table_1.symbols[0], 0);
                assert_eq!(dc_table_1.symbols[10], 10);

                let dc_table_2 = dc_tables.get(1).unwrap();
                assert_eq!(dc_table_2.offsets.len(), dc_2_offsets.len());
                for i in 0..dc_2_offsets.len() {
                    assert_eq!(dc_table_2.offsets[i], dc_2_offsets[i]);
                }
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_jpeg_decode() -> Result<(), Box<dyn std::error::Error>> {
        let mut decoder = Vexel::open(PATH_JPEG_CAT)?;

        match decoder.decode() {
            Ok(image) => {
                let pixels = image.as_rgb8();

                assert_eq!(pixels.len(), 680 * 453 * 3);
                assert_eq!(pixels[0], 25);
                assert_eq!(pixels[10], 20);
                assert_eq!(pixels[11111], 125);
                assert_eq!(pixels[900000], 193);
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        decoder = Vexel::open(PATH_JPEG_RED)?;

        match decoder.decode() {
            Ok(image) => {
                let pixels = image.as_rgb8();
                let expected: Vec<u8> = Vec::from([255, 0, 2]);

                assert_eq!(pixels, expected);
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        decoder = Vexel::open(PATH_JPEG_SUBSAMPLED)?;

        match decoder.decode() {
            Ok(image) => {
                //Vexel::write_bmp("test.bmp", image.width(), image.height(), &image.as_rgb8())?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_jls_decode() -> Result<(), Box<dyn std::error::Error>> {
        let mut decoder = Vexel::open(PATH_JPEG_LS_1)?;

        match decoder.decode() {
            Ok(image) => {
                //Vexel::write_bmp("test.bmp", decoder.width(), decoder.height(), image.frames[0].pixels.as_slice())?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_gif_decode() -> Result<(), Box<dyn std::error::Error>> {
        let mut decoder = Vexel::open(PATH_GIF_1)?;

        match decoder.decode() {
            Ok(image) => {
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }
        Ok(())
    }

    #[test]
    pub fn test_netpbm_decode() -> Result<(), Box<dyn std::error::Error>> {
        let mut decoder = Vexel::open(PATH_PPM_1)?;

        match decoder.decode() {
            Ok(image) => {
               //Vexel::write_bmp("test.bmp", image.width(), image.height(), &image.as_rgb8())?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }
}
