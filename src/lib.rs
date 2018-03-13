#[macro_use]
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;

use std::sync::Arc;
use std::cell::RefCell;
use std::error::Error;
use tokio_core::reactor::Core;
use tokio_core::reactor::Remote;
use futures::prelude::*;
use futures::Future;
use futures::future::Executor;
use futures::future::ok;
use futures::sync::mpsc;
use hyper::Client;
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;

#[derive(Debug)]
pub struct Config {
    pub url: String,
    pub number: u32,
    pub concurrency: u16,
}

impl Config {}

pub fn run(config: Config) -> Result<(), Box<Error>> {
    let mut core = Core::new()?;

    let uri = config.url.parse()?;
    let concurrency = config.concurrency;
    let number = config.number;

    println!("running core");

    let (tx, rx) = mpsc::channel(1);

    let rubber = Rubber {
        uri,
        results_tx: tx,
        results_rx: rx,
        remote: core.remote(),
        running: 0,
        number: number,
        finished: 0,
        results: Arc::new(RefCell::new(vec![])),
        concurrency: concurrency,
    };

    let results = core.run(rubber)?;
    let unwrapped = Arc::try_unwrap(results).unwrap().into_inner();
    let mut count = 0;

    for _ in unwrapped.into_iter() {
        count += 1;
    }

    // results.to_iter().for_each(|_| counter += 1;);
    // results

    println!("count: {:?}", count);

    println!("finished running");
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

impl Future for Rubber {
    type Item = Arc<RefCell<Results>>;
    type Error = Box<Error>;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            if self.finished >= self.number {
                return Ok(Async::Ready(self.results.clone()));
            }

            let done_spawning = self.running as u32 + self.finished >= self.number;
            if !done_spawning {
                for _ in 0..(self.concurrency - self.running) {
                    let remote = &mut self.remote;
                    let results_tx = self.results_tx.clone();
                    let uri = self.uri.clone();

                    remote.spawn(move |handle| {
                        let conn = HttpsConnector::new(4, &handle).unwrap();
                        let client = Client::configure().connector(conn).build(&handle);

                        client
                            .get(uri)
                            .then(|res| results_tx.send(res).then(|_| Ok(())))
                    });
                    self.running += 1;
                }
            }
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
