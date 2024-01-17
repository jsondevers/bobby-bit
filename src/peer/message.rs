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

    pub fn check(&self, info_hash: &[u8]) -> bool {
        self.info_hash == info_hash && self.pstr == "BitTorrent protocol" && self.reserved == [0; 8]
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),
    Bitfield(Vec<u8>),
    Request(u32, u32, u32),
    Piece(u32, u32, Vec<u8>),
    Cancel(u32, u32, u32),
    Port(u16),
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        match self {
            Message::KeepAlive => vec![0, 0, 0, 0],
            Message::Choke => vec![0, 0, 0, 1, 0],
            Message::Unchoke => vec![0, 0, 0, 1, 1],
            Message::Interested => vec![0, 0, 0, 1, 2],
            Message::NotInterested => vec![0, 0, 0, 1, 3],
            Message::Have(index) => {
                let mut msg = vec![0, 0, 0, 5, 4];
                msg.extend_from_slice(&index.to_be_bytes());
                msg
            }
            Message::Bitfield(bitfield) => {
                let mut msg = vec![0, 0, 0, 1 + bitfield.len() as u8, 5];
                msg.extend_from_slice(bitfield);
                msg
            }
            Message::Request(index, begin, length) => {
                let mut msg = vec![0, 0, 0, 13, 6];
                msg.extend_from_slice(&index.to_be_bytes());
                msg.extend_from_slice(&begin.to_be_bytes());
                msg.extend_from_slice(&length.to_be_bytes());
                msg
            }
            Message::Piece(index, begin, block) => {
                let mut msg = vec![0, 0, 0, 9 + block.len() as u8, 7];
                msg.extend_from_slice(&index.to_be_bytes());
                msg.extend_from_slice(&begin.to_be_bytes());
                msg.extend_from_slice(block);
                msg
            }
            Message::Cancel(index, begin, length) => {
                let mut msg = vec![0, 0, 0, 13, 8];
                msg.extend_from_slice(&index.to_be_bytes());
                msg.extend_from_slice(&begin.to_be_bytes());
                msg.extend_from_slice(&length.to_be_bytes());
                msg
            }
            Message::Port(port) => {
                let mut msg = vec![0, 0, 0, 3, 9];
                msg.extend_from_slice(&port.to_be_bytes());
                msg
            }
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<Message, Error> {
        // first 4 bytes are the length prefix and if they are 0, it's a keep-alive message
        if data.len() == 4 && data == [0, 0, 0, 0] {
            return Ok(Message::KeepAlive);
        }

        // only keep-alive message will be 4 bytes long
        if data.len() < 5 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Message too short to be valid",
            ));
        }

        let id = data[4];
        let msg = match id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => {
                if data.len() != 5 {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Have message should be 5 bytes long",
                    ));
                }
                let mut index = [0; 4];
                index.copy_from_slice(&data[1..5]);
                Message::Have(u32::from_be_bytes(index))
            }
            5 => {
                if data.len() < 6 {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Bitfield message should be at least 6 bytes long",
                    ));
                }
                Message::Bitfield(data[1..].to_vec())
            }
            6 => {
                if data.len() != 13 {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Request message should be 13 bytes long",
                    ));
                }
                let mut index = [0; 4];
                index.copy_from_slice(&data[1..5]);
                let mut begin = [0; 4];
                begin.copy_from_slice(&data[5..9]);
                let mut length = [0; 4];
                length.copy_from_slice(&data[9..13]);
                Message::Request(
                    u32::from_be_bytes(index),
                    u32::from_be_bytes(begin),
                    u32::from_be_bytes(length),
                )
            }
            7 => {
                if data.len() < 9 {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Piece message should be at least 9 bytes long",
                    ));
                }
                let mut index = [0; 4];
                index.copy_from_slice(&data[1..5]);
                let mut begin = [0; 4];
                begin.copy_from_slice(&data[5..9]);
                Message::Piece(
                    u32::from_be_bytes(index),
                    u32::from_be_bytes(begin),
                    data[9..].to_vec(),
                )
            }
            8 => {
                if data.len() != 13 {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Cancel message should be 13 bytes long",
                    ));
                }
                let mut index = [0; 4];
                index.copy_from_slice(&data[1..5]);
                let mut begin = [0; 4];
                begin.copy_from_slice(&data[5..9]);
                let mut length = [0; 4];
                length.copy_from_slice(&data[9..13]);
                Message::Cancel(
                    u32::from_be_bytes(index),
                    u32::from_be_bytes(begin),
                    u32::from_be_bytes(length),
                )
            }
            9 => {
                if data.len() != 3 {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Port message should be 3 bytes long",
                    ));
                }
                let mut port = [0; 2];
                port.copy_from_slice(&data[1..3]);
                Message::Port(u16::from_be_bytes(port))
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Unknown message id {}", id),
                ))
            }
        };
        Ok(msg)
    }

    pub fn id(&self) -> u8 {
        match self {
            Message::KeepAlive => 0,
            Message::Choke => 0,
            Message::Unchoke => 1,
            Message::Interested => 2,
            Message::NotInterested => 3,
            Message::Have(_) => 4,
            Message::Bitfield(_) => 5,
            Message::Request(_, _, _) => 6,
            Message::Piece(_, _, _) => 7,
            Message::Cancel(_, _, _) => 8,
            Message::Port(_) => 9,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Message::KeepAlive => 0,
            Message::Choke => 1,
            Message::Unchoke => 1,
            Message::Interested => 1,
            Message::NotInterested => 1,
            Message::Have(_) => 5,
            Message::Bitfield(bitfield) => 1 + bitfield.len(),
            Message::Request(_, _, _) => 13,
            Message::Piece(_, _, block) => 9 + block.len(),
            Message::Cancel(_, _, _) => 13,
            Message::Port(_) => 3,
        }
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

    #[test]
    fn test_message_keep_alive() {
        let msg = Message::KeepAlive;
        let bytes = msg.serialize();
        assert_eq!(bytes, vec![0, 0, 0, 0]);
        let msg = Message::deserialize(&bytes).unwrap();
        assert_eq!(msg, Message::KeepAlive);
    }

    #[test]
    fn test_message_choke() {
        let msg = Message::Choke;
        let bytes = msg.serialize();
        assert_eq!(bytes, vec![0, 0, 0, 1, 0]);
        let msg = Message::deserialize(&bytes).unwrap();
        assert_eq!(msg, Message::Choke);
    }
}
