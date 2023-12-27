use crate::peer::bitfield::BitField;
use anyhow::Result;
use sha1::{Digest, Sha1};
use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

const BLOCK_SIZE: usize = 1 << 14; // 2^14 bytes
const PIECE_SIZE: usize = 1 << 18; // 2^18 bytes

/// a block is what we request from a peer. a piece is a collection of blocks.
#[derive(Debug)]
struct Block {
    /// integer specifying the zero-based piece index
    index: usize,
    /// integer specifying the zero-based byte offset within the piece
    begin: usize,
    /// integer specifying the requested length.
    length: usize,
}

/// a piece is a collection of blocks. a piece is what we write to disk.
#[derive(Debug)]
struct Piece {
    blocks: Vec<Block>,
    index: usize,
    length: usize,
    hash: [u8; 20],
    data: Vec<u8>,
}

impl Piece {
    fn new(index: usize, length: usize, hash: [u8; 20]) -> Piece {
        let blocks = (0..length / BLOCK_SIZE)
            .map(|i| Block {
                index,
                begin: i * BLOCK_SIZE,
                length: BLOCK_SIZE,
            })
            .collect();
        Piece {
            blocks,
            index,
            length,
            hash,
            data: vec![0; length],
        }
    }

    fn is_complete(&self) -> bool {
        self.data == vec![0; self.length]
    }

    fn is_valid(&self) -> bool {
        let mut hasher = Sha1::new();
        hasher.update(&self.data);
        let result = hasher.finalize();
        result[..] == self.hash[..]
    }

    // takes a vector of [u8; 20] hashes and puts them into a vector of pieces
    fn from_hashes(hashes: Vec<[u8; 20]>, piece_length: usize) -> Vec<Piece> {
        hashes
            .iter()
            .enumerate()
            .map(|(i, hash)| Piece::new(i, piece_length, *hash))
            .collect()
    }
}

/// a storage is a collection of pieces that make up the whole file.
#[derive(Debug)]
pub struct Storage {
    /// the path to the file we are downloading
    path: PathBuf,
    /// the length of the file we are downloading
    length: usize,
    /// the size of each piece
    piece_length: usize,
    /// the pieces that make up the file we are downloading
    pieces: Vec<Piece>,
    /// the bitfield that keeps track of which pieces we have downloaded
    bitfield: BitField,
    /// the number of pieces we have downloaded
    downloaded: usize,
    /// the number of pieces we have verified
    verified: usize,
    /// the channel to communicate with the peer threads
    tx: Sender<usize>,
    /// the channel to receive messages from the peer threads
    rx: Receiver<usize>,
}

impl Storage {
    /// creates a new storage
    pub fn new(torrent_path: &std::path::Path) -> Result<Storage> {
        let torrent = crate::torrent::Torrent::from_path(torrent_path)?;
        let path = PathBuf::from(torrent.info.name.clone());
        let length = torrent.length() as usize;
        let piece_length = torrent.piece_length() as usize;
        let pieces = torrent.piece_hashes();
        let pieces = Piece::from_hashes(pieces, piece_length as usize);
        let bitfield = BitField::new(vec![0; pieces.len()]);
        let (tx, rx) = mpsc::channel();

        Ok(Storage {
            path,
            length,
            piece_length,
            pieces,
            bitfield,
            downloaded: 0,
            verified: 0,
            tx,
            rx,
        })
    }
    /// returns the path to the file we are downloading
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    /// returns the length of the file we are downloading
    pub fn length(&self) -> usize {
        self.length
    }

    /// returns the size of each piece
    pub fn piece_length(&self) -> usize {
        self.piece_length
    }

    /// returns the number of pieces we have downloaded
    pub fn downloaded(&self) -> usize {
        self.downloaded
    }

    /// returns the number of pieces we have verified
    pub fn verified(&self) -> usize {
        self.verified
    }

    /// returns the bitfield that keeps track of which pieces we have downloaded
    pub fn bitfield(&self) -> &BitField {
        &self.bitfield
    }

    /// returns the number of pieces we have left to download
    pub fn left(&self) -> usize {
        self.pieces.len() - self.downloaded
    }
}

/// the storage thread is responsible for writing the downloaded pieces to disk, and reading them back when needed. we are to download pieces as needed, and to communicate with our peer threads to orchestrate what pieces we should be requesting from which peers.
pub fn spawn_storage(
    path: PathBuf,
    length: usize,
    piece_length: usize,
) -> Result<Arc<Mutex<Storage>>> {
    let storage = Arc::new(Mutex::new(Storage {
        path,
        length,
        piece_length,
        pieces: Vec::new(),
        bitfield: BitField::new(Vec::new()),
        downloaded: 0,
        verified: 0,
        tx: mpsc::channel().0,
        rx: mpsc::channel().1,
    }));

    let storage_clone = storage.clone();
    thread::spawn(move || {
        let mut storage = storage_clone.lock().unwrap();
        loop {
            let index = storage.rx.recv().unwrap();
            let piece = &storage.pieces[index];
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&storage.path)
                .unwrap();
            file.seek(SeekFrom::Start(piece.index as u64 * piece.length as u64))
                .unwrap();
            file.write_all(&piece.data).unwrap();
            storage.downloaded += 1;
            if storage.downloaded == storage.pieces.len() {
                break;
            }
        }
    });
    Ok(storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEBIAN_FILE;

    #[test]
    fn test_storage_new() {
        let storage = Storage::new(std::path::Path::new(DEBIAN_FILE)).unwrap();
    }
}
