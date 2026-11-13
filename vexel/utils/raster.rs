use std::marker::PhantomData;

pub struct RasterWindow<'a, T> {
    ptr: *mut T,
    stride: usize,
    width: usize,
    height: usize,
    _marker: PhantomData<&'a mut [T]>,
}

unsafe impl<T: Send> Send for RasterWindow<'_, T> {}
unsafe impl<T: Sync> Sync for RasterWindow<'_, T> {}

impl<'a, T> RasterWindow<'a, T> {
    pub fn new(buf: &'a mut [T], stride: usize, width: usize, height: usize) -> Self {
        let width = width.min(stride);
        let height = if width == 0 || stride == 0 {
            0
        } else {
            let full_rows = buf.len() / stride;
            let partial = usize::from(buf.len() % stride >= width);
            height.min(full_rows + partial)
        };
        Self { ptr: buf.as_mut_ptr(), stride, width, height, _marker: PhantomData }
    }

    pub fn contiguous(buf: &'a mut [T], width: usize) -> Self {
        let height = buf.len().checked_div(width).unwrap_or(0);
        Self::new(buf, width.max(1), width, height)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    pub fn row_mut(&mut self, y: usize) -> &mut [T] {
        assert!(y < self.height);
        unsafe { std::slice::from_raw_parts_mut(self.ptr.add(y * self.stride), self.width) }
    }

    pub fn split_rows(self, at: usize) -> (Self, Self) {
        let at = at.min(self.height);
        let top = Self { ptr: self.ptr, stride: self.stride, width: self.width, height: at, _marker: PhantomData };
        let bottom_ptr = if at < self.height { unsafe { self.ptr.add(at * self.stride) } } else { self.ptr };
        let bottom = Self {
            ptr: bottom_ptr,
            stride: self.stride,
            width: self.width,
            height: self.height - at,
            _marker: PhantomData,
        };
        (top, bottom)
    }

    pub fn split_columns(self, at: usize) -> (Self, Self) {
        let at = at.min(self.width);
        let left = Self { ptr: self.ptr, stride: self.stride, width: at, height: self.height, _marker: PhantomData };
        let right_ptr = if at < self.width { unsafe { self.ptr.add(at) } } else { self.ptr };
        let right = Self {
            ptr: right_ptr,
            stride: self.stride,
            width: self.width - at,
            height: self.height,
            _marker: PhantomData,
        };
        (left, right)
    }

    pub fn into_row_bands(self, rows_per_band: usize) -> Vec<Self> {
        let rows_per_band = rows_per_band.max(1);
        let mut bands = Vec::with_capacity(self.height.div_ceil(rows_per_band));
        let mut rest = self;
        while rest.height > 0 {
            let (band, tail) = rest.split_rows(rows_per_band);
            bands.push(band);
            rest = tail;
        }
        bands
    }

    pub fn into_column_tiles(self, columns_per_tile: usize, tiles: usize) -> Vec<Self> {
        let columns_per_tile = columns_per_tile.max(1);
        let mut out = Vec::with_capacity(tiles);
        let mut rest = self;
        for _ in 0..tiles {
            let (tile, tail) = rest.split_columns(columns_per_tile);
            out.push(tile);
            rest = tail;
        }
        out
    }
}
