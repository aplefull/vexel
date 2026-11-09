use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::summary::ImageSummary;
use super::writer::{InfoGroup, InfoWriter};

pub trait InfoSection {
    fn title(&self) -> Cow<'_, str>;

    fn describe(&self, out: &mut InfoWriter);
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct Section<S> {
    pub offset: u64,
    pub length: Option<u64>,
    pub data: S,
}

impl<S> Section<S> {
    pub fn new(offset: u64, length: Option<u64>, data: S) -> Self {
        Section { offset, length, data }
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct FormatInfo<S> {
    pub summary: Option<ImageSummary>,
    pub sections: Vec<Section<S>>,
}

impl<S> Default for FormatInfo<S> {
    fn default() -> Self {
        FormatInfo { summary: None, sections: Vec::new() }
    }
}

impl<S> FormatInfo<S> {
    pub fn push(&mut self, offset: u64, length: Option<u64>, data: S) {
        self.sections.push(Section::new(offset, length, data));
    }
}

impl<S: InfoSection> FormatInfo<S> {
    pub fn to_tree(&self, title: &str) -> InfoGroup {
        let mut out = InfoWriter::new(title);

        if let Some(summary) = &self.summary {
            out.group("Summary", |out| summary.describe(out));
        }

        out.field("Sections", self.sections.len());
        self.describe_sections(&mut out);

        out.finish()
    }

    pub fn describe_sections(&self, out: &mut InfoWriter) {
        for section in &self.sections {
            out.section(section.data.title(), section.offset, section.length, |out| section.data.describe(out));
        }
    }

    pub fn shift_offsets(&mut self, base: u64) {
        for section in &mut self.sections {
            section.offset += base;
        }
    }
}

pub trait AnyFormatInfo {
    fn summary(&self) -> Option<&ImageSummary>;

    fn to_tree(&self, title: &str) -> InfoGroup;
}

impl<S: InfoSection> AnyFormatInfo for FormatInfo<S> {
    fn summary(&self) -> Option<&ImageSummary> {
        self.summary.as_ref()
    }

    fn to_tree(&self, title: &str) -> InfoGroup {
        FormatInfo::to_tree(self, title)
    }
}
