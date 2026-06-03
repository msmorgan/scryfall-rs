use crate::macros::unknown_fallback;

unknown_fallback! {
/// The security stamp on this card, if any.
#[derive(Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum SecurityStamp {
    Oval,
    Triangle,
    Acorn,
    Circle,
    Arena,
    Heart,
}
}
