use std::borrow::Cow;

use crate::log_warn;

const MAX_DEPTH: usize = 128;
const MAX_NODES: usize = 1 << 16;

#[derive(Debug, Clone, Default)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum Node {
    Element(Element),
    Text(String),
}

impl Element {
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    pub fn text(&self) -> String {
        self.children
            .iter()
            .filter_map(|node| match node {
                Node::Text(text) => Some(text.as_str()),
                Node::Element(_) => None,
            })
            .collect()
    }
}

pub fn decode_entities(text: &str) -> Cow<'_, str> {
    if !text.contains('&') {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let name_length = rest.as_bytes()[1..]
            .iter()
            .position(|&b| !(b.is_ascii_alphanumeric() || b == b'#'))
            .unwrap_or(rest.len() - 1);
        let end = Some(name_length + 1).filter(|&end| rest.as_bytes().get(end) == Some(&b';'));
        let decoded = end.and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => {
                    let number = entity.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code)
                }
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

fn drop_indentation(element: &mut Element) {
    if element.children.iter().any(|node| matches!(node, Node::Element(_))) {
        element.children.retain(|node| !matches!(node, Node::Text(text) if text.trim().is_empty()));
    }
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
    stack: Vec<Element>,
    skipped_depth: usize,
    nodes: usize,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn skip_past(&mut self, terminator: &str) -> Option<&'a str> {
        let rest = self.rest();
        match rest.find(terminator) {
            Some(end) => {
                self.pos += end + terminator.len();
                Some(&rest[..end])
            }
            None => {
                self.pos = self.text.len();
                None
            }
        }
    }

    fn count_node(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes == MAX_NODES {
            log_warn!("XML has more than {} nodes, ignoring the rest", MAX_NODES);
        }
        self.nodes < MAX_NODES
    }

    fn push_text(&mut self, text: Cow<'_, str>) {
        if text.is_empty() {
            return;
        }
        if let Some(Node::Text(previous)) = self.stack.last_mut().and_then(|parent| parent.children.last_mut()) {
            previous.push_str(&text);
            return;
        }
        if !text.trim().is_empty() && !self.count_node() {
            return;
        }
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(Node::Text(text.into_owned()));
        }
    }

    fn pop(&mut self) {
        if self.stack.len() < 2 {
            return;
        }
        if let Some(mut element) = self.stack.pop() {
            drop_indentation(&mut element);
            if let Some(parent) = self.stack.last_mut() {
                parent.children.push(Node::Element(element));
            }
        }
    }

    fn close(&mut self, name: &str) {
        if self.skipped_depth > 0 {
            self.skipped_depth -= 1;
            return;
        }
        let Some(index) = self.stack.iter().skip(1).rposition(|element| element.name == name) else {
            return;
        };
        while self.stack.len() > index + 1 {
            self.pop();
        }
    }

    fn open(&mut self, element: Element, self_closing: bool) {
        if self.skipped_depth > 0 || self.stack.len() > MAX_DEPTH {
            if !self_closing {
                self.skipped_depth += 1;
            }
            return;
        }
        if !self.count_node() {
            return;
        }
        self.stack.push(element);
        if self_closing {
            self.pop();
        }
    }

    fn start_tag(&mut self) -> bool {
        let bytes = self.text.as_bytes();
        let mut pos = self.pos + 1;
        let name_start = pos;
        while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() && bytes[pos] != b'>' && bytes[pos] != b'/' {
            pos += 1;
        }
        let mut element = Element { name: self.text[name_start..pos].to_string(), ..Default::default() };

        loop {
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            match bytes.get(pos) {
                None => return false,
                Some(b'>') => {
                    self.pos = pos + 1;
                    self.open(element, false);
                    return true;
                }
                Some(b'/') => {
                    if bytes.get(pos + 1) == Some(&b'>') {
                        self.pos = pos + 2;
                        self.open(element, true);
                        return true;
                    }
                    pos += 1;
                    continue;
                }
                Some(_) => {}
            }

            let attribute_start = pos;
            while pos < bytes.len() && !matches!(bytes[pos], b'=' | b'>' | b'/') && !bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let attribute = &self.text[attribute_start..pos];
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if bytes.get(pos) != Some(&b'=') {
                if !attribute.is_empty() {
                    element.attributes.push((attribute.to_string(), String::new()));
                }
                continue;
            }
            pos += 1;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let value = match bytes.get(pos) {
                Some(&quote @ (b'"' | b'\'')) => {
                    let value_start = pos + 1;
                    let Some(length) = bytes[value_start..].iter().position(|&b| b == quote) else {
                        return false;
                    };
                    pos = value_start + length + 1;
                    &self.text[value_start..value_start + length]
                }
                _ => {
                    let value_start = pos;
                    while pos < bytes.len() && bytes[pos] != b'>' && !bytes[pos].is_ascii_whitespace() {
                        pos += 1;
                    }
                    &self.text[value_start..pos]
                }
            };
            let value = value.replace(['\t', '\n'], " ");
            element.attributes.push((attribute.to_string(), decode_entities(&value).into_owned()));
        }
    }

    fn run(&mut self) {
        while self.pos < self.text.len() && self.nodes < MAX_NODES {
            let rest = self.rest();
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                self.pos += end;
                self.push_text(decode_entities(&rest[..end]));
                continue;
            }

            if rest.starts_with("<!--") {
                self.pos += 4;
                self.skip_past("-->");
            } else if rest.starts_with("<![CDATA[") {
                self.pos += 9;
                if let Some(data) = self.skip_past("]]>") {
                    self.push_text(Cow::Borrowed(data));
                }
            } else if rest.starts_with("<?") {
                self.pos += 2;
                self.skip_past("?>");
            } else if rest.starts_with("<!") {
                let bracket = rest.find('[');
                let close = rest.find('>');
                match (bracket, close) {
                    (Some(bracket), Some(close)) if bracket < close => self.skip_past("]>"),
                    _ => self.skip_past(">"),
                };
            } else if rest.starts_with("</") {
                self.pos += 2;
                let name = self.skip_past(">").unwrap_or_default().trim();
                self.close(name);
            } else if !self.start_tag() {
                log_warn!("Unterminated XML tag at offset {}", self.pos);
                break;
            }
        }

        while self.stack.len() > 1 {
            self.pop();
        }
    }
}

pub fn parse(text: &str) -> Element {
    let normalized;
    let text = if text.contains('\r') {
        normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        normalized.as_str()
    } else {
        text
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parser = Parser { text, pos: 0, stack: vec![Element::default()], skipped_depth: 0, nodes: 0 };
    parser.run();
    let mut root = parser.stack.pop().unwrap_or_default();
    drop_indentation(&mut root);
    root
}
