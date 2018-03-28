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
    let mut core = Core::new().unwrap();
    let rubber = Rubber::new(
        core.remote(),
        config.url.parse()?,
        config.number,
        config.concurrency,
    );

    println!("running core");
    let core_results = core.run(rubber)?;
    let results = Arc::try_unwrap(core_results).unwrap().into_inner();
    let count = results.into_iter().fold(0, |acc, x| {
        println!("{:?}", x);
        acc + 1
    });

    println!("finished requests: {:?}", count);
    Ok(())
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
impl Rubber {
    fn new(remote: Remote, uri: hyper::Uri, number: u32, concurrency: u16) -> Rubber {
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
            println!("finished: {:?}", self.finished);
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
        println!("{}", fmt_duration(self.duration().unwrap()));
        Ok(Async::Ready(ResponseWrapper {
            duration: self.duration().unwrap(),
            response,
        }))
    }
}

// Helpers

fn fmt_duration(d: Duration) -> String {
    format!(
        "request duration: {:?}s {}ms",
        d.as_secs() * 1_000,
        d.subsec_nanos() as u64 / 1_000_000
    )
}
