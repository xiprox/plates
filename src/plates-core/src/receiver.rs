//! The receiver on `127.0.0.1` that `plates-hook` forwards each Claude Code hook event to.
//!
//! It answers each request with an empty 200 before reading the event, so a slow Plates never
//! holds up the forwarder.

use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    sync::mpsc::Sender,
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::hooks::{self, HookMessage};

/// The port Plates listens on and `plates-hook` forwards to, which both read from here.
pub const PORT: u16 = 47231;

/// The path events are posted to, which also marks an `http` hook from an earlier install of
/// Plates in the settings file, so installing again replaces it.
pub const PATH: &str = "/plates/hook";

pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}{PATH}")
}

/// What the receiver hands on: each event it read, and each body it couldn't read, raw.
pub enum Received {
    // A HookMessage runs to about 260 bytes, so it's boxed to keep an Unreadable small.
    Event(Box<HookMessage>, Vec<u8>),
    Unreadable(String, Vec<u8>),
}

pub struct Receiver {
    pub port: u16,
    _thread: JoinHandle<()>,
}

impl Receiver {
    /// Listens on `127.0.0.1:port`, sending what it receives to `sink` from a thread of its own.
    /// It fails if the port is taken, such as by a second Plates.
    pub fn start(port: u16, sink: Sender<Received>) -> io::Result<Receiver> {
        let server =
            tiny_http::Server::http((Ipv4Addr::LOCALHOST, port)).map_err(io::Error::other)?;
        let thread = thread::Builder::new()
            .name("plates-receiver".into())
            .spawn(move || {
                for mut request in server.incoming_requests() {
                    let mut body = Vec::new();
                    let read = request.as_reader().read_to_end(&mut body);
                    let _ = request.respond(tiny_http::Response::empty(200));
                    let received = match read
                        .map_err(|e| e.to_string())
                        .and_then(|_| hooks::parse(&body).map_err(|e| e.to_string()))
                    {
                        Ok(message) => Received::Event(Box::new(message), body),
                        Err(e) => Received::Unreadable(e, body),
                    };
                    if sink.send(received).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Receiver {
            port,
            _thread: thread,
        })
    }
}

/// Posts `body` to a receiver, as `plates-hook` does for each hook event, giving up when nothing
/// takes the connection within `connect_timeout`. It returns the receiver's response.
pub fn forward(port: u16, body: &[u8], connect_timeout: Duration) -> io::Result<String> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, connect_timeout)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "POST {PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionEvent;
    use std::sync::mpsc;

    #[test]
    fn the_receiver_answers_empty_and_hands_on_the_event() {
        let (tx, rx) = mpsc::channel();
        // Port 0 isn't usable here, since the hooks need a fixed port, so the test takes one well
        // away from Plates' own.
        let port = 47331;
        let _receiver = Receiver::start(port, tx).unwrap();
        let body = br#"{"session_id":"abc","cwd":"/w","hook_event_name":"Stop","last_assistant_message":"Done."}"#;
        let response = forward(port, body, Duration::from_secs(1)).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.ends_with("\r\n\r\n"), "an empty body: {response}");
        let Received::Event(message, raw) = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
            panic!("the body should parse");
        };
        assert_eq!(raw, body);
        assert_eq!(
            message.event,
            Some(SessionEvent::TurnEnded {
                last_message: Some("Done.".into())
            })
        );

        forward(port, b"not json", Duration::from_secs(1)).unwrap();
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Received::Unreadable(..)
        ));
    }

    #[test]
    fn a_second_receiver_on_the_same_port_fails() {
        let (tx, _rx) = mpsc::channel();
        let _first = Receiver::start(47332, tx.clone()).unwrap();
        assert!(Receiver::start(47332, tx).is_err());
    }
}
