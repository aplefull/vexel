use serde::Serialize;

use super::xml::{self, Element};
use crate::log_warn;
use crate::utils::info::InfoWriter;

const PACKET_END: &[u8] = b"<?xpacket end=";
const XMPMETA_END: &[u8] = b"</x:xmpmeta>";
const XMP_MARKERS: [&str; 4] = ["x:xmpmeta", "x:xapmeta", "<?xpacket", "rdf:RDF"];

pub const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub const TIFF_NS: &str = "http://ns.adobe.com/tiff/1.0/";
pub const XMP_NOTE_NS: &str = "http://ns.adobe.com/xmp/note/";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const MAX_NAMESPACE_BINDINGS: usize = 256;
const WELL_KNOWN_PREFIXES: [(&str, &str); 10] = [
    ("rdf", RDF_NS),
    ("x", "adobe:ns:meta/"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("xmp", "http://ns.adobe.com/xap/1.0/"),
    ("xap", "http://ns.adobe.com/xap/1.0/"),
    ("xmpMM", "http://ns.adobe.com/xap/1.0/mm/"),
    ("xmpNote", XMP_NOTE_NS),
    ("tiff", TIFF_NS),
    ("exif", "http://ns.adobe.com/exif/1.0/"),
    ("photoshop", "http://ns.adobe.com/photoshop/1.0/"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum XmpArrayKind {
    Bag,
    Seq,
    Alt,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum XmpValue {
    Text {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
    },
    Array {
        kind: XmpArrayKind,
        items: Vec<XmpValue>,
    },
    Struct(Vec<XmpProperty>),
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct XmpProperty {
    pub namespace: String,
    pub name: String,
    pub value: XmpValue,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(missing_as_null))]
pub struct XmpData {
    pub packet: String,
    pub extended: Option<String>,
    pub properties: Vec<XmpProperty>,
}

impl XmpProperty {
    pub fn local_name(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }
}

impl XmpValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            XmpValue::Text { value, .. } => Some(value),
            _ => None,
        }
    }

    fn is_text(&self) -> bool {
        matches!(self, XmpValue::Text { .. })
    }
}

fn text_with_lang(value: &str, lang: Option<&str>) -> String {
    match lang {
        Some(lang) if !lang.eq_ignore_ascii_case("x-default") => format!("{} ({})", value, lang),
        _ => value.to_string(),
    }
}

fn describe_value(out: &mut InfoWriter, label: &str, value: &XmpValue) {
    match value {
        XmpValue::Text { value, lang } => {
            out.text(label, &text_with_lang(value, lang.as_deref()));
        }
        XmpValue::Array { items, .. } if items.iter().all(XmpValue::is_text) => {
            let joined: Vec<String> = items
                .iter()
                .filter_map(|item| match item {
                    XmpValue::Text { value, lang } => Some(text_with_lang(value, lang.as_deref())),
                    _ => None,
                })
                .collect();
            out.text(label, &joined.join(", "));
        }
        XmpValue::Array { items, .. } => {
            out.group(format!("{} ({} items)", label, items.len()), |out| {
                for (index, item) in items.iter().enumerate() {
                    describe_value(out, &format!("[{}]", index + 1), item);
                }
            });
        }
        XmpValue::Struct(fields) => {
            out.group(label.to_string(), |out| {
                for field in fields {
                    describe_value(out, &field.name, &field.value);
                }
            });
        }
    }
}

impl XmpData {
    pub fn new(packet: String, extended: Option<String>) -> Self {
        let mut properties = parse_properties(&packet);
        if let Some(extended) = &extended {
            properties.extend(parse_properties(extended));
        }
        XmpData { packet, extended, properties }
    }

    pub fn find(&self, namespace: &str, local_name: &str) -> Option<&XmpValue> {
        self.properties
            .iter()
            .find(|property| property.namespace == namespace && property.local_name() == local_name)
            .map(|property| &property.value)
    }

    pub fn describe_properties(&self, out: &mut InfoWriter) {
        for property in &self.properties {
            describe_value(out, &property.name, &property.value);
        }
    }
}

pub fn extended_guid(packet: &str) -> Option<String> {
    let properties = parse_properties(packet);
    let property = properties
        .iter()
        .find(|property| property.namespace == XMP_NOTE_NS && property.local_name() == "HasExtendedXMP")?;
    property.value.as_text().map(|guid| guid.trim().to_string())
}

struct Namespaces {
    bindings: Vec<(String, String)>,
    overflowed: bool,
}

impl Namespaces {
    fn enter(&mut self, element: &Element) -> usize {
        let mark = self.bindings.len();
        for (name, value) in &element.attributes {
            let prefix = match name.strip_prefix("xmlns") {
                Some("") => "",
                Some(rest) => match rest.strip_prefix(':') {
                    Some(prefix) => prefix,
                    None => continue,
                },
                None => continue,
            };
            if self.bindings.len() >= MAX_NAMESPACE_BINDINGS {
                if !self.overflowed {
                    log_warn!("XMP declares more than {} namespaces, ignoring the rest", MAX_NAMESPACE_BINDINGS);
                    self.overflowed = true;
                }
                break;
            }
            self.bindings.push((prefix.to_string(), value.clone()));
        }
        mark
    }

    fn leave(&mut self, mark: usize) {
        self.bindings.truncate(mark);
    }

    fn resolve<'n>(&self, name: &'n str) -> (String, &'n str) {
        let (prefix, local) = match name.split_once(':') {
            Some((prefix, local)) => (prefix, local),
            None => ("", name),
        };
        if prefix == "xml" {
            return (XML_NS.to_string(), local);
        }
        let bound = self.bindings.iter().rev().find(|(bound, _)| bound == prefix).map(|(_, uri)| uri.clone());
        let namespace = bound
            .or_else(|| WELL_KNOWN_PREFIXES.iter().find(|(known, _)| *known == prefix).map(|(_, uri)| uri.to_string()));
        (namespace.unwrap_or_default(), local)
    }

    fn is(&self, name: &str, namespace: &str, local: &str) -> bool {
        let (resolved, resolved_local) = self.resolve(name);
        resolved == namespace && resolved_local == local
    }

    fn is_syntax_attribute(&self, name: &str) -> bool {
        if name == "xmlns" || name.starts_with("xmlns:") || !name.contains(':') {
            return true;
        }
        let namespace = self.resolve(name).0;
        namespace == XML_NS || namespace == RDF_NS
    }

    fn attribute<'e>(&self, element: &'e Element, namespace: &str, local: &str) -> Option<&'e str> {
        element.attributes.iter().find(|(name, _)| self.is(name, namespace, local)).map(|(_, value)| value.as_str())
    }
}

fn parse_properties(packet: &str) -> Vec<XmpProperty> {
    let root = xml::parse(packet);
    let mut namespaces = Namespaces { bindings: Vec::new(), overflowed: false };
    let mut properties = Vec::new();
    collect_rdf(&root, &mut namespaces, &mut properties);
    properties
}

fn collect_rdf(element: &Element, namespaces: &mut Namespaces, properties: &mut Vec<XmpProperty>) {
    for child in element.elements() {
        let mark = namespaces.enter(child);
        if namespaces.is(&child.name, RDF_NS, "RDF") {
            for node in child.elements() {
                let node_mark = namespaces.enter(node);
                properties.extend(node_properties(node, namespaces));
                namespaces.leave(node_mark);
            }
        } else {
            collect_rdf(child, namespaces, properties);
        }
        namespaces.leave(mark);
    }
}

fn node_properties(node: &Element, namespaces: &mut Namespaces) -> Vec<XmpProperty> {
    let mut properties: Vec<XmpProperty> = node
        .attributes
        .iter()
        .filter(|(name, _)| !namespaces.is_syntax_attribute(name))
        .map(|(name, value)| XmpProperty {
            namespace: namespaces.resolve(name).0,
            name: name.clone(),
            value: XmpValue::Text { value: value.clone(), lang: None },
        })
        .collect();

    for child in node.elements() {
        let mark = namespaces.enter(child);
        if namespaces.resolve(&child.name).0 != RDF_NS {
            properties.push(XmpProperty {
                namespace: namespaces.resolve(&child.name).0,
                name: child.name.clone(),
                value: property_value(child, namespaces),
            });
        }
        namespaces.leave(mark);
    }

    properties
}

fn property_value(element: &Element, namespaces: &mut Namespaces) -> XmpValue {
    let lang = namespaces.attribute(element, XML_NS, "lang").map(str::to_string);
    if let Some(resource) = namespaces.attribute(element, RDF_NS, "resource") {
        return XmpValue::Text { value: resource.to_string(), lang };
    }
    if namespaces.attribute(element, RDF_NS, "parseType") == Some("Resource") {
        return XmpValue::Struct(node_properties(element, namespaces));
    }

    let Some(first) = element.elements().next() else {
        let has_fields = element.attributes.iter().any(|(name, _)| !namespaces.is_syntax_attribute(name));
        if has_fields {
            return XmpValue::Struct(node_properties(element, namespaces));
        }
        return XmpValue::Text { value: element.text(), lang };
    };

    let mark = namespaces.enter(first);
    let kind = [("Bag", XmpArrayKind::Bag), ("Seq", XmpArrayKind::Seq), ("Alt", XmpArrayKind::Alt)]
        .into_iter()
        .find(|(local, _)| namespaces.is(&first.name, RDF_NS, local))
        .map(|(_, kind)| kind);

    let value = if let Some(kind) = kind {
        let mut items = Vec::new();
        for item in first.elements() {
            let item_mark = namespaces.enter(item);
            if namespaces.is(&item.name, RDF_NS, "li") {
                items.push(property_value(item, namespaces));
            }
            namespaces.leave(item_mark);
        }
        XmpValue::Array { kind, items }
    } else if namespaces.is(&first.name, RDF_NS, "Description") {
        qualified_value(first, namespaces).unwrap_or_else(|| XmpValue::Struct(node_properties(first, namespaces)))
    } else {
        namespaces.leave(mark);
        return XmpValue::Struct(node_properties(element, namespaces));
    };
    namespaces.leave(mark);
    value
}

fn qualified_value(description: &Element, namespaces: &mut Namespaces) -> Option<XmpValue> {
    if let Some(value) = namespaces.attribute(description, RDF_NS, "value") {
        return Some(XmpValue::Text { value: value.to_string(), lang: None });
    }
    let element = description.elements().find(|child| namespaces.is(&child.name, RDF_NS, "value"))?;
    let mark = namespaces.enter(element);
    let value = property_value(element, namespaces);
    namespaces.leave(mark);
    Some(value)
}

pub fn decode_packet(bytes: &[u8]) -> Option<String> {
    let text = match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, u16::from_be_bytes),
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, u16::from_le_bytes),
        [0, b'<', ..] => decode_utf16(bytes, u16::from_be_bytes),
        [b'<', 0, ..] => decode_utf16(bytes, u16::from_le_bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    let text = text.trim_end_matches(['\0', ' ', '\t', '\r', '\n']);
    if text.trim_start().is_empty() {
        return None;
    }
    if !XMP_MARKERS.iter().any(|marker| text.contains(marker)) {
        log_warn!("Ignoring {} bytes of metadata that do not look like XMP", bytes.len());
        return None;
    }
    Some(text.to_string())
}

fn decode_utf16(bytes: &[u8], read: fn([u8; 2]) -> u16) -> String {
    let units = bytes.chunks_exact(2).map(|pair| read([pair[0], pair[1]]));
    char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

pub fn trim_to_packet(bytes: &[u8]) -> &[u8] {
    if let Some(start) = find(bytes, PACKET_END) {
        if let Some(close) = find(&bytes[start..], b"?>") {
            return &bytes[..start + close + 2];
        }
    }
    if let Some(start) = find(bytes, XMPMETA_END) {
        return &bytes[..start + XMPMETA_END.len()];
    }
    bytes
}

pub fn looks_like_xmp(bytes: &[u8]) -> bool {
    let window = &bytes[..bytes.len().min(4096)];
    XMP_MARKERS.iter().any(|marker| find(window, marker.as_bytes()).is_some())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[derive(Debug, Clone)]
struct ExtendedChunk {
    guid: String,
    full_length: u32,
    offset: u32,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct ExtendedXmpAssembler {
    chunks: Vec<ExtendedChunk>,
}

impl ExtendedXmpAssembler {
    pub fn add(&mut self, guid: String, full_length: u32, offset: u32, data: &[u8]) {
        self.chunks.push(ExtendedChunk { guid, full_length, offset, data: data.to_vec() });
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn assemble(&self, main_packet: Option<&str>) -> Option<String> {
        let wanted = main_packet.and_then(extended_guid);
        let guid = match wanted {
            Some(guid) if self.chunks.iter().any(|chunk| chunk.guid == guid) => guid,
            Some(guid) => {
                log_warn!("Extended XMP GUID {} named by the main packet has no chunks, using the first", guid);
                self.chunks.first()?.guid.clone()
            }
            None => self.chunks.first()?.guid.clone(),
        };

        let mut chunks: Vec<&ExtendedChunk> = self.chunks.iter().filter(|chunk| chunk.guid == guid).collect();
        chunks.sort_by_key(|chunk| chunk.offset);
        let declared = chunks.first().map_or(0, |chunk| u64::from(chunk.full_length));

        let mut buffer = Vec::with_capacity(chunks.iter().map(|chunk| chunk.data.len()).sum());
        let mut position = 0u64;
        let mut gaps = false;
        for chunk in chunks {
            let start = u64::from(chunk.offset);
            let end = start + chunk.data.len() as u64;
            if end <= position {
                continue;
            }
            if start > position {
                gaps = true;
            }
            let skip = position.saturating_sub(start) as usize;
            buffer.extend_from_slice(&chunk.data[skip..]);
            position = end;
        }
        if gaps || position < declared {
            log_warn!("Extended XMP is incomplete: {} of {} bytes present", buffer.len(), declared.max(position));
        }

        decode_packet(&buffer)
    }
}
