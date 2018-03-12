#[macro_use]
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;

use std::rc::Rc;
use std::cell::RefCell;
use std::error::Error;
use tokio_core::reactor::Core;
use tokio_core::reactor::Remote;
use futures::prelude::*;
use futures::Future;
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
    let handle = core.handle();

    let uri = config.url.parse()?;
    let concurrency = config.concurrency;
    let number = config.number;

    let client = Client::configure()
        .connector(HttpsConnector::new(4, &handle)?)
        .build(&handle);

    println!("running core");

    let (tx, rx) = mpsc::channel(1);

    let rubber = Rubber {
        uri,
        resultsTx: tx,
        resultsRx: rx,
        remote: core.remote(),
        client,
        running: 0,
        number: number,
        finished: 0,
        results: Rc::new(RefCell::new(vec![])),
    };

    let results = core.run(rubber)?;

    println!("finished running");
    Ok(())
}

type Results = Rc<RefCell<Vec<Result<hyper::Response<hyper::Body>, hyper::Error>>>>;

struct Rubber {
    uri: hyper::Uri,
    resultsTx: mpsc::Sender<Result<hyper::Response<hyper::Body>, hyper::Error>>,
    resultsRx: mpsc::Receiver<Result<hyper::Response<hyper::Body>, hyper::Error>>,
    remote: Remote,
    client: Client<hyper_tls::HttpsConnector<HttpConnector>>,
    running: u16,
    number: u32,
    finished: u32,
    results: Results,
}

impl Future for Rubber {
    type Item = Results;
    type Error = Box<Error>;
    fn poll(&mut self) -> Result<Async<Results>, Box<Error>> {
        loop {
            let res = match self.resultsRx.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(Some(v))) => v,
                Ok(Async::Ready(None)) => return Ok(Async::Ready(self.results.clone())),
                Err(_) => continue,
            };
            self.results.borrow_mut().push(res);

            let doneSpawning = self.running as u32 + self.finished >= self.number;
            if !doneSpawning {
                self.client.get(self.uri.clone()).then(|res| {
                    let tx = &mut self.resultsTx;
                    tx.send(res)
                });
            }
            if self.finished >= self.number {
                return Ok(Async::Ready(self.results.clone()));
            }
        }
    }
}
