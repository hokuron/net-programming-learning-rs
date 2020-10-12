mod webserver;

use clap::Clap;
use std::env;
use std::net::SocketAddr;

#[derive(Clap)]
struct Opts {
    addr: SocketAddr,
}

fn main() -> anyhow::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let opts = Opts::parse();
    let mut server = webserver::WebServer::new(opts.addr)?;
    server.run()
}
