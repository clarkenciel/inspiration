extern crate futures;
extern crate hyper;
extern crate tokio_core;

use std::env;

use futures::{Future, Stream};
use hyper::{Client, Uri};
use hyper::client::HttpConnector;
use hyper::header::ContentLength;
use hyper::StatusCode;
use hyper::server::{Http, Request, Response, Service};
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

#[derive(Debug)]
struct Inspiration {
    muse: Muse,
    core: Core,
    valid_tokens: Vec<String>,
}

impl Inspiration {
    fn new(core: Core, uri: Uri, tokens: Vec<String>) -> Self {
        Inspiration { muse: Muse::new(&core, uri), core: core, valid_tokens: tokens }
    }
}

impl Service for Inspiration {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, _req: Request) -> Self::Future {
        Box::new(
            self.muse.inspire().map(|message| {
                Response::new()
                    .with_header(ContentLength(message.len() as u64))
                    .with_status(StatusCode::Ok)
                    .with_body(message)
            })
        )
    }
}

fn main() {
    let addr = format!("0.0.0.0:{}", env::var("PORT").unwrap()).parse().unwrap();
    let server = Http::new().bind(&addr, || {
        let uri = "http://inspirobot.me/api?generate=true".parse().unwrap();
        let tokens_string = env::var("VALID_TOKENS").unwrap();
        let valid_tokens = tokens_string.split(':').map(String::from).collect();
        let core = Core::new().unwrap();
        let inspiration = Inspiration::new(core, uri, valid_tokens);
        Ok(inspiration)
    }).unwrap();

    server.run().unwrap();
}
