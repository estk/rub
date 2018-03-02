extern crate rayon;
extern crate reqwest;
use rayon::prelude::*;
use std::error::Error;
use std::thread;
use std::sync::{Arc, Mutex};

pub struct Config {
    pub url: String,
    pub number: u32,
    pub concurrency: u32,
}

impl Config {}

pub fn run(config: Config) -> Result<(), Box<Error>> {
    let mut workers = vec![];
    let requests_left = Arc::new(Mutex::new(config.number));
    let results = Arc::new(Mutex::new(vec![]));
    for _ in 0..config.concurrency {
        let url = config.url.clone();
        let rl = requests_left.clone();
        let rs = results.clone();
        let worker = thread::spawn(move || loop {
            {
                let mut n = rl.lock().unwrap();
                if *n < 1 {
                    return;
                }
                *n -= 1;
            }
            let res = reqwest::get(&url);
            rs.lock().unwrap().push(res);
        });
        workers.push(worker);
    }
    for w in workers {
        w.join().unwrap();
    }
    let lock = Arc::try_unwrap(results).expect("multiple owners of results still");
    let ress = lock.into_inner()
        .expect("couldnt unlock results after complete");
    let successes: u32 = ress.into_iter()
        .map(|x| match x {
            Ok(_) => 1,
            Err(_) => 0,
        })
        .sum();
    println!("successes {}", successes);
    // let successes: u32 = (0..config.concurrency)
    //     .into_par_iter()
    //     .map(|_: u32| {
    //         let rs = config.number / config.concurrency;
    //         let ss: u32 = (0..rs)
    //             .map(|_| match reqwest::get(&config.url) {
    //                 Ok(res) => {
    //                     println!("result:\n{}", res.status());
    //                     1
    //                 }
    //                 Err(e) => {
    //                     println!("error:\n{}", e);
    //                     0
    //                 }
    //             })
    //             .sum();
    //         ss
    //     })
    //     .sum();
    // println!("total successes {}", successes);
    Ok(())
}
