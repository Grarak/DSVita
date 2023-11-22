use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

pub fn align_up(n: u32, align: u32) -> u32 {
    (n + align - 1) & !(align - 1)
}

pub struct StrErr {
    str: String,
}

impl StrErr {
    pub fn new(str: String) -> Self {
        StrErr { str }
    }
}

impl From<&str> for StrErr {
    fn from(value: &str) -> Self {
        StrErr::new(value.to_string())
    }
}

impl Debug for StrErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.str, f)
    }
}

impl Display for StrErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.str, f)
    }
}

impl Error for StrErr {}
