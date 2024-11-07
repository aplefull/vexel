extern crate core;

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use vexel::{bitreader::BitReader, Vexel};

    const PATH_JPEG_RED: &str = "./tests/images/jpeg/1x1_red.jpg";
    const PATH_JPEG_CAT: &str = "./tests/images/jpeg/cat.jpg";
    const PATH_JPEG_SUBSAMPLED: &str = "./tests/images/jpeg/fish.jpg";
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
            Ok(image) => {}
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
