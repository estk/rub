#[macro_use]
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;

use std::error::Error;
use futures::prelude::*;
use futures::Future;
use futures::sync::mpsc;
use hyper::Client;
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use tokio_core::reactor::Core;
use tokio_core::reactor::Remote;

#[derive(Debug)]
pub struct Config {
    pub url: String,
    pub number: u32,
    pub concurrency: u16,
}

// clients: Vec<Client<HttpsConnector<HttpConnector>>>,
// todo maybe try ^^
impl Config {}

pub fn run(config: Config) -> Result<(), Box<Error>> {
    let mut core = Core::new()?;
    let handle = core.handle();

    let uri = config.url.parse()?;
    let concurrency = config.concurrency;
    let number = config.number;

    let client = Client::configure()
        .connector(HttpsConnector::new(4, &handle)?)
        .build(&handle);

    println!("running core");

    let (tx, rx) = mpsc::channel(1);

    for _ in 0..concurrency {
        handle.spawn(|_| client.get(uri).then(|res| tx.send(res)))
    }
    let rubber = Rubber {
        uri,
        resultsTx: tx,
        resultsRx: rx,
        remote: core.remote(),
        client,
        running: 0,
        number: number,
        finished: 0,
        results: vec![],
    };

    let results = core.run(rubber)?;

    println!("finished running");
    Ok(())
}

struct Rubber {
    uri: hyper::Uri,
    resultsTx: mpsc::Sender<Result<hyper::Response<hyper::Body>, hyper::Error>>,
    resultsRx: mpsc::Receiver<Result<hyper::Response<hyper::Body>, hyper::Error>>,
    remote: Remote,
    client: Client<hyper_tls::HttpsConnector<hyper::client::HttpConnector>>,
    running: u16,
    number: u32,
    finished: u32,
    results: Vec<hyper::Response<hyper::Body>>,
}

impl Future for Rubber {
    type Item = Vec<hyper::Response<hyper::Body>>;
    type Error = hyper::Error;
    fn poll(&mut self) -> Result<Async<Vec<hyper::Response<hyper::Body>>>, hyper::Error> {
        loop {
            let res = try_ready!(self.resultsRx.poll()).unwrap()?;
            self.results.push(res);

            let doneSpawning = self.running as u32 + self.finished >= self.number;
            if !doneSpawning {
                self.remote.spawn(|_| {
                    self.client
                        .get(self.uri)
                        .then(|res| self.resultsTx.send(res))
                })
            }
            if self.finished >= self.number {
                return Ok(Async::Ready(self.results));
            }
        }
    }
}
