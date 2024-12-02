use std::fs::File;
use std::io::{Write};

pub fn write_bmp(output_path: &str, width: u32, height: u32, pixels: &Vec<u8>) -> Result<(), std::io::Error> {
    let mut file = File::create(output_path)?;

    let padding_size = width % 4;
    let size: u32 = 14 + 12 + 40 + width * height * 3 + padding_size * height;
    let row_size = width * 3;
    let padded_row_size = row_size + padding_size;
    let pixel_array_size = padded_row_size * height;
    let header_size = 14 + 40;

    // BMP File Header (14 bytes)
    file.write_all(b"BM")?; // BM
    file.write_all(&(size as u32).to_le_bytes())?; // File size
    file.write_all(&[0; 4])?; // Reserved
    file.write_all(&(header_size as u32).to_le_bytes())?; // Offset to pixel array

    // DIB Header (40 bytes)
    file.write_all(&[40, 0, 0, 0])?; // DIB header size
    file.write_all(&(width as i32).to_le_bytes())?; // Image width
    file.write_all(&(height as i32).to_le_bytes())?; // Image height
    file.write_all(&[1, 0])?; // Planes
    file.write_all(&[24, 0])?; // Bits per pixel
    file.write_all(&[0; 4])?; // Compression method (none)
    file.write_all(&(pixel_array_size as u32).to_le_bytes())?; // Image size
    file.write_all(&[0; 16])?; // Remaining DIB header fields (set to 0)

    let padding = [0u8; 3]; // Max padding is 3 bytes
    for y in (0..height).rev() {
        for x in 0..width {
            let pixel_index = ((y * width + x) * 3) as usize;
            let r = pixels[pixel_index];
            let g = pixels[pixel_index + 1];
            let b = pixels[pixel_index + 2];
            file.write_all(&[b, g, r])?; // BMP stores pixels as BGR
        }

        // Write padding
        file.write_all(&padding[..padding_size as usize])?;
    }

    Ok(())
}

pub fn write_ppm(output_path: &str, width: u32, height: u32, pixels: &Vec<u8>) -> Result<(), std::io::Error> {
    let mut file = File::create(output_path)?;

    file.write_all(b"P6\n")?;
    file.write_all(format!("{} {}\n", width, height).as_bytes())?;
    file.write_all(b"255\n")?; // Max color value

    for y in 0..height {
        for x in 0..width {
            let pixel_index = ((y * width + x) * 3) as usize;
            if pixel_index + 2 >= pixels.len() {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Pixel index out of bounds while writing PPM file"));
            }
            
            let r = pixels[pixel_index];
            let g = pixels[pixel_index + 1];
            let b = pixels[pixel_index + 2];
            file.write_all(&[r, g, b])?;
        }
    }

    Ok(())
}
