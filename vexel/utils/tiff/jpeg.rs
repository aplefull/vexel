pub fn splice_jpeg_tables(tables: &[u8], strip: &[u8]) -> Vec<u8> {
    if tables.len() < 4 || strip.len() < 2 {
        return strip.to_vec();
    }

    let tables_body = if tables.starts_with(&[0xFF, 0xD8]) && tables.ends_with(&[0xFF, 0xD9]) {
        &tables[2..tables.len() - 2]
    } else {
        tables
    };

    if strip.starts_with(&[0xFF, 0xD8]) {
        let mut result = Vec::with_capacity(2 + tables_body.len() + strip.len() - 2);
        result.extend_from_slice(&[0xFF, 0xD8]);
        result.extend_from_slice(tables_body);
        result.extend_from_slice(&strip[2..]);
        result
    } else {
        let mut result = Vec::with_capacity(2 + tables_body.len() + strip.len());
        result.extend_from_slice(&[0xFF, 0xD8]);
        result.extend_from_slice(tables_body);
        result.extend_from_slice(strip);
        result
    }
}
