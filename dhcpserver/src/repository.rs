use anyhow::Context;
use log::info;
use pnet::datalink::MacAddr;
use rusqlite::{params, Connection, Transaction};
use std::net::Ipv4Addr;

pub fn find_all_addrs(conn: &Connection, deleted: bool) -> anyhow::Result<Vec<Ipv4Addr>> {
    let mut statement = conn.prepare("SELECT ip_addr FROM lease_entries WHERE deleted = ?")?;
    let mut ip_addrs = statement.query(params![if deleted { 1 } else { 0 }.to_string()])?;
    let mut leased_addrs = Vec::new();
    while let Some(Ok(ip_addr)) = ip_addrs.next()?.map(|row| row.get::<_, String>(0)) {
        leased_addrs.push(ip_addr.parse()?);
    }
    Ok(leased_addrs)
}

pub fn find_addr(conn: &Connection, mac_addr: MacAddr) -> anyhow::Result<Option<Ipv4Addr>> {
    let mut statement = conn.prepare("SELECT ip_addr FROM lease_entries WHERE mac_addr = ?1")?;
    let mut rows = statement
        .query(params![mac_addr.to_string()])?
        .and_then(|r| r.get::<_, String>(0));
    if let Some(ip_addr) = rows.next() {
        Ok(Some(ip_addr?.parse()?))
    } else {
        info!("Specified Mac address could not be founded");
        Ok(None)
    }
}

pub fn destroy(mac_addr: MacAddr, tx: &Transaction) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE lease_entries SET deleted = ?1 WHERE mac_addr = ?2",
        params![1.to_string(), mac_addr.to_string()],
    )?;
    Ok(())
}

pub fn create_or_update(entry: (MacAddr, Ipv4Addr), tx: &Transaction) -> anyhow::Result<()> {
    if count_for(entry.0, tx)? == 0 {
        create(entry, tx)
    } else {
        update(entry, tx)
    }
}

fn count_for(mac_addr: MacAddr, tx: &Transaction) -> anyhow::Result<u8> {
    let mut statement = tx.prepare("SELECT COUNT (*) FROM lease_entries WHERE mac_addr = ?")?;
    let mut result = statement.query(params![mac_addr.to_string()])?;
    let count = result
        .next()?
        .map(|r| r.get(0))
        .context("No query returned")?;
    Ok(count?)
}

fn create(entry: (MacAddr, Ipv4Addr), tx: &Transaction) -> anyhow::Result<()> {
    tx.execute(
        "INSERT INTO lease_entries (mac_addr, ipv4_addr) VALUES (?1, ?2)",
        params![entry.0.to_string(), entry.1.to_string()],
    )?;
    Ok(())
}

fn update(entry: (MacAddr, Ipv4Addr), tx: &Transaction) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE lease_entries SET ip_addr = ?2 WHERE mac_addr = ?1",
        params![entry.0.to_string(), entry.1.to_string()],
    )?;
    Ok(())
}
