pub(crate) trait SafeAccess<T> {
    fn get_safe(&self, index: usize) -> Result<&T, std::io::Error>;
    fn get_range_safe(&self, range: std::ops::Range<usize>) -> Result<&[T], std::io::Error>;
    fn check_range(&self, range: std::ops::Range<usize>) -> Result<(), std::io::Error>;
}

impl<T> SafeAccess<T> for [T] {
    /// Safely retrieves a reference to an element at the specified index in a slice.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the element to retrieve.
    ///
    /// # Returns
    ///
    /// * `Ok(&T)` - A reference to the element at the specified index if it exists.
    /// * `Err(std::io::Error)` - An error if the index is out of bounds.
    ///
    /// # Errors
    ///
    /// This function will return an `std::io::Error` with `std::io::ErrorKind::InvalidData`
    /// if the index is out of bounds.
    fn get_safe(&self, index: usize) -> Result<&T, std::io::Error> {
        self.get(index).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Index {} out of bounds (len {})", index, self.len()),
            )
        })
    }

    /// Safely retrieves a reference to a range of elements in a slice.
    ///
    /// # Arguments
    ///
    /// * `range` - The range of elements to retrieve.
    ///
    /// # Returns
    ///
    /// * `Ok(&[T])` - A reference to the range of elements if it exists.
    /// * `Err(std::io::Error)` - An error if the range is out of bounds.
    ///
    /// # Errors
    ///
    /// This function will return an `std::io::Error` with `std::io::ErrorKind::InvalidData`
    /// if the range is out of bounds.
    fn get_range_safe(&self, range: std::ops::Range<usize>) -> Result<&[T], std::io::Error> {
        self.get(range.clone()).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Range {}..{} out of bounds (len {})",
                    range.start,
                    range.end,
                    self.len()
                ),
            )
        })
    }

    /// Checks if a range is valid for this slice without actually retrieving the elements.
    ///
    /// # Arguments
    ///
    /// * `range` - The range to check.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the range is valid.
    /// * `Err(std::io::Error)` - An error if the range is invalid.
    ///
    /// # Errors
    ///
    /// This function will return an `std::io::Error` with `std::io::ErrorKind::InvalidData` if:
    /// - The range start is greater than the range end
    /// - The range end is greater than the slice length
    fn check_range(&self, range: std::ops::Range<usize>) -> Result<(), std::io::Error> {
        if range.start > range.end {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid range: start ({}) > end ({})", range.start, range.end),
            ));
        }

        if range.end > self.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Range end {} out of bounds (len {})", range.end, self.len()),
            ));
        }

        Ok(())
    }
}
