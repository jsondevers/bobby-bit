use anyhow::{anyhow, Context, Error, Result};
use serde::{Deserialize, Serialize};
use serde_bencode::{from_bytes, to_bytes};
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};
use std::io::Read;

#[derive(Debug, Deserialize, Serialize)]
struct Node(String, i64);

/// a file can be single xor multi file torrent, if length is None, it's a multi file torrent, else it's a single file torrent
#[derive(Debug, Deserialize, Serialize)]
struct File {
    /// The length of the file in bytes (integer)
    path: Vec<String>,
    /// The length of the file in bytes (integer)
    length: i64,
    /// (optional) a 32-character hexadecimal string corresponding to the MD5 sum of the file. This is not used by BitTorrent at all, but it is included by some programs for greater compatibility.
    #[serde(default)]
    md5sum: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
struct Info {
    pub name: String,
    pub pieces: ByteBuf,
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
    info: Info,
    #[serde(default)]
    /// The announce URL of the tracker (string)
    announce: Option<String>,
    /// (optional) this is an extention to the official specification, offering backwards-compatibility. (list of lists of strings).
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
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        from_bytes(bytes).context("failed to deserialize torrent")
    }

    pub fn from_file(path: &str) -> Result<Self> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEBIAN_FILE;

    #[test]
    fn test_torrent() {
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
