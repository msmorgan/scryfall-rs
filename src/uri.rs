//! Module for handling unresolved URLs returned by the scryfall api
//!
//! Some fields of the scryfall api have URLs referring to queries that can be
//! run to obtain more information. This module abstracts the work of fetching
//! that data.
use std::marker::PhantomData;
use std::time::Duration;

use cfg_if::cfg_if;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::Error;
use crate::list::{List, ListIter};

cfg_if! {
    if #[cfg(feature = "rate_limiter")] {
        use std::time::Duration;

        use crate::util::ratelimit::RateLimitedAgent;

        thread_local!(static CLIENT: RateLimitedAgent = RateLimitedAgent::new(Duration::from_millis(100)));
    } else {
        use ureq::Agent;

        thread_local!(static CLIENT: Agent = Agent::new());
    }
}

fn make_request(url: &Url) -> Result<ureq::Response, ureq::Error> {
    CLIENT.with(|client| client.request_url("GET", url).call())
}

/// An unresolved URI returned by the Scryfall API, or generated by this crate.
///
/// The `fetch` method handles requesting the resource from the API endpoint,
/// and deserializing it into a `T` object. If the type parameter is
/// [`List`][crate::list::List]`<_>`, then additional method `fetch_iter` is
/// available, producing an iterator over all the items in a paged collection.
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Hash, Debug)]
#[serde(transparent)]
pub struct Uri<T> {
    url: Url,
    _marker: PhantomData<fn() -> T>,
}

// TODO(msmorgan): This should be `TryFrom` since it's fallible.
impl<T: DeserializeOwned> From<&str> for Uri<T> {
    fn from(url: &str) -> Self {
        Uri::from(Url::parse(url).unwrap())
    }
}

impl<T: DeserializeOwned> From<Url> for Uri<T> {
    fn from(url: Url) -> Self {
        Uri {
            url,
            _marker: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> Uri<T> {
    /// Fetches a resource from the Scryfall API and deserializes it into a type
    /// `T`.
    ///
    /// # Example
    /// ```rust
    /// # use scryfall::card::Card;
    /// # use scryfall::uri::Uri;
    /// let uri = Uri::<Card>::from("https://api.scryfall.com/cards/named?exact=Lightning+Bolt");
    /// let bolt = uri.fetch().unwrap();
    /// assert_eq!(bolt.mana_cost, Some("{R}".to_string()));
    /// ```
    pub fn fetch(&self) -> crate::Result<T> {
        match make_request(&self.url) {
            Ok(response) => match response.status() {
                200..=299 => Ok(serde_json::from_reader(response.into_reader())?),
                status => Err(Error::HttpError(status, response.status_text().to_string())),
            },
            Err(ureq::Error::Status(400..=499, response)) => Err(Error::ScryfallError(
                serde_json::from_reader(response.into_reader())?,
            )),
            Err(error) => Err(Error::UreqError(error, self.url.to_string())),
        }
    }
}

impl<T: DeserializeOwned> Uri<List<T>> {
    /// Lazily iterate over items from all pages of a list. Following pages are
    /// only requested once the previous page has been exhausted. It is assumed
    /// that the following pages will not fail to fetch because the URLs are
    /// generated by the Scryfall API. If a following page fails, an error
    /// message is logged to stderr and the iterator will only return `None`.
    ///
    /// # Example
    /// ```rust
    /// # use scryfall::Card;
    /// # use scryfall::list::List;
    /// # use scryfall::uri::Uri;
    /// let uri = Uri::<List<Card>>::from("https://api.scryfall.com/cards/search?q=zurgo");
    /// assert!(
    ///     uri.fetch_iter()
    ///         .unwrap()
    ///         .find(|c| c.name.contains("Bellstriker"))
    ///         .is_some()
    /// );
    /// ```
    pub fn fetch_iter(&self) -> crate::Result<ListIter<T>> {
        Ok(self.fetch()?.into_iter())
    }

    /// Eagerly fetch items from all pages of a list. If any of the pages fail
    /// to load, returns an error.
    ///
    /// # Example
    /// ```rust
    /// # use scryfall::Card;
    /// # use scryfall::list::List;
    /// # use scryfall::uri::Uri;
    /// let uri =
    ///     Uri::<List<Card>>::from("https://api.scryfall.com/cards/search?q=e:ddu&unique=prints");
    /// assert_eq!(uri.fetch_all().unwrap().len(), 76);
    /// ```
    pub fn fetch_all(&self) -> crate::Result<Vec<T>> {
        let mut items = vec![];
        let mut next_page = Some(self.fetch()?);
        while let Some(page) = next_page {
            items.extend(page.data.into_iter());
            next_page = match page.next_page {
                Some(uri) => Some(uri.fetch()?),
                None => None,
            };
        }
        Ok(items)
    }
}
