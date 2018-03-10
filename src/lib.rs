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

    let url = &config.url;
    let concurrency = config.concurrency;
    let number = config.number;

    let client = Client::configure()
        .connector(HttpsConnector::new(4, &handle)?)
        .build(&handle);

    println!("running core");

    for (0..conc) {
        handle.spawn(|_| {
            client.get(url)
        })
    }

    let results = core.run(new Rubber())?;

    println!("finished running");

    let successes: u32 = results
        .into_iter()
        .map(|x| match x {
            200 => 1,
            _ => 0,
        })
        .sum();
    println!("successes {}", successes);
    Ok(())
}
// #[derive(Debug)]
struct Rubber {
    url: String,
    resultsRx: mpsc::Receiver<Result<hyper::Response<()>, hyper::Error>>,
    running: u16,
    number: u32,
    finished: u32,
}
impl Future for Rubber {
    type Item = ();
    type Error = hyper::Error;
    fn poll(&mut self) -> Result<Async<()>, hyper::Error> {
        let res = tryready!(self.resultsRx.poll());
        if self.running+self.finished < self.number {
            self.remote.spawn(|_| {
                client.get(self.url)
            })
        } 
    }
}


// #[derive(Debug)]
// struct Request {
//     url: String,
//     running: bool,
//     status: u16,
//     shared: Arc<Mutex<Shared>>,
//     client: Client<HttpsConnector<HttpConnector>>,
// }

// impl Future for Request {
//     type Item = ();
//     type Error = hyper::Error;
//     fn poll(&mut self) -> Result<Async<()>, hyper::Error> {
//         {
//             let mut conf = self.shared.lock().unwrap();
//             if !self.running {
//                 if conf.in_flight >= conf.concurrency {
//                     // TODO: not supposed to do this
//                     return Ok(Async::NotReady);
//                 }
//                 conf.in_flight += 1;
//                 self.running = true;
//             }
//             println!("in_flight: {}, conc: {}", conf.in_flight, conf.concurrency);
//         }
//         // TODO: hangs here
//         let res = try_ready!(self.client.get(self.url.parse()?).poll());
//         println!("heer");
//         let mut conf = self.shared.lock().unwrap();
//         conf.in_flight -= 1;
//         println!("Response: {}", res.status());
//         conf.results.push(res.status().as_u16());
//         Ok(Async::Ready(res))
//     }
// }
