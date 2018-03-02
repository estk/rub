#[macro_use]
extern crate clap;
extern crate rub;

use std::process;
use rub::Config;
use std::error::Error;
use clap::{App, Arg};

fn main() {
    let config = parse_args().unwrap_or_else(|err| {
        eprintln!("Problem parsing argumennts: {}", err);
        process::exit(1)
    });
    eprintln!("Making a request to {}", config.url);

    if let Err(e) = rub::run(config) {
        eprintln!("Application error {:?}", e);
        process::exit(1);
    }
    eprintln!("done");
}

const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const AUTHORS: &'static str = env!("CARGO_PKG_AUTHORS");
const DESCRIPTION: &'static str = env!("CARGO_PKG_DESCRIPTION");
// TODO have static names for args

fn parse_args() -> Result<Config, Box<Error>> {
    let matches = App::new("RUstBench")
        .version(VERSION)
        .author(AUTHORS)
        .about(DESCRIPTION)
        .arg(
            Arg::with_name("url")
                .required(true)
                .index(1)
                .help("The url to make a request to"),
        )
        .arg(
            Arg::with_name("number")
                .short("n")
                .long("number")
                .takes_value(true)
                .help("The number of requests to make"),
        )
        .arg(
            Arg::with_name("concurrency")
                .short("c")
                .long("concurrency")
                .takes_value(true)
                .help("The number of concurrent requests to make"),
        )
        .arg(
            Arg::with_name("method")
                .short("m")
                .long("method")
                .takes_value(true)
                .help("The HTTP method"),
        )
        .get_matches();

    let url = match matches.value_of("url") {
        Some(arg) => arg.to_string(),
        None => return Err("No url provided".into()),
    };
    let number = value_t!(matches, "number", u32).unwrap_or(100);
    let concurrency = value_t!(matches, "concurrency", u32).unwrap_or(10);
    Ok(Config {
        url,
        number,
        concurrency,
    })
}
