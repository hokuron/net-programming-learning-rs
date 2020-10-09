mod packets;

use crate::packets::GettableEndPoints;
use clap::Clap;
use log::{error, info};
use pnet::datalink::{self, Channel::Ethernet};
use pnet::packet::{
    ethernet::{EtherTypes, EthernetPacket},
    ip::IpNextHeaderProtocols,
    ipv4::Ipv4Packet,
    ipv6::Ipv6Packet,
    tcp::TcpPacket,
    udp::UdpPacket,
    Packet,
};
use std::env;

const WIDTH: usize = 20;

#[derive(Clap)]
struct Opts {
    interface: String,
}

fn main() -> anyhow::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let interface_name = Opts::parse().interface;
    let interfaces = datalink::interfaces();
    let interface = interfaces
        .iter()
        .find(|iface| iface.name == interface_name)
        .unwrap_or_else(|| {
            panic!(
                "Failed to get interface {} in {:?}",
                interface_name,
                interfaces
                    .iter()
                    .map(|iface| &iface.name)
                    .collect::<Vec<_>>()
            )
        });

    info!("found interface: {}", interface);

    let mut receiver = datalink::channel(&interface, Default::default())
        .map(|ch| {
            if let Ethernet(_tx, rx) = ch {
                return Some(rx);
            }
            None
        })?
        .expect("Channel is Ethernet");

    loop {
        match receiver.next() {
            Ok(frame) => {
                // Data Link Layer
                // Ethernet header + Payload
                let frame = EthernetPacket::new(frame).unwrap();
                match frame.get_ethertype() {
                    EtherTypes::Ipv4 => handle_ipv4(&frame),
                    EtherTypes::Ipv6 => handle_ipv6(&frame),
                    _ => info!("Not an IPv4 or IPv6 packet"),
                }
            }
            Err(e) => error!("Failed to read {}", e),
        }
    }
}

fn handle_ipv4(ethernet: &EthernetPacket) {
    // Internet Protocol (Network Layer)
    // IP header + Payload
    let packet = match Ipv4Packet::new(ethernet.payload()) {
        Some(packet) => packet,
        None => return,
    };

    match packet.get_next_level_protocol() {
        IpNextHeaderProtocols::Tcp => handle_tcp(&packet),
        IpNextHeaderProtocols::Udp => handle_udp(&packet),
        _ => info!("Not a TCP or UDP packet"),
    }
}

fn handle_ipv6(ethernet: &EthernetPacket) {
    // Internet Protocol (Network Layer)
    // IP header + Payload
    let packet = match Ipv6Packet::new(ethernet.payload()) {
        Some(packet) => packet,
        None => return,
    };

    match packet.get_next_header() {
        IpNextHeaderProtocols::Tcp => handle_tcp(&packet),
        IpNextHeaderProtocols::Udp => handle_udp(&packet),
        _ => info!("Not a TCP or UDP packet"),
    }
}

fn handle_tcp(packet: &dyn GettableEndPoints) {
    // Transmission Control Protocol (Transport Layer)
    // TCP header + Payload (Application Layer data)
    let tcp = TcpPacket::new(packet.get_payload());
    if let Some(tcp) = tcp {
        print_packet_info(packet, &tcp, "TCP")
    }
}

fn handle_udp(packet: &dyn GettableEndPoints) {
    // User Datagram Protocol (Transport Layer)
    // UDP header + Payload (Application Layer data)
    let udp = UdpPacket::new(packet.get_payload());
    if let Some(udp) = udp {
        print_packet_info(packet, &udp, "UDP")
    }
}

fn print_packet_info(l3: &dyn GettableEndPoints, l4: &dyn GettableEndPoints, proto: &str) {
    println!(
        "Captured a {} packet from {}|{} to {}|{}",
        proto,
        l3.get_source(),
        l4.get_source(),
        l3.get_destination(),
        l4.get_destination()
    );
    let payload = l4.get_payload();
    let payload_len = payload.len();

    for i in 0..payload_len {
        print!("{:<02X} ", payload[i]);

        if i % WIDTH == WIDTH - 1 || i == payload_len - 1 {
            for _j in 0..WIDTH - 1 - (i % (WIDTH)) {
                print!("    ");
            }
            print!("| ");

            for j in (i - i % WIDTH)..=i {
                if payload[j].is_ascii_alphabetic() {
                    print!("{}", payload[j] as char);
                } else {
                    print!(".");
                }
            }
            println!();
        }
    }
    println!("{}", "=".repeat(WIDTH * 3));
    println!();
}
