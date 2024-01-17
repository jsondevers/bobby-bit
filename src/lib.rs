pub mod bitfield;

pub mod storage;
pub mod torrent;
pub mod utils;
pub mod tracker {
    pub mod http;
    pub mod udp;
}

pub mod peer {
    pub mod connection;
    pub mod message;
}

pub const DEBIAN_FILE: &str = "sample/debian.torrent"; // debian.torrent test torrent
