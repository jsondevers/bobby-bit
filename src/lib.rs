pub mod torrent;
pub mod tracker {
    pub mod http;
    pub mod udp;
}
/// Peer module contains all the logic for the peer to peer connection
pub mod peer {
    pub mod connection;
    pub mod handshake;
    pub mod message;
}
pub mod bitfield;
pub mod picker;
pub mod storage;
pub mod utils;

pub const DEBIAN_FILE: &str = "sample/debian.torrent"; // debian.torrent test torrent
