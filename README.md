# Vexel

An image decoder library written in Rust. The primary goals are broad format support, best-effort decoding of corrupted or malformed files, and memory safety.

The library does not aim to be the fastest decoder for any particular format. It prioritizes recovering something useful from broken input over failing.

Every decoder is written from scratch. The core library has no algorithmic dependencies - all format parsing, decompression, and pixel handling is written from scratch. Runtime dependencies are limited to `rayon` for optional parallelism and the WASM binding crates (`wasm-bindgen`, `serde`, `tsify`), which only handle JS interop and carry no decoding logic.

## Supported formats

Full support means the decoder handles ALL features and subformats of the format.
Functional means some may not be implmented yet, but vast majority of files will decode correctly.

| Format   | Status |
|----------|--------|
| JPEG     | Functional - see notes below |
| JPEG-LS  | Functional |
| PNG      | Full support |
| GIF      | Full support |
| BMP      | Functional |
| TGA      | Full support |
| NetPBM   | Full support |
| HDR      | Full support |
| ICO/CUR  | Full support |
| JBIG1    | Full support |
| TIFF     | Partial - see notes below |

### JPEG

All coding modes are implemented:

- Baseline DCT (Huffman and arithmetic)
- Extended sequential DCT (Huffman and arithmetic)
- Progressive DCT (Huffman and arithmetic)
- Lossless (Huffman and arithmetic)
- Differential sequential, differential progressive, differential lossless
- Hierarchical

Known gaps: restart markers are not handled in lossless and arithmetic lossless paths. And Hierarchical mode is not well tested yet.

### BMP

Supports all bit depths (1, 4, 8, 16, 24, 32, 64), RLE4 and RLE8 compression, bitfield masks (RGB and RGBA), color table images, embedded JPEG and PNG, CORE headers, and the BM/BA/CI/CP/IC/PT file type variants. CMYK RLE and huffman compressions are not implemented.

### TIFF
TIFF is a beast, nothing supports every feature it has. We support most of the common and some rare features for now.

Strips and tiles are supported, including volumetric tiled images. Multi-page files decode each IFD as a separate frame. Both chunky and planar configurations are supported.

Supported compression: none, LZW, PackBits, Deflate/AdobeDeflate, JPEG (old and new), PNG, SGILog/SGILog24.

Color spaces supported: RGB, RGBA, grayscale, palette (color-mapped), YCbCr, CIELab, CMYK. 

## Usage

```rust
use vexel::Vexel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = Vexel::open("image.jpg")?;
    let image = decoder.decode()?;

    println!("{}x{}", image.width(), image.height());

    let pixels: Vec<u8> = image.as_rgba8();

    let info = decoder.get_info();
    println!("{:?}", info);

    Ok(())
}
```

The `Vexel::new` constructor accepts any `Read + Seek` source, so in-memory buffers work directly:

```rust
use std::io::Cursor;
use vexel::Vexel;

let data: Vec<u8> = std::fs::read("image.png")?;
let mut decoder = Vexel::new(Cursor::new(data))?;
let image = decoder.decode()?;
```

### Resource limits

By default, allocations for pixel data are capped at 512 MiB. Image dimensions are unconstrained. You can override this before decoding:

```rust
use vexel::{Vexel, Limits};

let mut decoder = Vexel::open("image.png")?;
decoder.set_limits(Limits {
    max_image_width: Some(8192),
    max_image_height: Some(8192),
    max_alloc: Some(256 * 1024 * 1024),
});
let image = decoder.decode()?;
```

Use `Limits::no_limits()` to remove all constraints.

### Pixel formats

Decoders produce one of the following pixel formats:

| Variant    | Description |
| ---------- | ----------- |
| `RGB8`     | 8-bit RGB |
| `RGBA8`    | 8-bit RGBA |
| `RGB16`    | 16-bit RGB |
| `RGBA16`   | 16-bit RGBA |
| `RGB32F`   | 32-bit float RGB |
| `RGBA32F`  | 32-bit float RGBA |
| `RGB64F`   | 64-bit float RGB |
| `RGBA64F`  | 64-bit float RGBA |
| `L1`       | 1-bit grayscale |
| `L8`       | 8-bit grayscale |
| `L16`      | 16-bit grayscale |
| `LA8`      | 8-bit grayscale + alpha |
| `LA16`     | 16-bit grayscale + alpha |
| `L32F`     | 32-bit float grayscale |
| `LA32F`    | 32-bit float grayscale + alpha |
| `L64F`     | 64-bit float grayscale |
| `LA64F`    | 64-bit float grayscale + alpha |

`image.as_rgb8()` and `image.as_rgba8()` convert any format to 8-bit and return the first frame's pixels as a `Vec<u8>`.

To work with the native data without converting, match on `frame.pixels()`:

```rust
use vexel::{Vexel, PixelData};

let mut decoder = Vexel::open("image.tiff")?;
let image = decoder.decode()?;

match image.pixels() {
    PixelData::RGB16(samples) => {
        // samples is &Vec<u16>, one u16 per channel, interleaved R G B R G B ...
        for pixel in samples.chunks_exact(3) {
            let (r, g, b) = (pixel[0], pixel[1], pixel[2]);
            println!("{r} {g} {b}");
        }
    }
    PixelData::RGB32F(samples) => {
        // samples is &Vec<f32>
    }
    PixelData::L16(samples) => {
        // samples is &Vec<u16>, one value per pixel
    }
    other => {
        let rgb8 = frame.as_rgb8();
    }
}
```

For animated formats, iterate over `image.frames()` and use `frame.delay()` to get the display duration in milliseconds.

## WebAssembly

The library builds to WASM. Four JS-facing exports are provided:

- `decodeImage(data: Uint8Array)` - decodes and returns all frames as RGBA8
- `getInfo(data: Uint8Array)` - decodes and returns format metadata
- `tryGuessFormat(data: Uint8Array)` - returns the detected format name without decoding
- `setLogLevel(level: string)` - sets the minimum log level; accepts `"Debug"`, `"Warning"`, or `"Error"`

## Performance

Performance is acceptable for most images, but not as good as in mature libraries like libjpeg-turbo.

There are some optimizations in place:

Rayon is used to parallelize some inner loops in decoders. SIMD paths exist for most SIMDable operations. Usually AVX2, WASM SIMD128 and scalar paths are implemented. 

These are opportunistic optimizations. They are not the primary focus of the project and coverage is uneven across formats.

## Fuzzing

There is a fuzz target for every supported format, using `cargo-fuzz` / libFuzzer. They live in `vexel/fuzz/fuzz_targets/`.

Run a single target:

```sh
cargo +nightly fuzz run decode_jpeg
```

The test images in `vexel/tests/images/` are used as the initial corpus, so the fuzzer starts from known-valid inputs and mutates from there. Corpus files are stored per-target under `vexel/fuzz/corpus/`. Crashes are written to `vexel/fuzz/artifacts/`.

## CLI

A command-line tool is included in the `vexel-cli` package:

```sh
cargo build --package vexel-cli
```

```
vexel [OPTIONS] <PATH>

Options:
  -f, --format <FORMAT>    Output format: ppm, pam, webp, jxl [default: jxl]
  -o, --output-dir <DIR>   Output directory for batch operations
  -O, --output <FILE>      Output file path
      --frames             Write each frame as a separate file
      --info               Print format metadata and exit
      --void               Decode but do not write output
      --gui                Open decoded image in a GUI preview window
      --log-level <LEVEL>  Log verbosity: Debug, Warning, Error
```

PATH supports glob patterns for batch processing:

```sh
vexel image.png                        # decode to JXL next to source
vexel -f webp image.png                # decode to WebP
vexel image.png -O output.webp         # specify output path
vexel "*.png" -f webp -o ./converted   # batch convert
vexel --frames -f ppm image.gif        # write each frame separately
vexel --info image.png                 # print format metadata
vexel --gui image.png                  # GUI preview
vexel --gui ./images                   # browse directory in GUI
```

Output formats JXL and WebP require the respective system libraries at link time (`libjxl`, `libwebp`).


## License

BSD 2-Clause - see [LICENSE](LICENSE).
