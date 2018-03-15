#[macro_use]
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;

use futures::prelude::*;
use std::sync::Arc;
use std::cell::RefCell;
use std::error::Error;
use std::time::{Duration, Instant};
use tokio_core::reactor::Core;
use tokio_core::reactor::Remote;
use futures::Future;
use futures::sync::mpsc;
use hyper::{Client, Response, Body};
use hyper::client::FutureResponse;
use hyper_tls::HttpsConnector;

#[derive(Debug)]
pub struct Config {
    pub url: String,
    pub number: u32,
    pub concurrency: u16,
}

pub fn run(config: Config) -> Result<(), Box<Error>> {
    let mut core = Core::new()?;
    let (results_tx, results_rx) = mpsc::channel(1);

    let rubber = Rubber {
        uri: config.url.parse()?,
        number: config.number,
        concurrency: config.concurrency,
        running: 0,
        finished: 0,
        remote: core.remote(),
        results_tx,
        results_rx,
        results: Arc::new(RefCell::new(vec![])),
    };

    println!("running core");

    let results = core.run(rubber)?;
    let unwrapped = Arc::try_unwrap(results).unwrap().into_inner();

    let count = unwrapped.into_iter().fold(0, |acc, x| {
        println!("{:?}", x);
        acc + 1
    });

    println!("finished requests: {:?}", count);
    Ok(())
}

type Results = Vec<Result<hyper::Response<hyper::Body>, hyper::Error>>;
struct Rubber {
    uri: hyper::Uri,
    results_tx: mpsc::Sender<Result<hyper::Response<hyper::Body>, hyper::Error>>,
    results_rx: mpsc::Receiver<Result<hyper::Response<hyper::Body>, hyper::Error>>,
    remote: Remote,
    running: u16,
    number: u32,
    finished: u32,
    results: Arc<RefCell<Results>>,
    concurrency: u16,
}
impl Rubber {
    fn spawn_requests(&mut self) {
        let done_spawning = self.running as u32 + self.finished >= self.number;
        if !done_spawning {
            for _ in 0..(self.concurrency - self.running) {
                self.spawn_request();
            }
        }
    }
    fn spawn_request(&mut self) {
        let remote = &mut self.remote;
        let results_tx = self.results_tx.clone();
        let uri = self.uri.clone();

        remote.spawn(move |handle| {
            let conn = HttpsConnector::new(4, &handle).unwrap();
            let client = Client::configure().connector(conn).build(&handle);

            RWrapper::new( client.get(uri))
            .then(move |res| {
                results_tx.send(res);
                Ok(())
            })
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

struct RWrapper {
    start: Option<Instant>,
    inner: FutureResponse,
}
impl RWrapper {
    fn new(inner: FutureResponse) -> RWrapper {
        RWrapper{start: None, inner}
    }
}
impl Future for RWrapper {
    type Item = Response<Body>;
    type Error =  hyper::Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self.start.is_none() {
            self.start = Some(Instant::now());
        }
        let v = try_ready!(self.inner.poll());
        println!("{}", fmtDuration(self.start.unwrap().elapsed()));
        Ok(Async::Ready(v))
    }
}

fn fmtDuration(d: Duration) -> String {
    format!("request duration: {:?}s {}ms", d.as_secs() * 1_000, d.subsec_nanos() as u64 / 1_000_000)
}
