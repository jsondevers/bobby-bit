use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_bencode::{from_bytes, to_bytes};
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};
use std::io::Read;

#[derive(Debug, Deserialize, Serialize)]
pub struct Node(String, i64);

/// a file can be single xor multi file torrent, if length is None, it's a multi file torrent, else it's a single file torrent
#[derive(Debug, Deserialize, Serialize)]
pub struct File {
    /// The length of the file in bytes (integer)
    pub path: Vec<String>,
    /// The length of the file in bytes (integer)
    pub length: i64,
    /// (optional) a 32-character hexadecimal string corresponding to the MD5 sum of the file. This is not used by BitTorrent at all, but it is included by some programs for greater compatibility.
    #[serde(default)]
    pub md5sum: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
pub struct Info {
    pub name: String,
    /// string consisting of the concatenation of all 20-byte SHA1 hash values, one per piece (byte string, i.e. not urlencoded)
    pub pieces: ByteBuf,
    /// number of bytes in each piece (integer)
    #[serde(rename = "piece length")]
    pub piece_length: i64,
    #[serde(default)]
    pub md5sum: Option<String>,
    #[serde(default)]
    pub length: Option<i64>,
    #[serde(default)]
    pub files: Option<Vec<File>>,
    #[serde(default)]
    pub private: Option<u8>,
    #[serde(default)]
    pub path: Option<Vec<String>>,
    #[serde(default)]
    #[serde(rename = "root hash")]
    pub root_hash: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Torrent {
    pub info: Info,
    #[serde(default)]
    /// The announce URL of the tracker (string)
    announce: Option<String>,
    /// (optional) this is an extension to the official specification, offering backwards-compatibility. (list of lists of strings).
    #[serde(default)]
    nodes: Option<Vec<Node>>,
    #[serde(default)]
    encoding: Option<String>,
    /// (optional) the creation time of the torrent, in standard UNIX epoch format (integer, seconds since 1-Jan-1970 00:00:00 UTC)
    #[serde(default)]
    httpseeds: Option<Vec<String>>,
    /// (optional) free-form textual comments of the author (string)
    #[serde(default)]
    #[serde(rename = "announce-list")]
    announce_list: Option<Vec<Vec<String>>>,
    /// (optional) name and version of the program used to create the .torrent (string)
    #[serde(default)]
    #[serde(rename = "creation date")]
    creation_date: Option<i64>,
    /// (optional) the string encoding format used to generate the pieces part of the info dictionary in the .torrent metafile (string)
    #[serde(rename = "comment")]
    comment: Option<String>,
    #[serde(default)]
    #[serde(rename = "created by")]
    created_by: Option<String>,
}

impl Torrent {
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        from_bytes(bytes).context("failed to deserialize torrent")
    }

    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Self::from_bytes(&buf)
    }

    pub fn from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Self::from_bytes(&buf)
    }

    pub fn info_hash(&self) -> [u8; 20] {
        let bytes = to_bytes(&self.info).unwrap();
        let mut hasher = Sha1::new();
        hasher.update(bytes);
        hasher.finalize().into()
    }

    pub fn announce(&self) -> &str {
        self.announce.as_ref().unwrap()
    }

    // if length is None, it's a multi file torrent, else it's a single file torrent
    pub fn length(&self) -> i64 {
        if let Some(length) = self.info.length {
            length
        } else {
            self.info
                .files
                .as_ref()
                .unwrap()
                .iter()
                .map(|f| f.length)
                .sum()
        }
    }

    pub fn piece_length(&self) -> i64 {
        self.info.piece_length
    }

    pub fn piece_hashes(&self) -> Vec<[u8; 20]> {
        self.info
            .pieces
            .chunks(20)
            .map(|chunk| {
                let mut array = [0u8; 20];
                array.copy_from_slice(chunk);
                array
            })
            .collect()
    }

    pub fn name(&self) -> &str {
        &self.info.name
    }

    /// Returns the announce list as a vector of SocketAddr
    pub fn announce_list(&self) -> Vec<std::net::SocketAddr> {
        let mut addrs = Vec::new();
        if let Some(announce_list) = &self.announce_list {
            for urls in announce_list {
                for url in urls {
                    if let Ok(addr) = url.parse::<std::net::SocketAddr>() {
                        addrs.push(addr);
                    }
                }
            }
        }
        addrs
    }

    pub fn has_udp_trackers(&self) -> bool {
        self.announce_list()
            .iter()
            .any(|addr| addr.to_string().contains("udp"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEBIAN_FILE;

    #[test]
    fn test_torrent_announce() {
        let mut file = std::fs::File::open(DEBIAN_FILE).unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        let torrent: Torrent = from_bytes(&buf).unwrap();
        assert_eq!(
            torrent.announce(),
            "http://bttracker.debian.org:6969/announce"
        );
    }
}
