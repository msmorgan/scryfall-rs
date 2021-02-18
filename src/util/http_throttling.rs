use std::cell::{RefCell, RefMut};
use std::time::{Duration, Instant};

use url::Url;

#[derive(Debug)]
pub struct ThrottledAgent {
    inner: ureq::Agent,
    throttler: RefCell<Throttler>,
}

#[derive(Debug)]
struct Throttler {
    delay: Duration,
    timestamp: Option<Instant>,
}

impl Throttler {
    pub fn new(delay: Duration) -> Self {
        Throttler {
            delay,
            timestamp: None,
        }
    }

    pub fn block(&mut self) {
        loop {
            if self
                .timestamp
                .map(|t| t.elapsed() >= self.delay)
                .unwrap_or(true)
            {
                self.timestamp = Some(Instant::now());
                break;
            } else {
                std::thread::yield_now();
            }
        }
    }
}

impl ThrottledAgent {
    pub fn new(delay: Duration) -> Self {
        ThrottledAgent {
            inner: ureq::Agent::new(),
            throttler: RefCell::new(Throttler::new(delay)),
        }
    }

    pub fn request_url<'a>(&'a self, method: &str, url: &Url) -> ThrottledRequest<'a> {
        ThrottledRequest {
            inner: self.inner.request_url(method, url),
            throttler: self.throttler.borrow_mut(),
        }
    }
}

pub struct ThrottledRequest<'a> {
    inner: ureq::Request,
    throttler: RefMut<'a, Throttler>,
}

impl<'a> ThrottledRequest<'a> {
    pub fn call(mut self) -> Result<ureq::Response, ureq::Error> {
        self.throttler.block();
        self.inner.call()
    }
}
