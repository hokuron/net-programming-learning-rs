use anyhow::{anyhow, Context};
use byteorder::{BigEndian, WriteBytesExt};
use log::{debug, info, warn};
use pnet::packet::icmp::{
    echo_request::{EchoRequestPacket, MutableEchoRequestPacket},
    IcmpTypes,
};
use pnet::packet::{ip::IpNextHeaderProtocols::Icmp, Packet};
use pnet::transport::{self, icmp_packet_iter, TransportChannelType, TransportProtocol::Ipv4};
use pnet::util::checksum;
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, io, str, thread};

#[derive(Deserialize)]
pub struct Environment {
    pub network_addr: Ipv4Addr,
    pub subnet_mask: Ipv4Addr,
    pub default_gateway: Ipv4Addr,
    #[serde(rename = "dhcp_svr_identifier")]
    pub dhcp_svr_addr: Ipv4Addr,
    pub dns_svr_addr: Ipv4Addr,
    pub lease_time: u32,
}

impl Environment {
    pub fn new() -> anyhow::Result<Self> {
        let file = fs::File::open("shared/env.json")?;
        serde_json::from_reader(file).context("Invalid env file format")
    }
}

pub fn big_endian_from(i: u32) -> Result<Vec<u8>, io::Error> {
    let mut v = Vec::new();
    v.write_u32::<BigEndian>(i)?;
    Ok(v)
}

pub fn is_ip_addr_available(target: Ipv4Addr) -> anyhow::Result<()> {
    let icmp_buf = new_default_icmp_buf();
    let icmp_packet = EchoRequestPacket::new(&icmp_buf).unwrap();
    let (mut transport_snd, mut transport_recv) =
        transport::transport_channel(1024, TransportChannelType::Layer4(Ipv4(Icmp)))?;
    transport_snd.send_to(icmp_packet, IpAddr::V4(target))?;

    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut iter = icmp_packet_iter(&mut transport_recv);
        let (packet, _) = iter.next().unwrap();
        if packet.get_icmp_type() == IcmpTypes::EchoReply {
            match sender.send(true) {
                Err(_) => info!("ICMP timeout"),
                Ok(_) => return,
            }
        }
    });

    if receiver.recv_timeout(Duration::from_millis(200)).is_ok() {
        let message = format!("IP address already in use: {}", target);
        warn!("{}", message);
        Err(anyhow!(message))
    } else {
        debug!("Not received reply within timeout");
        Ok(())
    }
}

fn new_default_icmp_buf() -> [u8; 8] {
    let mut buf = [0u8; 8];
    let mut icmp_packet = MutableEchoRequestPacket::new(&mut buf).unwrap();
    icmp_packet.set_icmp_type(IcmpTypes::EchoRequest);
    let checksum = checksum(icmp_packet.to_immutable().packet(), 16);
    icmp_packet.set_checksum(checksum);
    buf
}

pub fn ipv4_addr_from(buf: &[u8]) -> Option<Ipv4Addr> {
    if buf.len() == 4 {
        Some(Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]))
    } else {
        None
    }
}
