use crate::bitfield::BitField;
use crate::peer::handshake::Handshake;
use crate::peer::message::Message;
use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token};
use std::io::{Error, ErrorKind, Read, Write};
use std::net::SocketAddr;
use std::time::Duration;

pub struct Connection {
    /// this id can be changed for different peers to avoid being blacklisted
    pub my_id: [u8; 20],
    pub stream: TcpStream,
    pub poll: Poll,
    pub token: Token,
    pub addr: SocketAddr,
    /// the peer id of the remote peer (recv in handshake)
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
        my_id: [u8; 20],
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
        let peer_id = [0; 20]; // will be set after handshake

        log::info!("Connected to {:?}", peer);

        // start polling for events, try to send handshake
        poll.registry()
            .register(&mut stream, token, Interest::READABLE | Interest::WRITABLE)?;
        let mut events = Events::with_capacity(1024);
        let mut connection = Connection {
            my_id,
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

        // TODO: ensure this doesn't block forever

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

                                // set peer id
                                connection.peer_id = handshake.peer_id;

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

// TODO: maybe implement Drop trait

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
    use crate::torrent::Torrent;
    use crate::utils::{find_peers, generate_peer_id};
    use crate::DEBIAN_FILE;

    const PORT: u16 = 6969;

    #[test]
    fn test_peer_connect() {
        // generate a random peer id
        let peer_id = generate_peer_id();
        // read the torrent file
        let torrent: Torrent = Torrent::from_file(DEBIAN_FILE).unwrap();

        let peers = find_peers(&torrent, peer_id, PORT);
        // try connect to all peers
        for peer in peers {
            let peer_id = generate_peer_id();
            // try to connect, but not all peers will accept the connection
            let connection = Connection::new(peer, torrent.info_hash(), peer_id);
            if let Ok(mut connection) = connection {
                log::info!("Connection: {:?}", connection);
                connection.close().unwrap();
            }
        }
    }
}
