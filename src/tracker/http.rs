use crate::torrent::Torrent;
use anyhow::{anyhow, Context, Error, Result};
use log::{debug, error, info, trace, warn};
use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token};
use serde::{de, Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::time::Duration;
use url::Url;
use urlencoding::{encode, encode_binary};

#[derive(Debug, Serialize, Deserialize)]
pub struct AnnounceRequest {
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
    pub port: u16,
    pub uploaded: Option<u64>,
    pub downloaded: Option<u64>,
    pub left: Option<u64>,
    pub compact: Option<u8>,
    pub no_peer_id: Option<u8>,
    pub event: Option<String>,
    pub ip: Option<String>,
    pub numwant: Option<u64>,
    pub key: Option<String>,
    pub trackerid: Option<String>,
}

/// deserialize peers from compact representation for both ipv4 and ipv6
mod peers {
    use serde::de::{self, Deserialize, Deserializer, Visitor};
    use serde::ser::{Serialize, Serializer};
    use std::fmt;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

    #[derive(Debug, Clone)]
    pub struct Peers(pub Vec<SocketAddr>);
    struct PeersVisitor;

    impl<'de> Visitor<'de> for PeersVisitor {
        type Value = Peers;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("compact representation of peers")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let mut peers = Vec::new();
            let mut i = 0;
            while i < v.len() {
                if i + 6 <= v.len() {
                    let addr = Ipv4Addr::new(v[i], v[i + 1], v[i + 2], v[i + 3]);
                    let port = u16::from_be_bytes([v[i + 4], v[i + 5]]);
                    peers.push(SocketAddr::V4(SocketAddrV4::new(addr, port)));
                    i += 6;
                } else if i + 18 <= v.len() {
                    let addr = Ipv6Addr::from([
                        v[i],
                        v[i + 1],
                        v[i + 2],
                        v[i + 3],
                        v[i + 4],
                        v[i + 5],
                        v[i + 6],
                        v[i + 7],
                        v[i + 8],
                        v[i + 9],
                        v[i + 10],
                        v[i + 11],
                        v[i + 12],
                        v[i + 13],
                        v[i + 14],
                        v[i + 15],
                    ]);
                    let port = u16::from_be_bytes([v[i + 16], v[i + 17]]);
                    peers.push(SocketAddr::V6(SocketAddrV6::new(addr, port, 0, 0)));
                    i += 18;
                } else {
                    return Err(E::custom("Invalid peer length"));
                }
            }
            Ok(Peers(peers))
        }
    }

    impl<'de> Deserialize<'de> for Peers {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(PeersVisitor)
        }
    }

    impl Serialize for Peers {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut single_slice = Vec::new();
            for peer in &self.0 {
                match peer {
                    SocketAddr::V4(addr) => {
                        single_slice.extend(addr.ip().octets());
                        single_slice.extend(addr.port().to_be_bytes());
                    }
                    SocketAddr::V6(addr) => {
                        single_slice.extend(addr.ip().octets());
                        single_slice.extend(addr.port().to_be_bytes());
                    }
                }
            }
            serializer.serialize_bytes(&single_slice)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnnounceResponse {
    /// can still have a 200 ok, but this indicates a failure within the BT protocol request
    pub failure_reason: Option<String>,
    /// warning, similar to failure reason, but the response still gets processed normally
    pub warning_message: Option<String>,
    /// interval in seconds that the client should wait between sending regular requests to the tracker
    pub interval: u64,
    /// minimum announce interval. If present clients must not reannounce more frequently than this.
    pub min_interval: Option<u64>,
    /// string that the client should send back on its next announcements. If absent and a previous announce sent a tracker id, do not discard the old value; keep using it.
    pub tracker_id: Option<String>,
    /// number of peers with the entire file, i.e. seeders
    pub complete: Option<u64>,
    /// number of non-seeder peers, aka "leechers"
    pub incomplete: Option<u64>,
    /// list of peers
    pub peers: peers::Peers,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeRequest {
    pub info_hash: [u8; 20],
}

#[derive(Debug)]
pub struct ScrapeResponse {
    pub files: HashMap<Vec<u8>, ScrapeResponseFile>,
}

#[derive(Debug, Deserialize)]
pub struct ScrapeResponseFile {
    pub complete: u64,
    pub incomplete: u64,
    pub downloaded: u64,
}

struct ScrapeResponseVisitor;

impl<'de> serde::de::Visitor<'de> for ScrapeResponseVisitor {
    type Value = ScrapeResponse;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a bencoded dictionary")
    }

    fn visit_map<A>(self, mut map: A) -> Result<ScrapeResponse, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut files = HashMap::new();
        while let Some(key) = map.next_key::<Vec<u8>>()? {
            let file = map.next_value::<ScrapeResponseFile>()?;
            files.insert(key, file);
        }
        Ok(ScrapeResponse { files })
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<ScrapeResponse, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut files = HashMap::new();
        while let Some(key) = seq.next_element::<Vec<u8>>()? {
            let file = seq.next_element::<ScrapeResponseFile>()?.unwrap();
            files.insert(key, file);
        }
        Ok(ScrapeResponse { files })
    }
}

impl<'de> Deserialize<'de> for ScrapeResponse {
    fn deserialize<D>(deserializer: D) -> Result<ScrapeResponse, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ScrapeResponseVisitor)
    }
}

pub fn announce(
    torrent: &Torrent,
    peer_id: [u8; 20],
    port: u16,
    uploaded: Option<u64>,
    downloaded: Option<u64>,
    left: Option<u64>,
    compact: Option<u8>,
    no_peer_id: Option<u8>,
    event: Option<String>,
    ip: Option<String>,
    numwant: Option<u64>,
    key: Option<String>,
    trackerid: Option<String>,
) -> Result<AnnounceResponse> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);

    let announce_url = Url::parse(torrent.announce())?;
    let host = announce_url.host_str().ok_or(anyhow!("no host"))?;
    let port = announce_url.port().unwrap_or(6969);
    let addr = format!("{}:{}", host, port)
        .to_socket_addrs()?
        .next()
        .ok_or(anyhow!("Invalid address"))?;

    let mut stream = TcpStream::connect(addr)?;

    let query = format!(
        "?info_hash={}&peer_id={}&port={}&compact={}",
        encode_binary(&torrent.info_hash()),
        encode_binary(&peer_id),
        port,
        compact.unwrap_or(1)
    );
    let request = format!(
        "GET {}{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        announce_url.path(),
        query,
        host
    );

    let token = Token(1);
    poll.registry()
        .register(&mut stream, token, Interest::WRITABLE)?;

    loop {
        poll.poll(&mut events, Some(Duration::from_secs(5)))?;
        for event in events.iter() {
            match event.token() {
                token if token == token => {
                    if events.is_empty() {
                        return Err(anyhow!("Timeout waiting for tracker response"));
                    }
                    if event.is_writable() {
                        stream.write_all(request.as_bytes())?;
                        poll.registry()
                            .reregister(&mut stream, token, Interest::READABLE)?;
                    }
                    if event.is_readable() {
                        let mut buf = Vec::new();
                        stream.read_to_end(&mut buf)?;
                        let response = parse_announce_response(&buf)?;
                        return Ok(response);
                    }
                }
                _ => return Err(anyhow!("Unexpected token")),
            }
        }
    }
}

pub fn scrape(torrent: &Torrent) -> Result<ScrapeResponse> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);

    let announce_url = Url::parse(torrent.announce())?;
    // change /announce in the url to /scrape
    let mut scrape_url = announce_url.clone();
    let mut path = scrape_url.path().to_string();
    path = path.replace("/announce", "/scrape");
    scrape_url.set_path(&path);
    let host = scrape_url.host_str().ok_or(anyhow!("no host"))?;
    let port = scrape_url.port().unwrap_or(6969);
    let addr = format!("{}:{}", host, port)
        .to_socket_addrs()?
        .next()
        .ok_or(anyhow!("Invalid address"))?;

    let mut stream = TcpStream::connect(addr)?;

    let query = format!("?info_hash={}", encode_binary(&torrent.info_hash()));
    let request = format!(
        "GET {}{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        scrape_url.path(),
        query,
        host
    );

    println!("scrape request: {}", request);
    debug!("scrape request: {}", request);

    let token = Token(1);
    poll.registry()
        .register(&mut stream, token, Interest::WRITABLE)?;

    loop {
        poll.poll(&mut events, Some(Duration::from_secs(5)))?;
        for event in events.iter() {
            match event.token() {
                token if token == token => {
                    if events.is_empty() {
                        return Err(anyhow!("Timeout waiting for tracker response"));
                    }
                    if event.is_writable() {
                        stream.write_all(request.as_bytes())?;
                        poll.registry()
                            .reregister(&mut stream, token, Interest::READABLE)?;
                    }
                    if event.is_readable() {
                        let mut buf = Vec::new();
                        stream.read_to_end(&mut buf)?;
                        let response = parse_scrape_response(&buf)?;
                        return Ok(response);
                    }
                }
                _ => return Err(anyhow!("Unexpected token")),
            }
        }
    }
}

fn parse_announce_response(raw: &[u8]) -> Result<AnnounceResponse> {
    // try to put the headers in a string, read the first \r\n\r\n
    let mut header_end = 0;
    for i in 0..raw.len() - 3 {
        if raw[i] == b'\r' && raw[i + 1] == b'\n' && raw[i + 2] == b'\r' && raw[i + 3] == b'\n' {
            header_end = i + 4;
            break;
        }
    }

    if header_end == 0 {
        return Err(anyhow!("Invalid response"));
    }
    let headers = String::from_utf8(raw[..header_end].to_vec())?;
    debug!("Headers: {}", headers);
    println!("Headers: {}", headers);

    let mut body = Vec::new();
    body.extend_from_slice(&raw[header_end..]);

    debug!("Body: {:?}", body);

    let body = serde_bencode::from_bytes::<AnnounceResponse>(&body)?;
    Ok(body)
}

fn parse_scrape_response(raw: &[u8]) -> Result<ScrapeResponse> {
    // parse the scrape response
    let mut header_end = 0;
    for i in 0..raw.len() - 3 {
        if raw[i] == b'\r' && raw[i + 1] == b'\n' && raw[i + 2] == b'\r' && raw[i + 3] == b'\n' {
            header_end = i + 4;
            break;
        }
    }

    if header_end == 0 {
        return Err(anyhow!("Invalid response"));
    }

    let headers = String::from_utf8(raw[..header_end].to_vec())?;
    debug!("Headers: {}", headers);
    println!("Headers: {}", headers);

    // Directly use the slice of raw bytes after the header for deserialization
    let body = &raw[header_end..];
    debug!("Body: {:?}", body);
    println!("Body: {:?}", body);

    // try to put it in a string
    let body = String::from_utf8_lossy(body);
    debug!("Body: {}", body);
    println!("Body: {}", body);

    // Deserialize the bencoded response body directly from bytes
    let scrape_response = serde_bencode::from_bytes::<ScrapeResponse>(body.as_bytes())?;

    Ok(scrape_response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::generate_peer_id;
    use crate::DEBIAN_FILE;

    #[test]
    fn test_announce() {
        let torrent = Torrent::from_file(DEBIAN_FILE).unwrap();
        let peer_id = generate_peer_id();
        let port = 6881;
        let uploaded = None;
        let downloaded = None;
        let left = None;
        let compact = None;
        let no_peer_id = None;
        let event = None;
        let ip = None;
        let numwant = None;
        let key = None;
        let trackerid = None;

        let response = announce(
            &torrent, peer_id, port, uploaded, downloaded, left, compact, no_peer_id, event, ip,
            numwant, key, trackerid,
        )
        .unwrap();

        println!("{:?}", response);
    }

    #[test]
    fn test_scrape() {
        let torrent = Torrent::from_file(DEBIAN_FILE).unwrap();
        let response = scrape(&torrent).unwrap();
        println!("{:?}", response);
    }
}
