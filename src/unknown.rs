//! Interning support for the `Unknown(UnknownStr)` enum fallback.
//!
//! Compiled only under the `unknown_variants` feature. When Scryfall sends a
//! value this crate version has no typed variant for, it is interned here into
//! a `Copy` [`UnknownStr`] so the enclosing enum can stay `Copy`. Distinct
//! values are leaked exactly once and deduped; the unknown vocabulary is tiny
//! in practice. See
//! `docs/superpowers/specs/2026-06-02-unknown-enum-fallback-design.md`.

use std::borrow::{Borrow, Cow};
use std::collections::HashSet;
use std::fmt;
use std::ops::Deref;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

static POOL: Lazy<Mutex<HashSet<&'static str>>> = Lazy::new(|| Mutex::new(HashSet::new()));

/// Interns `s` to a `&'static str`. Equal strings return the same pointer;
/// each distinct string is leaked exactly once.
fn intern(s: &str) -> &'static str {
    let mut pool = POOL.lock().unwrap();
    if let Some(&hit) = pool.get(s) {
        return hit;
    }
    let leaked: &'static str = Box::leak(Box::from(s));
    pool.insert(leaked);
    leaked
}

/// A `Copy`, interned string standing in for an enum value this version of the
/// crate does not model.
///
/// Behaves like a `Box<str>`: it derefs to `str`, compares and hashes by
/// content (so ordering is deterministic, not pointer-based), and renders as
/// the underlying string under both `Display` and `Debug`. Because it is an
/// owned `Copy` type rather than a borrowed `&'static str`, it embeds cleanly
/// as a field of outer `#[derive(Deserialize)]` types.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct UnknownStr(&'static str);

impl Deref for UnknownStr {
    type Target = str;

    fn deref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for UnknownStr {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl Borrow<str> for UnknownStr {
    fn borrow(&self) -> &str {
        self.0
    }
}

impl fmt::Display for UnknownStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl fmt::Debug for UnknownStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.0, f)
    }
}

impl From<&str> for UnknownStr {
    fn from(s: &str) -> Self {
        UnknownStr(intern(s))
    }
}

impl From<String> for UnknownStr {
    fn from(s: String) -> Self {
        UnknownStr(intern(&s))
    }
}

impl Serialize for UnknownStr {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for UnknownStr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = Cow::<str>::deserialize(d)?;
        Ok(UnknownStr(intern(&s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_dedups_to_same_pointer() {
        assert!(std::ptr::eq(intern("alpha"), intern("alpha")));
        assert!(!std::ptr::eq(intern("alpha"), intern("beta")));
    }

    #[test]
    fn codec_round_trips_through_set_type() {
        use crate::set::SetType;
        let v: SetType = serde_json::from_str(r#""totally-new-set-type""#).unwrap();
        assert_eq!(v, SetType::Unknown("totally-new-set-type".into()));
        assert_eq!(serde_json::to_string(&v).unwrap(), r#""totally-new-set-type""#);
    }
}
