use std::io::{Error, ErrorKind};

#[derive(Debug, PartialEq, Clone)]
pub struct Handshake {
    /// string identifier of the protocol (19 bytes), e.g. "BitTorrent protocol"
    pub pstr: String,
    /// 8 reserved bytes. All current implementations use all zeroes.
    pub reserved: [u8; 8],
    pub info_hash: [u8; 20],
    /// 20-byte string used as a unique ID for the client.
    pub peer_id: [u8; 20],
}

impl Handshake {
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Handshake {
        Handshake {
            pstr: "BitTorrent protocol".to_string(),
            reserved: [0; 8],
            info_hash,
            peer_id,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Handshake, Error> {
        if bytes.len() < 68 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Handshake message should be at least 68 bytes long",
            ));
        }
        if bytes[0] != 19 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Handshake message should start with 19",
            ));
        }

        let mut info_hash = [0; 20];
        info_hash.copy_from_slice(&bytes[28..48]);
        let mut peer_id = [0; 20];
        peer_id.copy_from_slice(&bytes[48..68]);

        Ok(Handshake {
            pstr: String::from_utf8(bytes[1..20].to_vec()).unwrap(),
            reserved: [0; 8],
            info_hash,
            peer_id,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0; 68];
        bytes[0] = 19;
        bytes[1..20].copy_from_slice(self.pstr.as_bytes());
        bytes[28..48].copy_from_slice(&self.info_hash);
        bytes[48..68].copy_from_slice(&self.peer_id);
        bytes
    }

    // TODO: look more into this
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        let bytes = self as *mut Self as *mut [u8; std::mem::size_of::<Self>()];
        // Safety: Self is a POD with repr(c) and repr(packed)
        let bytes: &mut [u8; std::mem::size_of::<Self>()] = unsafe { &mut *bytes };
        bytes
    }

    pub fn check(&self, info_hash: &[u8]) -> bool {
        self.info_hash == info_hash && self.pstr == "BitTorrent protocol" && self.reserved == [0; 8]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEBIAN_FILE;

    #[test]
    fn test_handshake() {
        let torrent = crate::torrent::Torrent::from_file(DEBIAN_FILE).unwrap();
        let peer_id = crate::utils::generate_peer_id();
        let handshake = Handshake::new(torrent.info_hash(), peer_id);
        let bytes = handshake.to_bytes();
        let handshake2 = Handshake::from_bytes(&bytes).unwrap();
        assert_eq!(handshake, handshake2);
    }
}
