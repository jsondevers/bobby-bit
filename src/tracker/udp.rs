use crate::torrent::Torrent;
use crate::utils::generate_peer_id;
use anyhow::{anyhow, Context, Result};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use log::{debug, error, info, trace, warn};
use mio::net::{TcpStream, UdpSocket};
use mio::{Events, Interest, Poll, Token};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_bencode::{from_bytes, to_bytes};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::thread;
use std::time::Duration;
use url::Url;
use urlencoding::{encode, encode_binary};

/// magic constant for UDP tracker protocol, see BEP 15
const UDP_TRACKER_PROTOCOL_ID: u64 = 0x41727101980;

#[derive(Debug, Serialize, Deserialize)]
struct ConnectRequest {
    protocol_id: u64,
    action: u32,
    transaction_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectResponse {
    action: u32,
    transaction_id: u32,
    connection_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnnounceRequest {
    connection_id: u64,
    action: u32,
    transaction_id: u32,
    info_hash: [u8; 20],
    peer_id: [u8; 20],
    downloaded: u64,
    left: i64,
    uploaded: u64,
    event: u32,
    ip_address: u32,
    key: u32,
    num_want: u32,
    port: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnnounceResponse {
    action: u32,
    transaction_id: u32,
    interval: u32,
    leechers: u32,
    seeders: u32,
    peers: Vec<Peer>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScrapeRequest {
    connection_id: u64,
    action: u32,
    transaction_id: u32,
    info_hash: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeResponse {
    action: u32,
    transaction_id: u32,
    seeders: u32,
    completed: u32,
    leechers: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Error {
    action: u32,
    transaction_id: u32,
    message: String,
}

/// peer struct for UDP tracker, note that peer id is not included as it is with the HTTP tracker
#[derive(Debug, Serialize, Deserialize)]
struct Peer {
    ip_address: i32,
    port: i16,
}

#[derive(Debug)]
pub struct UdpTracker {
    socket: UdpSocket,
    connection_id: u64,
    poll: Poll,
    events: Events,
}

impl UdpTracker {
    pub fn new() -> Result<Self> {
        let mut socket = UdpSocket::bind("0.0.0.0:0".parse()?)?;
        let poll = Poll::new()?;
        let token = Token(0);
        poll.registry()
            .register(&mut socket, token, Interest::READABLE)?;
        Ok(Self {
            socket,
            connection_id: 0,
            poll,
            events: Events::with_capacity(1024),
        })
    }

    pub fn connect(&mut self, addr: SocketAddr) -> Result<ConnectResponse> {
        let mut rng = rand::thread_rng();
        let txn_id = rng.gen::<u32>();
        let mut buf = vec![0; 16];
        let req = ConnectRequest {
            protocol_id: UDP_TRACKER_PROTOCOL_ID,
            action: 0, // connect
            transaction_id: txn_id,
        };

        let mut bytes = to_bytes(&req)?;
        buf.append(&mut bytes);

        let mut attempts = 5; // 5 attempts to connect

        loop {
            self.socket.send_to(&buf, addr)?;
            self.poll
                .poll(&mut self.events, Some(Duration::from_secs(5)))?;
            let mut buf = vec![0; 16];
            let (len, _) = self.socket.recv_from(&mut buf)?;
            let res: ConnectResponse = from_bytes(&buf[..len])?;

            if res.transaction_id != txn_id {
                return Err(anyhow!("transaction id mismatch"));
            }

            if res.action == 0 {
                self.connection_id = res.connection_id;
                return Ok(res);
            }

            attempts -= 1;
            if attempts == 0 {
                return Err(anyhow!("connection failed"));
            }
        }
    }

    pub fn announce(&mut self, addr: SocketAddr, torrent: &Torrent) -> Result<AnnounceResponse> {
        let mut rng = rand::thread_rng();
        let txn_id = rng.gen::<u32>();
        let mut buf = vec![0; 98];
        let req = AnnounceRequest {
            connection_id: self.connection_id,
            action: 1, // announce
            transaction_id: txn_id,
            info_hash: torrent.info_hash(),
            peer_id: generate_peer_id(),
            downloaded: 0,
            left: torrent.length(),
            uploaded: 0,
            event: 0,
            ip_address: 0,
            key: 0,
            num_want: -1i32 as u32,
            port: 6881,
        };

        let mut bytes = to_bytes(&req)?;
        buf.append(&mut bytes);

        let mut attempts = 5; // 5 attempts to announce

        loop {
            self.socket.send_to(&buf, addr)?;
            self.poll
                .poll(&mut self.events, Some(Duration::from_secs(5)))?;
            let mut buf = vec![0; 98];
            let (len, _) = self.socket.recv_from(&mut buf)?;
            let res: AnnounceResponse = from_bytes(&buf[..len])?;

            if res.transaction_id != txn_id {
                return Err(anyhow!("transaction id mismatch"));
            }

            if res.action == 1 {
                return Ok(res);
            }

            attempts -= 1;
            if attempts == 0 {
                return Err(anyhow!("connection failed"));
            }
        }
    }

    pub fn scrape(&mut self, addr: SocketAddr, torrent: &Torrent) -> Result<ScrapeResponse> {
        let mut rng = rand::thread_rng();
        let txn_id = rng.gen::<u32>();
        let mut buf = vec![0; 36];
        let req = ScrapeRequest {
            connection_id: self.connection_id,
            action: 2, // scrape
            transaction_id: txn_id,
            info_hash: torrent.info_hash().to_vec(),
        };

        let mut bytes = to_bytes(&req)?;
        buf.append(&mut bytes);

        let mut attempts = 5; // 5 attempts to scrape

        loop {
            self.socket.send_to(&buf, addr)?;
            self.poll
                .poll(&mut self.events, Some(Duration::from_secs(5)))?;
            let mut buf = vec![0; 36];
            let (len, _) = self.socket.recv_from(&mut buf)?;
            let res: ScrapeResponse = from_bytes(&buf[..len])?;

            if res.transaction_id != txn_id {
                return Err(anyhow!("transaction id mismatch"));
            }

            if res.action == 2 {
                return Ok(res);
            }

            attempts -= 1;
            if attempts == 0 {
                return Err(anyhow!("connection failed"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_udp_tracker() {
        // TODO: find a torrent with a UDP announce url
    }
}
