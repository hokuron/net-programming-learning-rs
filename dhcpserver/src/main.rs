mod dhcp;
mod repository;
mod util;

use crate::dhcp::{DhcpOptions, DhcpPacket, DhcpServer, MessageType};
use anyhow::{anyhow, Context};
use log::{debug, error};
use std::env;
use std::net::UdpSocket;
use std::sync::Arc;
use std::thread;

const BOOTREQUEST: u8 = 1;
#[allow(dead_code)]
const BOOTREPLY: u8 = 2;

fn main() -> anyhow::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let svr_soc = UdpSocket::bind("0.0.0.0:0").context("Failed to bind socket")?;
    svr_soc.set_broadcast(true)?;

    let dhcp_svr = Arc::new(DhcpServer::new().context("Failed to start DHCP server")?);

    loop {
        let mut recv_buf = [0u8; 1024];
        let (size, src) = svr_soc
            .recv_from(&mut recv_buf)
            .context("A datagram could not be receive")?;
        debug!("receive data from {}, size: {}", src, size);
        let transmission_soc = svr_soc.try_clone().expect("Failed to create client socket");
        let dhcp_svr = dhcp_svr.clone();

        thread::spawn(move || {
            let dhcp_packet = match DhcpPacket::new(recv_buf[..size].to_vec()) {
                Some(packet) if packet.op() == BOOTREQUEST => packet,
                Some(_) | None => return,
            };

            let result = handle_dhcp(&dhcp_packet, &transmission_soc, dhcp_svr);
            if let Err(e) = result {
                error!("{}", e);
            }
        });
    }
}

fn handle_dhcp(
    packet: &DhcpPacket,
    transmission_soc: &UdpSocket,
    server: Arc<DhcpServer>,
) -> anyhow::Result<()> {
    let message = packet
        .option(DhcpOptions::MessageType)
        .context("Specified option was not found")?;
    let message_type = MessageType(message[0]);

    match message_type {
        MessageType::DHCPDISCOVER => server.offer_network_addr(packet, transmission_soc),
        MessageType::DHCPREQUEST => match packet.option(DhcpOptions::ServerIdentifier) {
            Some(svr_id) => server.allocate_ip_addr(&svr_id, packet, transmission_soc),
            None => server.reallocate_ip_addr(packet, transmission_soc),
        },
        MessageType::DHCPRELEASE => server.release_ip_addr(packet),
        _ => Err(anyhow!(
            "{:x}: received unimplemented message, message type: {}",
            packet.transaction_id(),
            message_type.0
        )),
    }
}
