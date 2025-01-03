use serde::Serialize;
use std::collections::HashMap;
use std::io::{Cursor, Read, Result, Seek, SeekFrom};
use tsify::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ICCProfile {
    pub header: ProfileHeader,
    pub tag_table: TagTable,
    pub tag_data: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ProfileHeader {
    pub size: u32,
    pub preferred_cmm_type: u32,
    pub version: u32,
    pub profile_class: String,
    pub color_space: String,
    pub pcs: String,
    pub creation_date: [u8; 12],
    pub signature: String,
    pub platform: String,
    pub flags: u32,
    pub manufacturer: String,
    pub model: String,
    pub attributes: [u8; 8],
    pub rendering_intent: u32,
    pub illuminant: [u32; 3],
    pub creator: String,
    pub profile_id: [u8; 16],
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TagTable {
    pub tag_count: u32,
    pub entries: Vec<TagEntry>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TagEntry {
    pub signature: String,
    pub offset: u32,
    pub size: u32,
}

impl ICCProfile {
    pub fn new(data: &[u8]) -> Result<ICCProfile> {
        let mut reader = Cursor::new(data);
        let header = ICCProfile::read_header(&mut reader)?;
        let tag_table = ICCProfile::read_tag_table(&mut reader)?;
        let tag_data = ICCProfile::read_tag_data(&mut reader, &tag_table)?;

        Ok(ICCProfile {
            header,
            tag_table,
            tag_data,
        })
    }

    fn read_header<R: Read>(reader: &mut R) -> Result<ProfileHeader> {
        let mut buffer = [0u8; 128];
        reader.read_exact(&mut buffer)?;

        let size = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
        let preferred_cmm_type = u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]);
        let version = u32::from_be_bytes([buffer[8], buffer[9], buffer[10], buffer[11]]);

        let profile_class = String::from_utf8_lossy(&buffer[12..16]).to_string();
        let color_space = String::from_utf8_lossy(&buffer[16..20]).to_string();
        let pcs = String::from_utf8_lossy(&buffer[20..24]).to_string();

        let mut creation_date = [0u8; 12];
        creation_date.copy_from_slice(&buffer[24..36]);

        let signature = String::from_utf8_lossy(&buffer[36..40]).to_string();
        let platform = String::from_utf8_lossy(&buffer[40..44]).to_string();

        let flags = u32::from_be_bytes([buffer[44], buffer[45], buffer[46], buffer[47]]);
        let manufacturer = String::from_utf8_lossy(&buffer[48..52]).to_string();
        let model = String::from_utf8_lossy(&buffer[52..56]).to_string();

        let mut attributes = [0u8; 8];
        attributes.copy_from_slice(&buffer[56..64]);

        let rendering_intent = u32::from_be_bytes([buffer[64], buffer[65], buffer[66], buffer[67]]);

        let illuminant = [
            u32::from_be_bytes([buffer[68], buffer[69], buffer[70], buffer[71]]),
            u32::from_be_bytes([buffer[72], buffer[73], buffer[74], buffer[75]]),
            u32::from_be_bytes([buffer[76], buffer[77], buffer[78], buffer[79]]),
        ];

        let creator = String::from_utf8_lossy(&buffer[80..84]).to_string();

        let mut profile_id = [0u8; 16];
        profile_id.copy_from_slice(&buffer[84..100]);

        Ok(ProfileHeader {
            size,
            preferred_cmm_type,
            version,
            profile_class,
            color_space,
            pcs,
            creation_date,
            signature,
            platform,
            flags,
            manufacturer,
            model,
            attributes,
            rendering_intent,
            illuminant,
            creator,
            profile_id,
        })
    }

    fn read_tag_table<R: Read>(reader: &mut R) -> Result<TagTable> {
        let mut buffer = [0u8; 4];
        reader.read_exact(&mut buffer)?;
        let tag_count = u32::from_be_bytes(buffer);

        let mut entries = Vec::with_capacity(tag_count as usize);
        for _ in 0..tag_count {
            let mut entry_buffer = [0u8; 12];
            reader.read_exact(&mut entry_buffer)?;

            let signature = String::from_utf8_lossy(&entry_buffer[0..4]).to_string();
            let offset = u32::from_be_bytes([entry_buffer[4], entry_buffer[5], entry_buffer[6], entry_buffer[7]]);
            let size = u32::from_be_bytes([entry_buffer[8], entry_buffer[9], entry_buffer[10], entry_buffer[11]]);

            entries.push(TagEntry {
                signature,
                offset,
                size,
            });
        }

        Ok(TagTable { tag_count, entries })
    }

    fn read_tag_data<R: Read + Seek>(reader: &mut R, tag_table: &TagTable) -> Result<HashMap<String, Vec<u8>>> {
        let mut tag_data = HashMap::new();

        for entry in &tag_table.entries {
            reader.seek(SeekFrom::Start(entry.offset as u64))?;
            let mut data = vec![0u8; entry.size as usize];
            reader.read_exact(&mut data)?;
            tag_data.insert(entry.signature.clone(), data);
        }

        Ok(tag_data)
    }

    pub fn log_info(&self) {
        println!("ICC Profile Information:");
        println!("----------------------");
        println!("Profile Size: {} bytes", self.header.size);
        println!("Version: 0x{:08X}", self.header.version);
        println!("Profile Class: {}", self.header.profile_class);
        println!("Color Space: {}", self.header.color_space);
        println!("PCS: {}", self.header.pcs);
        println!("Platform: {}", self.header.platform);
        println!("Manufacturer: {}", self.header.manufacturer);
        println!("Model: {}", self.header.model);
        println!("\nTag Table:");
        println!("Number of Tags: {}", self.tag_table.tag_count);
        for entry in &self.tag_table.entries {
            println!(
                "  {} - Offset: {}, Size: {} bytes",
                entry.signature, entry.offset, entry.size
            );
        }
    }
}
