use std::fmt::Debug;

pub trait Marker: Debug + Clone + PartialEq {
    fn from_u16(value: u16) -> Option<Self>
    where
        Self: Sized;
    fn to_u16(&self) -> u16;
}
