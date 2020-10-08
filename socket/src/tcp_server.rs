use log::{debug, error};
use std::net::{TcpListener, TcpStream};
use std::{str, thread};
use std::io::{Read, Write};

pub fn server(address: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(address)?;
    loop {
        let (stream, _) = listener.accept()?;
        thread::spawn(move || {
            handler(stream).unwrap_or_else(|error| error!("{:?}", error));
        });
    }
}

fn handler(mut stream: TcpStream) -> anyhow::Result<()> {
    debug!("Handling data from {}", stream.peer_addr()?);

    let mut buf = [0u8; 1024];

    loop {
        let nbytes = stream.read(&mut buf)?;

        if nbytes == 0 {
            debug!("Connection closed");
            return Ok(());
        }

        print!("{}", str::from_utf8(&buf[..nbytes])?);
        stream.write_all(&buf[..nbytes])?;
    }
}
