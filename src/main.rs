extern crate futures;
extern crate hyper;
extern crate slack;
extern crate tokio_core;

use std::env;

use futures::{Future, Stream};
use hyper::{Client, Uri};
use hyper::client::HttpConnector;
use slack::{Event, EventHandler, RtmClient};
use slack::Event::Message;
use slack::Message::Standard;
use tokio_core::reactor::Core;

const INSPIRATION_URI: &'static str = "http://inspirobot.me/api?generate=true";
const API_ENV_VAR: &'static str = "SLACK_API_TOKEN";

#[derive(Debug)]
struct Muse {
    http_client: Client<HttpConnector>,
    uri: Uri,
}

#[derive(Debug)]
struct MuseErr(&'static str);

impl Muse {
    fn new(core: &Core, uri: Uri) -> Self {
        Muse {
            uri: uri,
            http_client: Client::new(&core.handle()),
        }
    }

    fn inspire(&self) -> Box<Future<Item = String, Error = MuseErr>> {
        let fut = self.http_client
            .get(self.uri.clone())
            .and_then(|response| {
                response.body().fold(Vec::new(), |mut acc, chunk| {
                    acc.extend_from_slice(&*chunk);
                    futures::future::ok::<_, hyper::Error>(acc)
                })
            })
            .map(String::from_utf8)
            .map(|string| string.unwrap_or(String::new()))
            .map_err(|_| MuseErr("The well's run dry friend!"));

        Box::new(fut)
    }
}

#[derive(Debug)]
struct Inspiration<'a> {
    muse: Muse,
    core: &'a mut Core
}

impl<'a> Inspiration<'a> {
    fn new(core: &'a mut Core, uri: Uri) -> Self {
        Inspiration { muse: Muse::new(core, uri), core: core }
    }

    fn handle_message(&mut self, client: &RtmClient, message: &slack::Message) {
        match message {
            &Standard(ref message) => {
                match message.channel {
                    Some(ref ch) => self.send_inspiration(client, ch),
                    _ => ()
                }
            },
            _ => (),
        };
    }

    fn send_inspiration(&mut self, client: &RtmClient, channel: &str) {
        let fut = self.muse.inspire().map(|response| {
            client.sender().send_message(channel, &response)
        });

        self.core.run(fut).unwrap();
    }
}

#[allow(unused_variables)]
impl<'a> EventHandler for Inspiration<'a> {
    fn on_event(&mut self, client: &RtmClient, event: Event) {
        println!("receieved message: {:?}", event);
        match event {
            Message(boxed_message) => self.handle_message(client, boxed_message.as_ref()),
            _ => (),
        }
    }

    fn on_close(&mut self, client: &RtmClient) {
        println!("closed!");
    }

    fn on_connect(&mut self, client: &RtmClient) {
        println!("connected");
    }
}

fn main() {
    let uri = INSPIRATION_URI.parse().unwrap();
    let api_key = env::var_os(API_ENV_VAR).unwrap();
    let api_key = api_key.to_str()
        .map(|s| String::from(s))
        .unwrap();
    let mut core = Core::new().unwrap();
    let mut inspiration = Inspiration::new(&mut core, uri);
    let client = RtmClient::login_and_run(&api_key, &mut inspiration);

    match client {
        Err(err) => panic!("Error: {}", err),
        _ => (),
    }
}
