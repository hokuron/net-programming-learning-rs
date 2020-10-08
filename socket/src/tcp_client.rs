use std::io::{self, Write, BufReader, BufRead};
use std::str;
use std::net::TcpStream;

pub fn connect(address: &str) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect(address)?;
    loop {
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
        stream.write_all(input.as_bytes())?;

        let mut reader = BufReader::new(&stream);
        let mut buf = Vec::new();
        let _ = reader.read_until(b'\n', &mut buf);
        print!("{}", str::from_utf8(&buf)?);
    }
}