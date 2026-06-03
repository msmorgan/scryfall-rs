use crate::macros::unknown_fallback;

unknown_fallback! {
/// The finish the card can come in.
#[derive(Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum Finishes {
    /// Nonfoil.
    Nonfoil,
    /// Foil.
    Foil,
    /// Etched foil.
    Etched,
}
}
