use std::net::UdpSocket;
use std::{io, str};

pub fn communicate(address: &str) -> anyhow::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        socket.send_to(input.as_bytes(), address)?;

        let mut buf = [0u8; 1024];
        let _ = socket.recv(&mut buf).expect("Failed to receive");
        print!("{}", str::from_utf8(&buf)?);
    }
}