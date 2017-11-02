extern crate futures;
extern crate hyper;
extern crate regex;
extern crate tokio_core;

use std::env;
use std::cell::RefCell;

use futures::{Future, Stream};
use futures::future;
use hyper::{Client, Uri};
use hyper::client::HttpConnector;
use hyper::header::ContentLength;
use hyper::StatusCode;
use hyper::server::{Http, Request, Response, Service};
use regex::Regex;
use tokio_core::reactor::Core;

#[derive(Debug)]
struct Muse {
    http_client: Client<HttpConnector>,
    uri: Uri,
}

impl Muse {
    fn new(core: &Core, uri: Uri) -> Self {
        Muse {
            uri: uri,
            http_client: Client::new(&core.handle()),
        }
    }

    fn inspire(&self) -> Box<Future<Item = String, Error = hyper::Error>> {
        let fut = self.http_client
            .get(self.uri.clone())
            .and_then(|response| {
                response.body().fold(Vec::new(), |mut acc, chunk| {
                    acc.extend_from_slice(&*chunk);
                    futures::future::ok::<_, hyper::Error>(acc)
                })
            })
            .map(|chunks| String::from_utf8(chunks).unwrap_or(String::new()));

        Box::new(fut)
    }
}

struct Inspiration {
    muse: Muse,
    core: RefCell<Core>,
    valid_tokens: Vec<String>,
}

impl Inspiration {
    fn new(core: Core, uri: Uri, tokens: Vec<String>) -> Self {
        Inspiration {
            muse: Muse::new(&core, uri),
            core: RefCell::new(core),
            valid_tokens: tokens,
        }
    }

    fn validate_request(&self, query_string: &str) -> bool {
        println!("{:?}", query_string);
        let re = Regex::new(r"token=(.*)&?").expect("Coud not compile token regexp");
        let result = re.captures(query_string).and_then(|cap| cap.get(1)).map(
            |m| {
                println!("{:?}", self.valid_tokens);
                self.valid_tokens.iter().any(
                    |valid| valid.as_str() == m.as_str(),
                )
            },
        );

        result.unwrap_or(false)
    }
}

impl Service for Inspiration {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        if self.validate_request(req.query().unwrap_or("")) {
            match req.method() {
                &hyper::Method::Post => {
                    let message = self.core.borrow_mut().run(self.muse.inspire()).unwrap();
                    println!("got message: {}", message);
                    Box::new(future::ok(
                        Response::new()
                            .with_header(ContentLength(message.len() as u64))
                            .with_status(StatusCode::Ok)
                            .with_body(message),
                    ))
                },

                _ => Box::new(future::ok(
                    Response::new().with_status(StatusCode::NotFound),
                )),
            }
        } else {
            Box::new(future::finished(
                Response::new().with_status(StatusCode::Forbidden),
            ))
        }
    }
}

fn main() {
    let port_str = env::var("PORT").expect("Please set the PORT environment variable");
    let addr = format!("0.0.0.0:{}", port_str).parse().expect(
        "Could not parse a listening address",
    );

    let server = Http::new()
        .bind(&addr, || {
            let uri = "http://inspirobot.me/api?generate=true".parse().expect(
                "Could not parse inspirobot url",
            );
            let tokens_string = env::var("VALID_TOKENS").unwrap_or(String::new());
            let valid_tokens = tokens_string.split(':').map(String::from).collect();
            let core = Core::new().expect("Coud not create new tokio core");
            let inspiration = Inspiration::new(core, uri, valid_tokens);

            Ok(inspiration)
        })
        .unwrap();

    server.run().unwrap();
}
