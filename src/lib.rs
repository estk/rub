#![feature(duration_extras)]

#[macro_use]
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;

use futures::Future;
use futures::prelude::*;
use futures::sync::mpsc;
use hyper::client::FutureResponse;
use hyper::{Body, Client, Response};
use hyper_tls::HttpsConnector;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_core::reactor::{Core, Remote};

#[derive(Debug)]
pub struct Config {
    pub url: String,
    pub number: u32,
    pub concurrency: u16,
}

pub fn run(config: Config) -> Result<(), Box<Error>> {
    let core = Core::new().unwrap();
    let rubber = Rubber::new(
        core.remote(),
        config.url.parse()?,
        config.number,
        config.concurrency,
    );

    let stats = rubber.run(core)?;
    println!("{}", stats.display());
    Ok(())
}

struct Stats {
    slowest: Duration,
    fastest: Duration,
    average: Duration,
    total: Duration,
    count: u32,
    errors: u32,
    status_codes: StatusCodeMap,
}
impl Stats {
    fn display(&self) -> String {
        format!(
            r#"
Summary:
    slowest: {}
    fastest: {}
    average: {}
    total:   {}
    requsts: {}
    errors:  {}
Status Codes:
{}
        "#,
            display(self.slowest),
            display(self.fastest),
            display(self.average),
            display(self.total),
            self.count,
            self.errors,
            display_map(&self.status_codes),
        )
    }
}

fn display_map(m: &StatusCodeMap) -> String {
    let mut s = String::new();
    for (k, v) in m {
        s += &format!("    {}: {}\n", k, v);
    }
    s
}

type Results = Vec<Result<ResponseWrapper, hyper::Error>>;
struct Rubber {
    uri: hyper::Uri,
    results_tx: mpsc::Sender<Result<ResponseWrapper, hyper::Error>>,
    results_rx: mpsc::Receiver<Result<ResponseWrapper, hyper::Error>>,
    remote: Remote,
    running: u16,
    number: u32,
    finished: u32,
    results: Arc<RefCell<Results>>,
    concurrency: u16,
}

type StatusCodeMap = HashMap<hyper::StatusCode, u32>;
impl Rubber {
    pub fn new(remote: Remote, uri: hyper::Uri, number: u32, concurrency: u16) -> Rubber {
        let (results_tx, results_rx) = mpsc::channel(1);
        let results = Arc::new(RefCell::new(vec![]));

        Rubber {
            uri,
            number,
            concurrency,
            remote,
            results_tx,
            results_rx,
            results,
            running: 0,
            finished: 0,
        }
    }
    pub fn run(self, mut core: Core) -> Result<Stats, Box<Error>> {
        let core_results = core.run(self)?;
        let results = Arc::try_unwrap(core_results).unwrap().into_inner();

        let mut count = 0;
        let mut errors = 0;
        let mut total = Duration::new(0, 0);
        let mut slowest = Duration::new(0, 0);
        let mut fastest = Duration::new(u64::max_value(), 0);
        let mut status_codes = StatusCodeMap::new();

        for r in results.into_iter() {
            match r {
                Err(_) => errors += 1,
                Ok(res) => {
                    let d = res.duration;
                    count += 1;
                    total += d;
                    if d > slowest {
                        slowest = d;
                    }
                    if d < fastest {
                        fastest = d;
                    }

                    let status = res.response.status();
                    let entry = status_codes.entry(status);
                    entry.and_modify(|e| *e += 1).or_insert(1);
                }
            }
        }
        Ok(Stats {
            count,
            errors,
            total,
            slowest,
            fastest,
            average: total / count,
            status_codes,
        })
    }
    fn spawn_requests(&mut self) {
        let done_spawning = self.running as u32 + self.finished >= self.number;
        if !done_spawning {
            for _ in 0..(self.concurrency - self.running) {
                self.spawn_request();
            }
        }
    }
    fn spawn_request(&mut self) {
        let results_tx = self.results_tx.clone();
        let uri = self.uri.clone();

        self.remote.spawn(move |handle| {
            let conn = HttpsConnector::new(4, &handle).unwrap();
            let client = Client::configure().connector(conn).build(&handle);

            FutureResponseWrapper::new(client.get(uri))
                .then(move |res| results_tx.send(res).then(|_| Ok(())))
        });
        self.running += 1;
    }
}

impl Future for Rubber {
    type Item = Arc<RefCell<Results>>;
    type Error = Box<Error>;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            if self.finished >= self.number {
                return Ok(Async::Ready(self.results.clone()));
            }
            self.spawn_requests();

            let res = match self.results_rx.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(Some(v))) => v,
                Ok(Async::Ready(None)) => panic!("faack"),
                Err(_) => continue,
            };
            self.running -= 1;
            self.finished += 1;

            self.results.borrow_mut().push(res);
        }
    }
}

#[derive(Debug)]
struct ResponseWrapper {
    duration: Duration,
    response: Response<Body>,
}

// FutureResponseWrapper wraps a response with timing information
struct FutureResponseWrapper {
    start: Option<Instant>,
    finish: Option<Instant>,
    inner: FutureResponse,
}
impl FutureResponseWrapper {
    fn new(inner: FutureResponse) -> FutureResponseWrapper {
        FutureResponseWrapper {
            start: None,
            finish: None,
            inner,
        }
    }
    fn duration(&mut self) -> Result<std::time::Duration, &str> {
        match (self.start, self.finish) {
            (Some(start), Some(finish)) => Ok(finish.duration_since(start)),
            _ => Err("Could not get the duration of an incomplete request"),
        }
    }
}
impl Future for FutureResponseWrapper {
    type Item = ResponseWrapper;
    type Error = hyper::Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self.start.is_none() {
            self.start = Some(Instant::now());
        }
        let response = try_ready!(self.inner.poll());
        self.finish = Some(Instant::now());
        // println!("{}", fmt_duration(self.duration().unwrap()));
        Ok(Async::Ready(ResponseWrapper {
            duration: self.duration().unwrap(),
            response,
        }))
    }
}

// Helpers
fn display(d: Duration) -> String {
    format!(
        "{}ms",
        (d.as_secs() * 1_000 + d.subsec_nanos() as u64 / 1_000_000)
    )
}
