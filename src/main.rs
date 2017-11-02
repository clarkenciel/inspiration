extern crate futures;
extern crate hyper;
extern crate regex;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate tokio_core;

use std::env;
use std::sync::Arc;

use futures::{Future, Stream};
use futures::future;
use hyper::{Client, Uri};
use hyper::client::HttpConnector;
use hyper::header::ContentLength;
use hyper::StatusCode;
use hyper::server::{Http, Request, Response, Service};
use regex::Regex;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;

#[derive(Debug)]
struct Muse {
    http_client: Client<HttpConnector>,
    uri: Uri,
}

impl Muse {
    fn new(client: Client<HttpConnector>, uri: Uri) -> Self {
        Muse {
            uri: uri,
            http_client: client,
        }
    }

    fn inspire(&self) -> Box<Future<Item = String, Error = hyper::Error>> {
        Box::new(self.http_client.get(self.uri.clone()).and_then(
            |response| {
                unchunk(response.body())
            },
        ))
    }
}

struct Inspiration {
    responder: Arc<Responder>,
    valid_tokens: Arc<Vec<String>>,
}

impl Inspiration {
    fn new(responder: Responder, tokens: Vec<String>) -> Self {
        Inspiration {
            responder: Arc::new(responder),
            valid_tokens: Arc::new(tokens),
        }
    }

    fn validate_request(
        &self,
        body: hyper::Body,
    ) -> Box<Future<Item = bool, Error = hyper::Error>> {
        let valid_tokens = self.valid_tokens.clone();
        Box::new(unchunk(body).and_then(move |body_str| {
            let re = Regex::new(r"token=(\w+)&").expect("Coud not compile token regexp");
            let result = re.captures(&*body_str).and_then(|cap| cap.get(1)).map(|m| {
                valid_tokens.iter().any(
                    |valid| valid.as_str() == m.as_str(),
                )
            });

            future::ok(result.unwrap_or(false))
        }))
    }
}

struct Responder {
    muse: Muse,
}

impl Responder {
    fn new(muse: Muse) -> Self {
        Responder { muse: muse }
    }

    fn respond(&self) -> Box<Future<Item = Response, Error = hyper::Error>> {
        Box::new(
            self.muse
                .inspire()
                .map(|message| {
                    Response::new()
                        .with_header(ContentLength(message.len() as u64))
                        .with_status(StatusCode::Ok)
                        .with_body(serde_json::to_string(&Message::new(message)).unwrap_or(
                            String::new(),
                        ))
                }),
        )
    }
}

#[derive(Serialize, Deserialize)]
struct Message {
    response_type: &'static str,
    text: String,
}

impl Message {
    fn new(message: String) -> Self {
        Message {
            response_type: "ephemeral",
            text: message,
        }
    }
}

fn unchunk(body: hyper::Body) -> Box<Future<Item = String, Error = hyper::Error>> {
    Box::new(
        body.fold(Vec::new(), |mut acc, chunk| {
            acc.extend_from_slice(&*chunk);
            futures::future::ok::<_, hyper::Error>(acc)
        }).map(|chunks| String::from_utf8(chunks).unwrap_or(String::new())),
    )
}

impl Service for Inspiration {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let responder = self.responder.clone();
        let method = req.method().clone();

        Box::new(self.validate_request(req.body()).and_then(
            move |check| if check {
                match method {
                    hyper::Method::Post => responder.respond(),
                    _ => Box::new(future::ok(
                        Response::new().with_status(StatusCode::NotFound),
                    )),
                }
            } else {
                Box::new(future::finished(
                    Response::new().with_status(StatusCode::Forbidden),
                ))
            },
        ))
    }
}

fn main() {
    let port_str = env::var("PORT").expect("Please set the PORT environment variable");
    let listen_addr = format!("0.0.0.0:{}", port_str).parse().expect(
        "Could not parse a listening address",
    );
    let mut core = Core::new().expect("Cound not create new async engine");
    let handle = core.handle();
    let tokens_string = env::var("VALID_TOKENS").unwrap_or(String::new());

    let listener = TcpListener::bind(&listen_addr, &handle).expect("Could not build tcp listener");

    let http = Http::new();
    let server = listener.incoming().for_each(|(socket, addr)| {
        let uri = "http://inspirobot.me/api?generate=true".parse().expect(
            "Could not parse inspirobot url",
        );
        let valid_tokens = tokens_string.split(':').map(String::from).collect();
        let responder = Responder::new(Muse::new(Client::new(&handle), uri));
        let inspiration = Inspiration::new(responder, valid_tokens);

        http.bind_connection(&handle, socket, addr, inspiration);
        Ok(())
    });

    core.run(server).expect(
        "something went terribly wrong with the server",
    );
}
