# Vexel

A resilient image decoder written in Rust that prioritizes format support and corruption recovery.

## Project Goals

1. **Maximum Format Support**: Focus on supporting as many image formats and their features as possible
2. **Corruption Recovery**: Make the best effort to decode images even when they're corrupted or malformed
3. **Safety**: Ensure memory-safe operations while handling potentially broken files

## Current Status
🚧 **Work in Progress** 🚧   

This decoder is currently under heavy development and is not suitable for usage. While it can decode some formats, optimization and
performance are in a pretty bad state right now.

## Format Support
🚧 **All formats have poor performance for now** 🚧

### JPEG
- ⚠️ Not well tested - may crash/decode incorrectly
- ✅ Baseline DCT
- ❌ Extended sequential DCT
- ✅ Progressive DCT
- ✅ Lossless mode
- ❌ Arithmetic coding
- ❌ Differential coding
- ❌ JPEG-LS
- ❌ Hierarchical mode

### PNG
- ✅ All bit depth and color types
- ✅ APNG animation
- ❌ APNG frame blending is not always correct
- ✅ Interlacing
- ✅ Basic chunk handling
- ❌ Advanced chunk handling

### GIF
- ✅ Fully supported

### BMP
- ⚠️ Not well tested - may crash/decode incorrectly
- ✅ 1/4/8-bit indexed color
- ✅ 16/24/32/64-bit RGB(A)
- ✅ RLE4/RLE8 compression
- ❌ JPEG/PNG compression
- ❌ V4/V5 header features
- ⚠️ Performance optimization needed

### NetPBM
- ✅ ASCII formats (P1-P3)
- ✅ Binary formats (P4-P6)
- ✅ PAM format (P7)
- ✅ All standard features

### TIFF
- ⚠️ Not well tested - may crash/decode incorrectly
- ✅ Extremely basic support
- ✅ Grayscale & RGB(A)
- ❌ Compression support
- ❌ CMYK/YCbCr/CIELab
- ❌ Multi-page support
- ❌ Advanced features (tiles, etc.)

## Corruption Recovery

The decoder attempts to recover as much data as possible from corrupted files rather than failing outright.
It attempts to continue even when encountering corrupt/incorrect information, substituting sensible defaults
or clamping existing values where possible. Corruption recovery is still being improved,
so some complex scenarios may still result in decode failures, but the decoder aims to handle common
corruption patterns gracefully.

## Usage

```rust
use vexel::Vexel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Basic decoding
    let mut decoder = Vexel::open("image.jpg")?;
    let image = decoder.decode()?;
    let pixels = image.as_rgb8();
    println!("Image size: {}x{}", image.width(), image.height());
    
    // Get all information that decoder has about the image
    let info = decoder.get_info();
    println!("Image info: {:?}", info);

    Ok(())
}
```

## Contributing

If you've stumbled across this project and want to contribute - you're very welcome!   
Some areas that need work:
1. Adding support for missing format features
2. Improving how the decoders handle corrupted files and recover partial data
3. Adding more tests
4. Performance optimizations, especially for larger files

## License

BSD 2-Clause License - see [LICENSE](LICENSE) for details.
