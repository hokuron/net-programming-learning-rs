use super::{repository, util};
use crate::dhcp::DhcpOptions::RequestedIpAddress;
use anyhow::{anyhow, Context};
use byteorder::{BigEndian, ByteOrder};
use ipnetwork::Ipv4Network;
use log::{debug, info};
use pnet::datalink::MacAddr;
use pnet::packet::PrimitiveValues;
use rusqlite::Connection;
use serde::export::Formatter;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Mutex, RwLock};

const OP: usize = 0;
const HTYPE: usize = 1;
const HLEN: usize = 2;
#[allow(dead_code)]
const HOPS: usize = 3;
const XID: usize = 4;
const SECS: usize = 8;
const FLAGS: usize = 10;
const CIADDR: usize = 12;
const YIADDR: usize = 16;
const SIADDR: usize = 20;
const GIADDR: usize = 24;
const CHADDR: usize = 28;
const SNAME: usize = 44;
#[allow(dead_code)]
const FILE: usize = 108;
const OPTIONS: usize = 236;

const MAGIC_COOKIE: [u8; 4] = [0x63, 0x82, 0x53, 0x63];

const DHCP_SIZE: usize = 400;
#[allow(dead_code)]
const BOOTREQUEST: u8 = 1;
const BOOTREPLY: u8 = 2;
const HTYPE_ETHER: u8 = 1;

const PACKET_MINIMUM_SIZE: usize = 237;

pub enum DhcpOptions {
    MessageType = 53,
    IpAddressLeaseTime = 51,
    ServerIdentifier = 54,
    RequestedIpAddress = 50,
    SubnetMask = 1,
    Router = 3,
    Dns = 6,
    End = 255,
}

impl DhcpOptions {
    fn code(&self) -> u8 {
        match self {
            DhcpOptions::MessageType => 53,
            DhcpOptions::IpAddressLeaseTime => 51,
            DhcpOptions::ServerIdentifier => 54,
            DhcpOptions::RequestedIpAddress => 50,
            DhcpOptions::SubnetMask => 1,
            DhcpOptions::Router => 3,
            DhcpOptions::Dns => 6,
            DhcpOptions::End => 255,
        }
    }

    fn len(&self) -> usize {
        match self {
            DhcpOptions::MessageType => 1,
            DhcpOptions::IpAddressLeaseTime => 4,
            DhcpOptions::ServerIdentifier => 4,
            DhcpOptions::RequestedIpAddress => 4,
            DhcpOptions::SubnetMask => 4,
            DhcpOptions::Router => 4,
            DhcpOptions::Dns => 4,
            DhcpOptions::End => 0,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct MessageType(pub u8);

impl MessageType {
    pub const DHCPDISCOVER: Self = Self(1);
    pub const DHCPOFFER: Self = Self(2);
    pub const DHCPREQUEST: Self = Self(3);
    pub const DHCPACK: Self = Self(5);
    pub const DHCPNAK: Self = Self(6);
    pub const DHCPRELEASE: Self = Self(7);
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let str = match self.0 {
            1 => "DHCPDISCOVER",
            2 => "DHCPOFFER",
            3 => "DHCPREQUEST",
            5 => "DHCPACK",
            6 => "DHCPNAK",
            7 => "DHCPRELEASE",
            _ => "Unknown MessageType",
        };
        write!(f, "{}", str)
    }
}

pub struct DhcpServer {
    addr_pool: RwLock<Vec<Ipv4Addr>>,
    pub db_conn: Mutex<Connection>,
    pub network_addr: Ipv4Network,
    pub svr_addr: Ipv4Addr,
    pub default_gateway: Ipv4Addr,
    pub subnet_mask: Ipv4Addr,
    pub dns_svr: Ipv4Addr,
    pub lease_time: Vec<u8>,
}

impl DhcpServer {
    pub fn new() -> anyhow::Result<Self> {
        let env = util::Environment::new()?;
        let prefixed_network_addr = Ipv4Network::new(
            env.network_addr,
            ipnetwork::ipv4_mask_to_prefix(env.subnet_mask)?,
        )?;
        let conn = Connection::open("shared/dhcp.db")?;
        let addr_pool = Self::init_addr_pool(&conn, &env, prefixed_network_addr)?;
        info!("There are {} address in the address pool", addr_pool.len());
        let lease_time = util::big_endian_from(env.lease_time)?;
        Ok(DhcpServer {
            addr_pool: RwLock::new(addr_pool),
            db_conn: Mutex::new(conn),
            network_addr: prefixed_network_addr,
            svr_addr: env.dhcp_svr_addr,
            default_gateway: env.default_gateway,
            subnet_mask: env.subnet_mask,
            dns_svr: env.dns_svr_addr,
            lease_time,
        })
    }

    fn init_addr_pool(
        conn: &Connection,
        env: &util::Environment,
        prefixed_network_addr: Ipv4Network,
    ) -> anyhow::Result<Vec<Ipv4Addr>> {
        let mut used_ip_addrs = repository::find_all_addrs(conn, false)?;
        used_ip_addrs.push(env.network_addr);
        used_ip_addrs.push(env.default_gateway);
        used_ip_addrs.push(env.dhcp_svr_addr);
        used_ip_addrs.push(env.dns_svr_addr);
        used_ip_addrs.push(prefixed_network_addr.broadcast());
        let mut ret = prefixed_network_addr
            .iter()
            .filter(|addr| !used_ip_addrs.contains(addr))
            .collect::<Vec<_>>();
        ret.reverse();
        Ok(ret)
    }

    fn send_broadcast_response(
        &self,
        transmission_soc: &UdpSocket,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let dest = "255.255.255.255:68".parse::<SocketAddr>()?;
        transmission_soc.send_to(data, dest)?;
        Ok(())
    }

    pub fn find_available_ip_addr(&self) -> Option<Ipv4Addr> {
        let mut lock = self.addr_pool.write().unwrap();
        lock.pop()
    }

    pub fn find_ip_addr(&self, ip_addr: Ipv4Addr) -> Option<Ipv4Addr> {
        let mut lock = self.addr_pool.write().unwrap();
        for i in 0..lock.len() {
            if lock[i] == ip_addr {
                return Some(lock.remove(i));
            }
        }
        None
    }
}

impl DhcpServer {
    pub fn offer_network_addr(
        &self,
        recv_packet: &DhcpPacket,
        transmission_soc: &UdpSocket,
    ) -> anyhow::Result<()> {
        let transaction_id = recv_packet.transaction_id();
        info!("{:x}: received DHCPDISCOVER", transaction_id);

        let ip_addr_to_lease = self.choose_leased_ip_addr(recv_packet)?;
        let offer_packet =
            self.make_dhcp_packet(recv_packet, MessageType::DHCPOFFER, ip_addr_to_lease)?;
        self.send_broadcast_response(transmission_soc, offer_packet.buf())?;

        info!("{:x}: sent DHCPOFFER", transaction_id);
        Ok(())
    }

    fn choose_leased_ip_addr(&self, recv_packet: &DhcpPacket) -> anyhow::Result<Ipv4Addr> {
        let conn = self.db_conn.lock().unwrap();
        if let Some(used_ip_addr) = repository::find_addr(&conn, recv_packet.chaddr())? {
            if self.network_addr.contains(used_ip_addr)
                && util::is_ip_addr_available(used_ip_addr).is_ok()
            {
                return Ok(used_ip_addr);
            }
        }

        if let Some(ip_addr) = recv_packet
            .requested_ip_addr()
            .and_then(|addr| self.find_ip_addr(addr))
        {
            if util::is_ip_addr_available(ip_addr).is_ok() {
                return Ok(ip_addr);
            }
        }

        while let Some(ip_addr) = self.find_available_ip_addr() {
            if util::is_ip_addr_available(ip_addr).is_ok() {
                return Ok(ip_addr);
            }
        }

        Err(anyhow!("There are no available IP addresses"))
    }
}

impl DhcpServer {
    pub fn allocate_ip_addr(
        &self,
        svr_id: &[u8],
        recv_packet: &DhcpPacket,
        transmission_soc: &UdpSocket,
    ) -> anyhow::Result<()> {
        let transaction_id = recv_packet.transaction_id();

        info!(
            "{:x}: received DHCPREQUEST with server identifier",
            transaction_id
        );

        let svr_ip_addr = util::ipv4_addr_from(svr_id).with_context(|| {
            format!(
                "Server identifier could not be converted to IP address: {:?}",
                svr_id
            )
        })?;

        if svr_ip_addr != self.svr_addr {
            info!("Client has chosen another DHCP server");
            return Ok(());
        }

        let offered_ip_addr = recv_packet.option(DhcpOptions::RequestedIpAddress).unwrap();
        let ip_addr_to_lease = util::ipv4_addr_from(&offered_ip_addr).with_context(|| {
            format!(
                "Requested (offered) IP address is invalid format: {:?}",
                offered_ip_addr
            )
        })?;

        let mut conn = self.db_conn.lock().unwrap();
        {
            let transaction = conn.transaction()?;
            repository::create_or_update((recv_packet.chaddr(), ip_addr_to_lease), &transaction)?;
            let ack_packet =
                self.make_dhcp_packet(recv_packet, MessageType::DHCPACK, ip_addr_to_lease)?;
            self.send_broadcast_response(transmission_soc, ack_packet.buf())?;

            info!("{:x}: sent DHCPACK", transaction_id);

            transaction.commit()?;
        }

        debug!(
            "{:x}: leased IP address: {}",
            transaction_id, ip_addr_to_lease
        );

        Ok(())
    }
}

impl DhcpServer {
    pub fn reallocate_ip_addr(
        &self,
        recv_packet: &DhcpPacket,
        transmission_soc: &UdpSocket,
    ) -> anyhow::Result<()> {
        let transaction_id = recv_packet.transaction_id();

        info!(
            "{:x}: received DHCPREQUEST without server identifier",
            transaction_id
        );

        let ip_addr_to_lease = match recv_packet.option(RequestedIpAddress) {
            Some(requested_ip_addr) => {
                let requested_ip_addr =
                    util::ipv4_addr_from(&requested_ip_addr).with_context(|| {
                        format!(
                            "Requested IP address is invalid format: {:?}",
                            requested_ip_addr
                        )
                    })?;

                debug!("client is in INIT-REBOOT state");

                let conn = self.db_conn.lock().unwrap();
                let ip_addr = match repository::find_addr(&conn, recv_packet.chaddr())? {
                    Some(ip_addr) => ip_addr,
                    None => return Ok(()),
                };

                if ip_addr == requested_ip_addr && self.network_addr.contains(ip_addr) {
                    Some(ip_addr)
                } else {
                    None
                }
            }
            None => {
                debug!("client is in RENEWING or REBINDING state");

                let requested_ip_addr = recv_packet.ciaddr();
                if self.network_addr.contains(requested_ip_addr) {
                    Some(requested_ip_addr)
                } else {
                    return Err(anyhow!("Invalid ciaddr is mismatched network address"));
                }
            }
        };

        let snd_packet = if let Some(ip_addr_to_lease) = ip_addr_to_lease {
            self.make_dhcp_packet(recv_packet, MessageType::DHCPACK, ip_addr_to_lease)?
        } else {
            self.make_dhcp_packet(
                recv_packet,
                MessageType::DHCPNAK,
                "0.0.0.0".parse().unwrap(),
            )?
        };
        self.send_broadcast_response(transmission_soc, snd_packet.buf())?;

        info!(
            "{:x}: sent {}",
            transaction_id,
            MessageType(snd_packet.option(DhcpOptions::MessageType).unwrap()[0])
        );

        Ok(())
    }
}

impl DhcpServer {
    pub fn release_ip_addr(&self, recv_packet: &DhcpPacket) -> anyhow::Result<()> {
        let transaction_id = recv_packet.transaction_id();
        info!("{:x}: received DHCPRELEASE", transaction_id);

        let mut conn = self.db_conn.lock().unwrap();
        let transaction = conn.transaction()?;
        repository::destroy(recv_packet.chaddr(), &transaction)?;
        transaction.commit()?;

        debug!("{:x}: deleted from DB", transaction_id);

        let mut lock = self.addr_pool.write().unwrap();
        lock.insert(0, recv_packet.ciaddr());

        Ok(())
    }
}

impl DhcpServer {
    fn make_dhcp_packet(
        &self,
        recv_packet: &DhcpPacket,
        message_type: MessageType,
        ip_addr_to_lease: Ipv4Addr,
    ) -> anyhow::Result<DhcpPacket> {
        let buf = vec![0u8; DHCP_SIZE];
        let mut dhcp_packet = DhcpPacket::new(buf).unwrap();
        dhcp_packet.set_op(BOOTREPLY);
        dhcp_packet.set_htype(HTYPE_ETHER);
        dhcp_packet.set_hlen(6);
        dhcp_packet.set_xid(recv_packet.xid());
        if message_type == MessageType::DHCPACK {
            dhcp_packet.set_ciaddr(recv_packet.ciaddr());
        }
        dhcp_packet.set_yiaddr(ip_addr_to_lease);
        dhcp_packet.set_flags(recv_packet.flags());
        dhcp_packet.set_giaddr(recv_packet.giaddr());
        dhcp_packet.set_chaddr(recv_packet.chaddr());

        let mut cursor = OPTIONS;
        dhcp_packet.set_magic_cookie(&mut cursor);
        dhcp_packet.set_option(
            DhcpOptions::MessageType,
            Some(&[message_type.0]),
            &mut cursor,
        );
        dhcp_packet.set_option(
            DhcpOptions::IpAddressLeaseTime,
            Some(&self.lease_time),
            &mut cursor,
        );
        dhcp_packet.set_option(
            DhcpOptions::ServerIdentifier,
            Some(&self.svr_addr.octets()),
            &mut cursor,
        );
        dhcp_packet.set_option(
            DhcpOptions::SubnetMask,
            Some(&self.subnet_mask.octets()),
            &mut cursor,
        );
        dhcp_packet.set_option(
            DhcpOptions::Router,
            Some(&self.default_gateway.octets()),
            &mut cursor,
        );
        dhcp_packet.set_option(DhcpOptions::Dns, Some(&self.dns_svr.octets()), &mut cursor);
        dhcp_packet.set_option(DhcpOptions::End, None, &mut cursor);

        Ok(dhcp_packet)
    }
}

pub struct DhcpPacket {
    buf: Vec<u8>,
}

impl DhcpPacket {
    pub fn new(buf: Vec<u8>) -> Option<DhcpPacket> {
        if buf.len() > PACKET_MINIMUM_SIZE {
            Some(DhcpPacket { buf })
        } else {
            None
        }
    }

    fn buf(&self) -> &[u8] {
        self.buf.as_ref()
    }

    pub fn op(&self) -> u8 {
        self.buf[OP]
    }

    pub fn xid(&self) -> &[u8] {
        &self.buf[XID..SECS]
    }

    pub fn transaction_id(&self) -> u32 {
        BigEndian::read_u32(self.xid())
    }

    pub fn flags(&self) -> &[u8] {
        &self.buf[FLAGS..CIADDR]
    }

    pub fn ciaddr(&self) -> Ipv4Addr {
        let v = &self.buf[CIADDR..YIADDR];
        Ipv4Addr::new(v[0], v[1], v[2], v[3])
    }

    pub fn giaddr(&self) -> Ipv4Addr {
        let v = &self.buf[GIADDR..CHADDR];
        Ipv4Addr::new(v[0], v[1], v[2], v[3])
    }

    pub fn chaddr(&self) -> MacAddr {
        let v = &self.buf[CHADDR..SNAME];
        MacAddr::new(v[0], v[1], v[2], v[3], v[4], v[5])
    }

    pub fn options(&self) -> &[u8] {
        &self.buf[OPTIONS..]
    }

    pub fn option(&self, option: DhcpOptions) -> Option<Vec<u8>> {
        let mut index: usize = MAGIC_COOKIE.len();
        let options = self.options();

        while options[index] != DhcpOptions::End.code() {
            if options[index] == option.code() {
                let len = options[index + 1] as usize;
                let buf_idx = index + 2;
                let data = options[buf_idx..buf_idx + len].to_vec();
                return Some(data);
            } else if options[index] == 0 {
                index += 1;
            } else {
                let len = options[index + 1] as usize;
                let buf_idx = index + 2;
                index += buf_idx + len;
            }
        }
        None
    }

    pub fn set_op(&mut self, op: u8) {
        self.buf[OP] = op;
    }

    pub fn set_htype(&mut self, htype: u8) {
        self.buf[HTYPE] = htype;
    }

    pub fn set_hlen(&mut self, hlen: u8) {
        self.buf[HLEN] = hlen;
    }

    pub fn set_xid(&mut self, xid: &[u8]) {
        self.buf[XID..SECS].copy_from_slice(xid);
    }

    pub fn set_flags(&mut self, flags: &[u8]) {
        self.buf[FLAGS..CIADDR].copy_from_slice(flags);
    }

    pub fn set_ciaddr(&mut self, ciaddr: Ipv4Addr) {
        self.buf[CIADDR..YIADDR].copy_from_slice(&ciaddr.octets());
    }

    pub fn set_yiaddr(&mut self, yiaddr: Ipv4Addr) {
        self.buf[YIADDR..SIADDR].copy_from_slice(&yiaddr.octets());
    }

    pub fn set_giaddr(&mut self, giaddr: Ipv4Addr) {
        self.buf[GIADDR..CHADDR].copy_from_slice(&giaddr.octets());
    }

    pub fn set_chaddr(&mut self, chaddr: MacAddr) {
        let v = chaddr.to_primitive_values();
        let mac_addr = [v.0, v.1, v.2, v.3, v.4, v.5];
        self.buf[CHADDR..mac_addr.len()].copy_from_slice(&mac_addr);
    }

    pub fn set_magic_cookie(&mut self, cursor: &mut usize) {
        self.buf[*cursor..*cursor + MAGIC_COOKIE.len()].copy_from_slice(&MAGIC_COOKIE);
        *cursor += MAGIC_COOKIE.len();
    }

    pub fn set_option(&mut self, option: DhcpOptions, data: Option<&[u8]>, cursor: &mut usize) {
        self.buf[*cursor] = option.code();

        if option.code() == DhcpOptions::End.code() {
            return;
        }

        *cursor += 1;

        self.buf[*cursor] = option.len() as u8;
        *cursor += 1;

        if let Some(data) = data {
            self.buf[*cursor..*cursor + data.len()].copy_from_slice(data)
        }

        *cursor += 1;
    }
}

impl DhcpPacket {
    fn requested_ip_addr(&self) -> Option<Ipv4Addr> {
        let buf = self.option(DhcpOptions::RequestedIpAddress)?;
        util::ipv4_addr_from(&buf)
    }
}
