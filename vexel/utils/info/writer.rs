use std::borrow::Cow;
use std::fmt;

const TEXT_PREVIEW_CHARS: usize = 60;

#[derive(Debug, Clone)]
pub enum InfoValue {
    Plain(String),
    Text(String),
}

#[derive(Debug, Clone)]
pub enum InfoNode {
    Field { label: String, value: InfoValue },
    Group(InfoGroup),
}

#[derive(Debug, Clone, Default)]
pub struct InfoGroup {
    pub title: String,
    pub offset: Option<u64>,
    pub length: Option<u64>,
    pub children: Vec<InfoNode>,
}

pub struct InfoWriter {
    group: InfoGroup,
}

impl InfoWriter {
    pub fn new(title: impl Into<String>) -> Self {
        InfoWriter { group: InfoGroup { title: title.into(), ..Default::default() } }
    }

    pub fn field(&mut self, label: &str, value: impl fmt::Display) -> &mut Self {
        self.group
            .children
            .push(InfoNode::Field { label: label.to_string(), value: InfoValue::Plain(value.to_string()) });
        self
    }

    pub fn opt_field<T: fmt::Display>(&mut self, label: &str, value: Option<T>) -> &mut Self {
        if let Some(value) = value {
            self.field(label, value);
        }
        self
    }

    pub fn text(&mut self, label: &str, value: &str) -> &mut Self {
        self.group
            .children
            .push(InfoNode::Field { label: label.to_string(), value: InfoValue::Text(value.to_string()) });
        self
    }

    pub fn group(&mut self, title: impl Into<String>, describe: impl FnOnce(&mut InfoWriter)) -> &mut Self {
        let mut child = InfoWriter::new(title);
        describe(&mut child);
        self.group.children.push(InfoNode::Group(child.finish()));
        self
    }

    pub fn section(
        &mut self,
        title: impl Into<String>,
        offset: u64,
        length: Option<u64>,
        describe: impl FnOnce(&mut InfoWriter),
    ) -> &mut Self {
        let mut child = InfoWriter::new(title);
        child.group.offset = Some(offset);
        child.group.length = length;
        describe(&mut child);
        self.group.children.push(InfoNode::Group(child.finish()));
        self
    }

    pub fn finish(self) -> InfoGroup {
        self.group
    }
}

fn escape_control(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return Cow::Borrowed(text);
    }

    let mut escaped = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => escaped.push_str(&format!("\\x{:02X}", u32::from(c))),
            c => escaped.push(c),
        }
    }
    Cow::Owned(escaped)
}

fn text_preview(text: &str) -> String {
    let char_count = text.chars().count();
    if char_count > TEXT_PREVIEW_CHARS {
        let preview: String = text.chars().take(TEXT_PREVIEW_CHARS).collect();
        format!("{}... ({} chars)", escape_control(&preview), char_count)
    } else {
        escape_control(text).into_owned()
    }
}

impl InfoGroup {
    fn write_header(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        write!(f, "{:indent$}", "")?;
        if let Some(offset) = self.offset {
            write!(f, "Offset 0x{:08X}  ", offset)?;
        }
        write!(f, "{}", escape_control(&self.title))?;
        if let Some(length) = self.length {
            write!(f, " ({} bytes)", length)?;
        }
        writeln!(f)
    }

    fn write_children(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        let mut after_field = false;
        for child in &self.children {
            match child {
                InfoNode::Field { label, value } => {
                    let value = match value {
                        InfoValue::Plain(value) => escape_control(value).into_owned(),
                        InfoValue::Text(value) => text_preview(value),
                    };
                    writeln!(f, "{:indent$}{}: {}", "", escape_control(label), value)?;
                    after_field = true;
                }
                InfoNode::Group(group) => {
                    if indent == 0 && after_field {
                        writeln!(f)?;
                    }
                    after_field = false;
                    group.write_header(f, indent)?;
                    group.write_children(f, indent + 2)?;
                    if indent == 0 {
                        writeln!(f)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl fmt::Display for InfoGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let title = escape_control(&self.title);
        writeln!(f, "{}", title)?;
        writeln!(f, "{}", "=".repeat(title.chars().count().max(21)))?;
        writeln!(f)?;
        self.write_children(f, 0)
    }
}
