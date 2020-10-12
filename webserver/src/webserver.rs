use anyhow::{anyhow, Context};
use log::{debug, error};
use mio::net::{TcpListener, TcpStream};
use mio::{
    event::{Event, Events},
    Interest, Poll, Token,
};
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, ErrorKind, Read, Write};
use std::net::SocketAddr;
use std::{env, str};

const SERVER: Token = Token(0);
const WEBROOT: &str = "/webroot";

pub struct WebServer {
    listening_soc: TcpListener,
    conns: HashMap<usize, TcpStream>,
    next_conn_id: usize,
}

impl WebServer {
    pub fn new(addr: SocketAddr) -> anyhow::Result<Self> {
        let listening_soc = TcpListener::bind(addr)?;
        Ok(WebServer {
            listening_soc,
            conns: HashMap::new(),
            next_conn_id: 1,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut poll = Poll::new()?;
        poll.registry()
            .register(&mut self.listening_soc, SERVER, Interest::READABLE)?;

        let mut events = Events::with_capacity(1024);
        let mut response = Vec::new();

        loop {
            // Wait for an event to occur (blocking a thread).
            match poll.poll(&mut events, None) {
                Ok(_) => {}
                Err(e) => {
                    error!("{}", e);
                    continue;
                }
            }

            for event in &events {
                match event.token() {
                    // An event for the listening socket
                    SERVER => {
                        // The `PollOpt` has been removed in v0.7 and only the edge-triggered are now supported.
                        // https://tokio.rs/blog/2019-12-mio-v0.7-alpha.1#moving-to-edge-triggers
                        // Rewrite to the edge trigger version.
                        loop {
                            let (stream, remote_addr) = match self.listening_soc.accept() {
                                Ok(t) => t,
                                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                                Err(e) => {
                                    error!("{}", e);
                                    continue;
                                }
                            };
                            debug!("Connection from {}", &remote_addr);

                            self.register_conn(&poll, stream)?;
                        }
                    }
                    // A read or write event fo the connected socket
                    Token(conn_id) => self.handle_http(conn_id, event, &poll, &mut response)?,
                }
            }
        }
    }

    fn register_conn(&mut self, poll: &Poll, mut stream: TcpStream) -> anyhow::Result<()> {
        let token = Token(self.next_conn_id);
        poll.registry()
            .register(&mut stream, token, Interest::READABLE)?;

        if self.conns.insert(self.next_conn_id, stream).is_some() {
            error!("Connection ID is already exist.");
        }

        self.next_conn_id += 1;

        Ok(())
    }
    fn handle_http(
        &mut self,
        conn_id: usize,
        event: &Event,
        poll: &Poll,
        response: &mut Vec<u8>,
    ) -> anyhow::Result<()> {
        let stream = self
            .conns
            .get_mut(&conn_id)
            .context("Failed to get connection")?;
        if event.is_readable() {
            debug!("readable conn_id: {}", conn_id);
            let mut buf = [0u8; 1024];
            let nbytes = stream.read(&mut buf)?;

            if nbytes != 0 {
                *response = make_response(&buf[..nbytes])?;
                poll.registry()
                    .reregister(stream, Token(conn_id), Interest::WRITABLE)?;
            } else {
                self.conns.remove(&conn_id);
            }
            Ok(())
        } else if event.is_writable() {
            debug!("writable conn_id: {}", conn_id);
            stream.write_all(response)?;
            self.conns.remove(&conn_id);
            Ok(())
        } else {
            Err(anyhow!("Undefined event: {:?}", event))
        }
    }
}

fn make_response(buf: &[u8]) -> anyhow::Result<Vec<u8>> {
    let http_pattern = Regex::new(r"(.*) (.*) HTTP/1.([0-1])\n.*")?;
    let captures = match http_pattern.captures(str::from_utf8(buf)?) {
        Some(cap) => cap,
        None => return make_msg_from_code(400, None),
    };
    let method = captures[1].to_string();
    let path = format!(
        "{}{}{}",
        env::current_dir()?.display(),
        WEBROOT,
        &captures[2]
    );
    let _version = captures[3].to_string();

    if method == "GET" {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(_) => return make_msg_from_code(404, None),
        };
        let mut reader = BufReader::new(file);
        let mut file_buf = Vec::new();
        reader.read_to_end(&mut file_buf)?;
        make_msg_from_code(200, Some(file_buf))
    } else {
        make_msg_from_code(501, None)
    }
}

fn make_msg_from_code(stat_code: u16, msg: Option<Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    match stat_code {
        200 => {
            let mut header = "HTTP/1.0 200 OK\r\n Server: mio webserver\r\n\r\n"
                .to_string()
                .into_bytes();
            if let Some(mut msg) = msg {
                header.append(&mut msg);
            }
            Ok(header)
        }
        400 => Ok("HTTP/1.0 400 Bad Request\r\n Server: mio webserver\r\n\r\n"
            .to_string()
            .into_bytes()),
        501 => Ok(
            "HTTP/1.0 501 Not Implemented\r\n Server: mio webserver\r\n\r\n"
                .to_string()
                .into_bytes(),
        ),
        _ => Err(anyhow!("Undefined status code: {}", stat_code)),
    }
}
