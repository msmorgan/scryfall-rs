//! Crate-internal macros.

/// Declares a serde enum that gains an `Unknown` fallback under the
/// `unknown_variants` / `unknown_variants_slim` features and is
/// `#[non_exhaustive]` otherwise.
///
/// Under `unknown_variants` the fallback is `Unknown(UnknownStr)`, an interned
/// (and therefore `Copy`) string (see `UnknownStr`). Under `unknown_variants_slim`
/// it is a unit `Unknown` via `#[serde(other)]`. With neither feature the enum
/// has no `Unknown` arm and stays `#[non_exhaustive]`.
///
/// Callers pass any extra container attributes (`#[serde(rename_all = …)]`,
/// `#[derive(Ord, PartialOrd)]`, `#[allow(missing_docs)]`, doc comments) and
/// per-variant attributes through verbatim. All target enums are unit-variant.
///
/// See `docs/superpowers/specs/2026-06-02-unknown-enum-fallback-design.md`.
macro_rules! unknown_fallback {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$var_meta:meta])*
                $var:ident
            ),* $(,)?
        }
    ) => {
        #[derive(::serde::Serialize, ::serde::Deserialize, Clone, Copy, Eq, PartialEq, Hash, Debug)]
        #[cfg_attr(
            all(not(feature = "unknown_variants"), not(feature = "unknown_variants_slim")),
            non_exhaustive
        )]
        #[cfg_attr(test, serde(deny_unknown_fields))]
        $(#[$enum_meta])*
        $vis enum $name {
            $(
                $(#[$var_meta])*
                $var,
            )*

            #[cfg_attr(
                docsrs,
                doc(cfg(any(feature = "unknown_variants", feature = "unknown_variants_slim")))
            )]
            #[cfg(feature = "unknown_variants")]
            #[serde(untagged)]
            /// An unknown value not yet modeled by this version of the crate.
            Unknown(crate::unknown::UnknownStr),

            #[cfg_attr(
                docsrs,
                doc(cfg(any(feature = "unknown_variants", feature = "unknown_variants_slim")))
            )]
            #[cfg(all(not(feature = "unknown_variants"), feature = "unknown_variants_slim"))]
            #[serde(other)]
            /// An unknown value not yet modeled by this version of the crate.
            Unknown,
        }
    };
}

pub(crate) use unknown_fallback;
