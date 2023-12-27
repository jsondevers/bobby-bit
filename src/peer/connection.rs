use crate::bitfield::BitField;
use crate::peer::handshake::Handshake;
use crate::peer::message::Message;
use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token};
use std::io::{Error, ErrorKind, Read, Write};
use std::net::SocketAddr;
use std::time::Duration;

pub struct Connection {
    pub stream: TcpStream,
    pub poll: Poll,
    pub token: Token,
    pub addr: SocketAddr,
    pub peer_id: [u8; 20],
    pub info_hash: [u8; 20],
    pub am_choking: bool,
    pub am_interested: bool,
    pub peer_choking: bool,
    pub peer_interested: bool,
    pub bitfield: BitField,
    pub downloaded: u32,
    pub uploaded: u32,
    pub left: u32,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("addr", &self.addr)
            .field("peer_id", &self.peer_id)
            .field("info_hash", &self.info_hash)
            .field("am_choking", &self.am_choking)
            .field("am_interested", &self.am_interested)
            .field("peer_choking", &self.peer_choking)
            .field("peer_interested", &self.peer_interested)
            .field("bitfield", &self.bitfield)
            .field("downloaded", &self.downloaded)
            .field("uploaded", &self.uploaded)
            .field("left", &self.left)
            .finish()
    }
}

impl Connection {
    /// Creates a new connection to a peer, initiates a handshake, and returns the connection
    pub fn new(
        peer: SocketAddr,
        info_hash: [u8; 20],
        peer_id: [u8; 20],
    ) -> Result<Connection, Error> {
        let poll = Poll::new()?;
        let token = Token(0);
        let addr = peer;
        let am_choking = true;
        let am_interested = false;
        let peer_choking = true;
        let peer_interested = false;
        let bitfield = BitField::new(vec![0; 0]);
        let downloaded = 0;
        let uploaded = 0;
        let left = 0;

        // connect to peer
        let mut stream = TcpStream::connect(peer)?;

        log::info!("Connected to {:?}", peer);

        // start polling for events, try to send handshake
        poll.registry()
            .register(&mut stream, token, Interest::READABLE | Interest::WRITABLE)?;
        let mut events = Events::with_capacity(1024);
        let mut connection = Connection {
            stream,
            poll,
            token,
            addr,
            peer_id,
            info_hash,
            am_choking,
            am_interested,
            peer_choking,
            peer_interested,
            bitfield,
            downloaded,
            uploaded,
            left,
        };

        let handshake = Handshake::new(info_hash, peer_id);
        let timeout = Duration::from_secs(3); // Adjust the timeout as needed

        loop {
            connection.poll.poll(&mut events, Some(timeout))?;
            for event in events.iter() {
                match event.token() {
                    Token(0) => {
                        if event.is_writable() {
                            // send handshake
                            let bytes = handshake.to_bytes();
                            connection.stream.write_all(&bytes)?;
                            log::debug!("Sent handshake to {:?}", peer);

                            // reregister stream to only listen for readable events
                            connection.poll.registry().reregister(
                                &mut connection.stream,
                                connection.token,
                                Interest::READABLE,
                            )?;
                        }
                        if event.is_readable() {
                            // read handshake
                            let mut buf = vec![0; 68];
                            connection.stream.read_exact(&mut buf)?;
                            let handshake = Handshake::from_bytes(&buf)?;

                            // check handshake
                            if handshake.check(&info_hash) {
                                log::info!("Handshake check passed");
                                // reregister stream to listen for both readable and writable events
                                connection.poll.registry().reregister(
                                    &mut connection.stream,
                                    connection.token,
                                    Interest::READABLE | Interest::WRITABLE,
                                )?;
                                return Ok(connection);
                            } else {
                                log::error!("Handshake check failed");
                                return Err(Error::new(
                                    ErrorKind::InvalidData,
                                    "Handshake check failed",
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Sends a message to the peer
    pub fn send(&mut self, message: Message) -> Result<(), Error> {
        let bytes = message.serialize();
        self.stream.write_all(&bytes)?;

        log::info!("Sent type {:?} message to {:?}", message.id(), self.addr);

        Ok(())
    }

    /// Receives a message from the peer
    pub fn recv(&mut self) -> Result<Message, Error> {
        let mut buf = vec![0; 4];
        self.stream.read_exact(&mut buf)?;
        // convert the length prefix to u32
        let len = u32::from_be_bytes(buf.try_into().unwrap());
        let mut buf = vec![0; len as usize];
        self.stream.read_exact(&mut buf)?;
        let message = Message::deserialize(&buf)?;

        Ok(message)
    }

    /// Closes the connection to the peer
    pub fn close(&mut self) -> Result<(), Error> {
        self.stream.shutdown(std::net::Shutdown::Both)?;
        Ok(())
    }

    /// Returns true if the connection is still open
    pub fn is_open(&self) -> bool {
        self.stream.peer_addr().is_ok()
    }

    /// Returns true if the connection is choked
    pub fn is_choked(&self) -> bool {
        self.peer_choking
    }

    /// Returns true if the connection is interested
    pub fn is_interested(&self) -> bool {
        self.peer_interested
    }
}

// todo: maybe implement Drop trait

/// spawns a thread that will create a connection to the peer and will be managed by the main thread using a channel
pub fn spawn_peer(peer: SocketAddr, info_hash: [u8; 20], peer_id: [u8; 20]) -> Result<(), Error> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let connection = Connection::new(peer, info_hash, peer_id).unwrap();
        tx.send(connection).unwrap();
    });
    let connection = rx.recv().unwrap();
    log::info!("Connection: {:?}", connection);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{generate_peer_id, get_peers};
    use crate::DEBIAN_FILE;

    #[test]
    fn test_peer_connect() {
        let peers = get_peers(&DEBIAN_FILE).unwrap();
        let torrent = crate::torrent::Torrent::from_file(DEBIAN_FILE).unwrap();
        let info_hash = torrent.info_hash();
        // try connect to all peers
        for peer in peers {
            let peer_id = generate_peer_id();
            let _connection = Connection::new(peer, info_hash, peer_id).unwrap();
            // println!("Connection: {:?}", connection);
        }
    }

    #[test]
    fn test_peer_send() {
        let peers = get_peers(&DEBIAN_FILE).unwrap();
        let torrent = crate::torrent::Torrent::from_file(DEBIAN_FILE).unwrap();
        let info_hash = torrent.info_hash();
        // try connect to all peers
        for peer in peers {
            let peer_id = generate_peer_id();
            let mut connection = Connection::new(peer, info_hash, peer_id).unwrap();
            let message = Message::KeepAlive;
            connection.send(message).unwrap();
        }
    }
}
