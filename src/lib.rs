pub mod torrent;
pub mod tracker {
    pub mod http;
    pub mod udp;
}
pub mod utils;

pub const DEBIAN_FILE: &str = "sample/debian.torrent"; // debian.torrent test torrent
