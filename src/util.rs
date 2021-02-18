//! Module containing utility functions and structs.

use once_cell::sync::Lazy;
use url::Url;
pub use uuid::Uuid;

#[cfg(feature = "throttling")]
pub mod http_throttling;

/// The [scryfall](https://scryfall.com/docs/api) endpoint.
pub static ROOT_URL: Lazy<Url> = Lazy::new(|| Url::parse("https://api.scryfall.com/").unwrap());
/// The [cards](https://scryfall.com/docs/api/cards) endpoint.
pub static CARDS_URL: Lazy<Url> = Lazy::new(|| ROOT_URL.join("cards/").unwrap());
/// The [sets](https://scryfall.com/docs/api/sets) endpoint.
pub static SETS_URL: Lazy<Url> = Lazy::new(|| ROOT_URL.join("sets/").unwrap());
/// The [bulk-data](https://scryfall.com/docs/api/bulk-data) endpoint.
pub static BULK_DATA_URL: Lazy<Url> = Lazy::new(|| ROOT_URL.join("bulk-data/").unwrap());
/// The [catalog](https://scryfall.com/docs/api/catalogs) endpoint.
pub static CATALOG_URL: Lazy<Url> = Lazy::new(|| ROOT_URL.join("catalog/").unwrap());

/// The [rulings](https://scryfall.com/docs/api/rulings) path segment, which goes on the end of a
/// card URL.
pub const API_RULING: &str = "rulings/";
