pub(crate) trait SafeAccess<T> {
    fn get_safe(&self, index: usize) -> Result<&T, std::io::Error>;
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
}
