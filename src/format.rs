//! The available magic the gathering formats.
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[allow(missing_docs)]
#[non_exhaustive]
pub enum Format {
    Standard,
    Modern,
    Legacy,
    Vintage,
    Commander,
    Future,
    Pauper,
    Frontier,
    Penny,
    Duel,
    Oldschool,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Format::*;
        write!(
            f,
            "{}",
            match self {
                Standard => "standard",
                Modern => "modern",
                Legacy => "legacy",
                Vintage => "vintage",
                Commander => "commander",
                Future => "future",
                Pauper => "pauper",
                Frontier => "frontier",
                Penny => "penny",
                Duel => "duel",
                Oldschool => "oldschool",
            }
        )
    }
}
