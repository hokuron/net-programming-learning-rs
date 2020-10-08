use std::env;
use log::error;
use clap::Clap;

mod tcp_server;
mod tcp_client;
mod udp_server;
mod udp_client;

#[derive(Clap, Debug)]
struct Opts {
    #[clap(arg_enum)]
    protocol: Protocol,
    #[clap(arg_enum)]
    role: Role,
    #[clap(long = "host", default_value = "127.0.0.1:33333")]
    address: String,
}

#[derive(Clap, Debug)]
enum Protocol {
    Tcp,
    Udp,
}

#[derive(Clap, Debug)]
enum Role {
    Server,
    Client,
}

fn main() {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let opts = Opts::parse();
    let result = match (opts.protocol, opts.role) {
        (Protocol::Tcp, Role::Server) => {
            tcp_server::server(&opts.address)
        }
        (Protocol::Tcp, Role::Client) => {
            tcp_client::connect(&opts.address)
        }
        (Protocol::Udp, Role::Server) => {
            udp_server::server(&opts.address)
        }
        (Protocol::Udp, Role::Client) => {
            udp_client::communicate(&opts.address)
        }
    };
    result.unwrap_or_else(|err| error!("{}", err));
}
