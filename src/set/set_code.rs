use std::{
    cmp::Ordering,
    convert::{AsRef, TryFrom},
    fmt,
    str,
};

use serde::{
    de::{self, Deserializer, Visitor},
    ser::Serializer,
    Deserialize,
    Serialize,
};
use smallvec::SmallVec;

/// A 3 to 6 letter set code, like 'war' for 'War of the Spark'.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct SetCode(CodeInner);

#[allow(dead_code)]
impl SetCode {
    /// Creates a set code from a str.
    ///
    /// Valid set codes are ascii between 3 and 6 letters long. If any of these
    /// conditions fails, the conversion fails.
    ///
    /// The error value is None if the `str` was not ascii, otherwise it holds
    /// the size of the `str`.
    ///
    /// ```rust
    /// use scryfall::set::SetCode;
    ///
    /// assert_eq!(SetCode::new("war").unwrap().as_ref(), "war")
    /// ```
    pub fn new(code: &str) -> Result<Self, Option<usize>> {
        SetCode::try_from(code)
    }

    /// Returns a reference to the inner set code.
    pub fn get(&self) -> &str {
        // The inner code is always a valid utf8 str since it can
        // only be created from a valid &str.
        str::from_utf8(self.0.get()).unwrap()
    }
}

impl TryFrom<&str> for SetCode {
    type Error = Option<usize>;
    /// See [`new`](#method.new) for documentation on why this might return an
    /// `Err`.
    fn try_from(code: &str) -> Result<Self, Option<usize>> {
        if !code.is_ascii() {
            return Err(None);
        }
        let code = code.as_bytes();
        Ok(SetCode(match code.len() {
            3..=6 => CodeInner(SmallVec::from_slice(code)),
            invalid => return Err(Some(invalid)),
        }))
    }
}

impl AsRef<str> for SetCode {
    fn as_ref(&self) -> &str {
        self.get()
    }
}

#[derive(Default)]
struct SetCodeVisitor {
    size: Option<usize>,
}

impl<'de> Visitor<'de> for SetCodeVisitor {
    type Value = SetCode;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.size {
            Some(size) => write!(f, "set code size between 3 and 6, found {}", size),
            None => write!(f, "set code to be ascii"),
        }
    }

    fn visit_str<E>(mut self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        SetCode::try_from(s).map_err(|size| {
            self.size = size;
            de::Error::invalid_value(de::Unexpected::Str(s), &self)
        })
    }
}

impl<'de> Deserialize<'de> for SetCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(SetCodeVisitor::default())
    }
}

impl Serialize for SetCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(str::from_utf8(self.0.get()).unwrap())
    }
}

impl fmt::Display for SetCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct CodeInner(SmallVec<[u8; 6]>);

impl PartialOrd for CodeInner {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.get().partial_cmp(other.get())
    }
}

impl Ord for CodeInner {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl CodeInner {
    fn get(&self) -> &[u8] {
        self.0.as_slice()
    }
}
