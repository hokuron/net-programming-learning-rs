use log::debug;
use std::net::UdpSocket;
use std::str;

pub fn server(address: &str) -> anyhow::Result<()> {
    let socket = UdpSocket::bind(&address)?;
    loop {
        let mut buf = [0u8; 1024];
        let (size, src) = socket.recv_from(&mut buf)?;
        debug!("handling data from {}", src);
        print!("{}", str::from_utf8(&buf[..size])?);
        socket.send_to(&buf, src)?;
    }
}