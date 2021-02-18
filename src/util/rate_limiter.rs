use std::cell::{RefCell, RefMut};
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use url::Url;

pub struct Limiter {
    capacity: usize,
    available: isize,
    interval: Duration,
    tx: mpsc::SyncSender<(usize, mpsc::Sender<Option<Duration>>)>,
    rx: mpsc::Receiver<(usize, mpsc::Sender<Option<Duration>>)>,
}

impl Limiter {
    pub fn new(interval: Duration) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);

        Limiter {
            capacity: 1,
            available: 1,
            interval,
            tx,
            rx,
        }
    }

    pub fn wait(&mut self) {
        self.wait_for(1)
    }

    pub fn wait_for(&mut self, count: usize) {
        if let Some(dur) = self.request(count) {
            thread::sleep(dur)
        }
    }

    pub fn run(mut self) -> ! {
        loop {
            if let Ok((count, tx)) = self.rx.try_recv() {
                let _ = tx.send(self.request(count));
            }

            thread::sleep(self.interval);
            self.available = (self.available + 1).min(self.capacity as isize);
        }
    }

    pub fn get_handle(&self) -> Handle {
        Handle {
            tx: self.tx.clone(),
        }
    }

    fn request(&mut self, count: usize) -> Option<Duration> {
        self.available -= count as isize;
        if self.available < 0 {
            // If these tokens are not available yet, they will become available
            // as soon as enough are generated.
            Some(self.interval * (-avaialable) as u32)
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct Handle {
    tx: mpsc::SyncSender<(usize, mpsc::Sender<Option<Duration>>)>,
}

impl Handle {
    pub fn wait(&self) {
        self.wait_for(1);
    }

    pub fn wait_for(&self, count: usize) {
        if let Some(dur) = self.request(count) {
            thread::sleep(dur)
        }
    }

    fn request(&self, count: usize) -> Option<Duration> {
        let (tx, rx) = mpsc::channel();
        if self.tx.send((count, tx)).is_ok() {
            rx.recv().expect("Didn't receive a response.")
        } else {
            panic!("Failed to send to limiter channel.");
        }
    }
}

impl RateLimitedAgent {
    pub fn new(interval: Duration) -> Self {
        RateLimitedAgent {
            inner: ureq::Agent::new(),
            limiter: RefCell::new(Limiter::new(interval)),
        }
    }

    pub fn request_url<'a>(&'a self, method: &str, url: &Url) -> RateLimitedRequest<'a> {
        RateLimitedRequest {
            inner: self.inner.request_url(method, url),
            limiter: self.limiter.borrow_mut(),
        }
    }
}

pub struct RateLimitedRequest<'a> {
    inner: ureq::Request,
    limiter: RefMut<'a, Limiter>,
}

impl<'a> RateLimitedRequest<'a> {
    pub fn call(mut self) -> Result<ureq::Response, ureq::Error> {
        self.limiter.wait();
        self.inner.call()
    }
}
