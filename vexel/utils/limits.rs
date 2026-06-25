use crate::utils::error::{VexelError, VexelResult};

/// Resource limits enforced during decoding.
///
/// `None` for a field means that dimension or allocation is unconstrained.
/// The default limits allow any image dimensions but cap total pixel memory at 512 MiB.
/// Use [`no_limits`](Limits::no_limits) to remove all constraints, or construct
/// the struct directly to set only the fields you care about.
#[derive(Clone, Debug, PartialEq)]
pub struct Limits {
    /// Maximum permitted image width in pixels. `None` means no limit.
    pub max_image_width: Option<u32>,
    /// Maximum permitted image height in pixels. `None` means no limit.
    pub max_image_height: Option<u32>,
    /// Maximum number of bytes that may be allocated for pixel data across
    /// the entire decode operation. Tracked as a decreasing budget: each
    /// allocation subtracts from it; `None` means no limit. Default is 512 MiB.
    pub max_alloc: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_image_width: None,
            max_image_height: None,
            max_alloc: Some(512 * 1024 * 1024),
        }
    }
}

impl Limits {
    /// Returns a `Limits` with all fields set to `None`, disabling every constraint.
    pub fn no_limits() -> Self {
        Self {
            max_image_width: None,
            max_image_height: None,
            max_alloc: None,
        }
    }

    pub(crate) fn check_dimensions(&self, width: u32, height: u32) -> VexelResult<()> {
        if let Some(max_w) = self.max_image_width {
            if width > max_w {
                return Err(VexelError::LimitExceeded(format!(
                    "image width {} exceeds limit {}",
                    width, max_w
                )));
            }
        }
        if let Some(max_h) = self.max_image_height {
            if height > max_h {
                return Err(VexelError::LimitExceeded(format!(
                    "image height {} exceeds limit {}",
                    height, max_h
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn reserve(&mut self, bytes: u64) -> VexelResult<()> {
        if let Some(remaining) = self.max_alloc.as_mut() {
            if *remaining < bytes {
                return Err(VexelError::LimitExceeded(format!(
                    "allocation of {} bytes would exceed remaining budget of {} bytes",
                    bytes, remaining
                )));
            }
            *remaining -= bytes;
        }
        Ok(())
    }

    pub(crate) fn reserve_usize(&mut self, bytes: usize) -> VexelResult<()> {
        match u64::try_from(bytes) {
            Ok(n) => self.reserve(n),
            Err(_) if self.max_alloc.is_some() => Err(VexelError::LimitExceeded(
                "allocation size overflows u64".to_string(),
            )),
            Err(_) => Ok(()),
        }
    }

    pub(crate) fn reserve_buffer(&mut self, width: u32, height: u32, bytes_per_pixel: u8) -> VexelResult<()> {
        self.check_dimensions(width, height)?;
        let size = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(u64::from(bytes_per_pixel));
        self.reserve(size)
    }

    pub(crate) fn free(&mut self, bytes: u64) {
        if let Some(remaining) = self.max_alloc.as_mut() {
            *remaining = remaining.saturating_add(bytes);
        }
    }

    pub(crate) fn free_usize(&mut self, bytes: usize) {
        if let Ok(n) = u64::try_from(bytes) {
            self.free(n);
        }
    }
}
