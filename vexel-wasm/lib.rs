#![cfg(target_arch = "wasm32")]

use std::fmt::Display;
use std::io::Cursor;
use std::sync::Arc;

use js_sys::{Reflect, Uint8ClampedArray};
use serde::Serialize;
use tsify::{Ts, Tsify};
use vexel::{parse_icc, set_log_level, Frames, ICCProfile, Image, ImageFormat, ImageInfo, Limits, LogLevel, Vexel};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsError, JsValue};
#[cfg(feature = "rayon")]
pub use wasm_bindgen_rayon::init_thread_pool;

const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

#[wasm_bindgen(typescript_custom_section)]
const LIMITS_TS: &'static str = r#"
export interface Limits {
    max_image_width?: number | null;
    max_image_height?: number | null;
    max_alloc?: number | null;
    max_pixel_count?: number | null;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

#[derive(Clone)]
struct SharedBytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

fn js_error(error: impl Display) -> JsError {
    JsError::new(&error.to_string())
}

fn bigints_to_numbers(value: &JsValue) -> JsValue {
    if value.is_bigint() {
        let digits = value.unchecked_ref::<js_sys::BigInt>().to_string(10).ok().map(String::from);
        return JsValue::from_f64(digits.and_then(|digits| digits.parse().ok()).unwrap_or(f64::NAN));
    }

    if js_sys::Array::is_array(value) {
        let array = value.unchecked_ref::<js_sys::Array>();
        for index in 0..array.length() {
            array.set(index, bigints_to_numbers(&array.get(index)));
        }
    } else if value.is_object() {
        for key in js_sys::Object::keys(value.unchecked_ref::<js_sys::Object>()).iter() {
            if let Ok(field) = Reflect::get(value, &key) {
                let _ = Reflect::set(value, &key, &bigints_to_numbers(&field));
            }
        }
    }

    value.clone()
}

fn limit_option(options: &JsValue, key: &str, default: Option<u64>, max: u64) -> Result<Option<u64>, JsError> {
    let value =
        Reflect::get(options, &JsValue::from_str(key)).map_err(|_| JsError::new("setLimits expects an object"))?;
    if value.is_undefined() {
        return Ok(default);
    }
    if value.is_null() {
        return Ok(None);
    }

    match value.as_f64() {
        Some(v) if v.fract() == 0.0 && v >= 0.0 && v <= max as f64 => Ok(Some(v as u64)),
        _ => {
            Err(JsError::new(&format!("setLimits: {} must be an integer from 0 to {}, or null for no limit", key, max)))
        }
    }
}

fn pixels_to_js(image: Image) -> Uint8ClampedArray {
    let bytes: Vec<u8> = image.into_rgba();
    let array = Uint8ClampedArray::new_with_length(bytes.len() as u32);
    array.copy_from(&bytes);
    array
}

/// WebAssembly entry point that mirrors the native [`Vexel`] API.
#[wasm_bindgen(js_name = Decoder)]
pub struct WasmDecoder {
    data: SharedBytes,
    decoder: Vexel<Cursor<SharedBytes>>,
    limits: Limits,
    frames: Option<Frames>,
    timings: JsTimings,
}

#[wasm_bindgen(js_class = Decoder)]
impl WasmDecoder {
    /// Creates a new decoder from raw image bytes.
    ///
    /// The image format is detected from the content of `data`. Data in a
    /// format vexel does not recognize still constructs a decoder: `getInfo()`
    /// then returns `"Unknown"` and the decode methods throw.
    #[wasm_bindgen(constructor)]
    pub fn new(data: Vec<u8>) -> Result<WasmDecoder, JsError> {
        let data = SharedBytes(Arc::new(data));
        let decoder = Vexel::new(Cursor::new(data.clone()))?;
        Ok(WasmDecoder { data, decoder, limits: Limits::default(), frames: None, timings: JsTimings::default() })
    }

    /// Sets resource limits that the decoder will enforce during decoding.
    ///
    /// Each field of `limits` is optional: an omitted field keeps its default
    /// and `null` removes that limit. Widths and heights are in pixels,
    /// `max_alloc` is in bytes and `max_pixel_count` counts pixels. Every call
    /// starts from the defaults: no dimension limits, a 1 GiB `max_alloc` and
    /// a `max_pixel_count` of 150 Mpx. Throws if a value is not a non-negative
    /// integer.
    #[wasm_bindgen(js_name = setLimits)]
    pub fn set_limits(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Limits")] limits: JsValue,
    ) -> Result<(), JsError> {
        if !limits.is_object() {
            return Err(JsError::new("setLimits expects an object"));
        }

        let defaults = Limits::default();
        let dimension = |key: &str, default: Option<u32>| {
            limit_option(&limits, key, default.map(u64::from), u64::from(u32::MAX)).map(|v| v.map(|v| v as u32))
        };
        let limits = Limits {
            max_image_width: dimension("max_image_width", defaults.max_image_width)?,
            max_image_height: dimension("max_image_height", defaults.max_image_height)?,
            max_alloc: limit_option(&limits, "max_alloc", defaults.max_alloc, MAX_SAFE_INTEGER)?,
            max_pixel_count: limit_option(&limits, "max_pixel_count", defaults.max_pixel_count, MAX_SAFE_INTEGER)?,
        };

        self.decoder.set_limits(limits.clone());
        self.limits = limits;
        Ok(())
    }

    /// Decodes the image and returns it as RGBA8.
    ///
    /// For animated images this is the first frame. Use
    /// [`nextFrame`](WasmDecoder::next_frame) to walk the whole animation.
    /// Pixel data is a `Uint8ClampedArray` of length `width * height * 4`.
    #[wasm_bindgen(js_name = decode)]
    pub fn decode(&mut self) -> Result<Ts<JsImage>, JsError> {
        let start = performance_now();
        let image = self.decoder.decode()?;
        let decoded = performance_now();

        let result = JsImage {
            width: image.width(),
            height: image.height(),
            image_format: self.decoder.format(),
            pixels: pixels_to_js(image),
        }
        .into_ts()
        .map_err(js_error);

        self.timings = JsTimings { decode_ms: decoded - start, convert_ms: performance_now() - decoded };
        result
    }

    /// Returns the next animation frame as RGBA8, or `undefined` once the
    /// animation is exhausted.
    ///
    /// The first call starts decoding; each frame is released once JavaScript
    /// drops it, so long animations can be painted without retaining them all.
    /// Images without animation yield exactly one frame. Frames are decoded
    /// independently of [`decode`](WasmDecoder::decode), so the two can be
    /// mixed on the same decoder.
    #[wasm_bindgen(js_name = nextFrame)]
    pub fn next_frame(&mut self) -> Result<Option<Ts<JsImageFrame>>, JsError> {
        let start = performance_now();

        let frames = match &mut self.frames {
            Some(frames) => frames,
            None => {
                let mut decoder = Vexel::new(Cursor::new(self.data.clone()))?;
                decoder.set_limits(self.limits.clone());
                self.frames.insert(decoder.decode_frames()?)
            }
        };
        let frame = frames.next_frame()?;
        let decoded = performance_now();

        let result = frame
            .map(|frame| {
                let delay = frame.delay();
                let image = frame.into_image();
                let (width, height) = (image.width(), image.height());
                JsImageFrame { width, height, pixels: pixels_to_js(image), delay }.into_ts()
            })
            .transpose()
            .map_err(js_error);

        self.timings = JsTimings { decode_ms: decoded - start, convert_ms: performance_now() - decoded };
        result
    }

    /// Decodes every page contained in the file, returning one [`JsImage`] per page in file order.
    ///
    /// Multi-page formats yield one entry per contained image; all other formats yield a single entry.
    #[wasm_bindgen(js_name = decodePages)]
    pub fn decode_pages(&mut self) -> Result<Vec<Ts<JsImage>>, JsError> {
        let start = performance_now();
        let pages = self.decoder.decode_pages()?;
        let decoded = performance_now();

        let result = self.images_to_js(pages);

        self.timings = JsTimings { decode_ms: decoded - start, convert_ms: performance_now() - decoded };
        result
    }

    /// Decodes all embedded thumbnails and preview images found in the file.
    ///
    /// The images are sorted from smallest to largest by pixel count. Formats
    /// without thumbnail support, and files that contain none, return an empty array.
    #[wasm_bindgen(js_name = decodeThumbnails)]
    pub fn decode_thumbnails(&mut self) -> Result<Vec<Ts<JsImage>>, JsError> {
        let start = performance_now();
        let thumbnails = self.decoder.decode_thumbnails()?;
        let decoded = performance_now();

        let result = self.images_to_js(thumbnails);

        self.timings = JsTimings { decode_ms: decoded - start, convert_ms: performance_now() - decoded };
        result
    }

    /// Returns how long the most recent `decode`, `nextFrame`, `decodePages` or
    /// `decodeThumbnails` call took, split into decoding and RGBA conversion.
    ///
    /// All values are zero until one of those methods has been called.
    #[wasm_bindgen(js_name = lastTimings)]
    pub fn last_timings(&self) -> Result<Ts<JsTimings>, JsError> {
        self.timings.into_ts().map_err(js_error)
    }

    /// Returns format-specific metadata for the image.
    ///
    /// Does not decode pixels. 64-bit values larger than 2^53 are rounded to the nearest
    /// representable number.
    #[wasm_bindgen(js_name = getInfo)]
    pub fn get_info(&mut self) -> Result<Ts<ImageInfo>, JsError> {
        let info = self.decoder.info();
        if let Ok(value) = info.into_ts() {
            return Ok(value);
        }

        let config = <ImageInfo as Tsify>::SERIALIZATION_CONFIG;
        let serializer = serde_wasm_bindgen::Serializer::new()
            .serialize_missing_as_null(config.missing_as_null)
            .serialize_maps_as_objects(config.hashmap_as_object)
            .serialize_large_number_types_as_bigints(true);
        let value = info.serialize(&serializer).map_err(js_error)?;
        Ok(Ts::new_unchecked(bigints_to_numbers(&value)))
    }

    /// Detects the image format from `data` without decoding it.
    ///
    /// Returns `"Unknown"` if the format cannot be identified.
    #[wasm_bindgen(js_name = tryGuessFormat)]
    pub fn try_guess_format(data: &[u8]) -> Result<Ts<ImageFormat>, JsError> {
        let format = Vexel::new(Cursor::new(data)).map_or(ImageFormat::Unknown, |decoder| decoder.format());
        format.into_ts().map_err(js_error)
    }
}

impl WasmDecoder {
    fn images_to_js(&self, images: Vec<Image>) -> Result<Vec<Ts<JsImage>>, JsError> {
        let format = self.decoder.format();
        images
            .into_iter()
            .map(|image| {
                JsImage {
                    width: image.width(),
                    height: image.height(),
                    image_format: format,
                    pixels: pixels_to_js(image),
                }
                .into_ts()
                .map_err(js_error)
            })
            .collect()
    }
}

/// WebAssembly-compatible image returned by [`WasmDecoder::decode`].
///
/// `pixels` is always RGBA8, regardless of the source format, and is a
/// `Uint8ClampedArray` with length `width * height * 4`.
#[derive(Serialize, Tsify)]
pub struct JsImage {
    pub width: u32,
    pub height: u32,
    pub image_format: ImageFormat,
    #[serde(with = "tsify::serde_wasm_bindgen::preserve")]
    #[tsify(type = "Uint8ClampedArray")]
    pub pixels: Uint8ClampedArray,
}

/// A single animation frame returned by [`WasmDecoder::next_frame`].
///
/// `pixels` is an RGBA8 `Uint8ClampedArray` with length `width * height * 4`.
/// `delay` is the display duration in milliseconds; `0` for non-animated images.
#[derive(Serialize, Tsify)]
pub struct JsImageFrame {
    pub width: u32,
    pub height: u32,
    #[serde(with = "tsify::serde_wasm_bindgen::preserve")]
    #[tsify(type = "Uint8ClampedArray")]
    pub pixels: Uint8ClampedArray,
    pub delay: u32,
}

/// Timings of the last decoding call, returned by [`WasmDecoder::last_timings`].
///
/// `decode_ms` is the time spent decoding and `convert_ms` the time spent
/// converting the result to RGBA8 JavaScript objects, both in milliseconds.
#[derive(Serialize, Tsify, Clone, Copy, Default)]
pub struct JsTimings {
    pub decode_ms: f64,
    pub convert_ms: f64,
}

/// Parses a standalone ICC color profile from raw bytes.
///
/// Returns a structured representation of the profile header, tag table, and
/// all recognized tag data.
#[wasm_bindgen(js_name = parseIcc)]
pub fn wasm_parse_icc(data: &[u8]) -> Result<Ts<ICCProfile>, JsError> {
    parse_icc(data)?.into_ts().map_err(js_error)
}

/// Sets the minimum log level for all decoders.
///
/// Accepts `"Debug"`, `"Warning"`, `"Error"`, or `"None"` (case-sensitive).
/// Throws if the value is not one of those.
#[wasm_bindgen(js_name = setLogLevel)]
pub fn wasm_set_log_level(
    #[wasm_bindgen(unchecked_param_type = "\"Debug\" | \"Warning\" | \"Error\" | \"None\"")] level: &str,
) -> Result<(), JsError> {
    let level = match level {
        "Debug" => LogLevel::Debug,
        "Warning" => LogLevel::Warning,
        "Error" => LogLevel::Error,
        "None" => LogLevel::None,
        other => return Err(JsError::new(&format!("unknown log level: {}", other))),
    };
    set_log_level(level);
    Ok(())
}
