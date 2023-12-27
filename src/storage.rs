/*
src/storage.rs

This file contains the logic for the storage of the torrent file. It is responsible for the creation of the file and the writing of the pieces. It also contains the logic for the verification of the pieces.

note the difference between the torrent file and the torrent pieces. The torrent file is the file that contains the metadata of the torrent. The torrent pieces are the pieces that are downloaded from the peers. note the difference between a piece and a block. A piece is a part of the torrent file. A block is a part of a piece. A piece is made up of blocks. A block is the smallest unit of a piece. A piece is the smallest unit of a torrent file.

The storage module is responsible for the creation of the file and the writing of the pieces. It also contains the logic for the verification of the pieces.
*/

use crate::torrent::Torrent;
use sha1::{Digest, Sha1};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct Storage {
    file: File,
    path: String,
    length: i64,
    piece_length: i64,
    pieces: Vec<[u8; 20]>,
    completed: Vec<bool>,
    completed_pieces: u64,
    completed_blocks: u64,
    blocks: Vec<Vec<u8>>,
    blocks_written: Vec<bool>,
    blocks_written_count: u64,
    blocks_written_mutex: Arc<Mutex<()>>,
}

impl Storage {
    pub fn new(torrent: &Torrent, path: &str) -> io::Result<Storage> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        let length = torrent.length();
        let piece_length = torrent.piece_length();
        let pieces = torrent.piece_hashes();
        let completed = vec![false; pieces.len()];
        let completed_pieces = 0;
        let completed_blocks = 0;
        let blocks = vec![vec![0; piece_length as usize]; pieces.len()];
        let blocks_written = vec![false; pieces.len() * (piece_length / (16 * 1024)) as usize];
        let blocks_written_count = 0;
        let blocks_written_mutex = Arc::new(Mutex::new(()));

        Ok(Storage {
            file,
            path: path.to_string(),
            length,
            piece_length,
            pieces,
            completed,
            completed_pieces,
            completed_blocks,
            blocks,
            blocks_written,
            blocks_written_count,
            blocks_written_mutex,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn length(&self) -> i64 {
        self.length
    }

    pub fn piece_length(&self) -> i64 {
        self.piece_length
    }

    pub fn pieces(&self) -> &Vec<[u8; 20]> {
        &self.pieces
    }

    pub fn completed(&self) -> &Vec<bool> {
        &self.completed
    }

    pub fn completed_pieces(&self) -> u64 {
        self.completed_pieces
    }

    pub fn completed_blocks(&self) -> u64 {
        self.completed_blocks
    }

    pub fn blocks(&self) -> &Vec<Vec<u8>> {
        &self.blocks
    }

    pub fn blocks_written(&self) -> &Vec<bool> {
        &self.blocks_written
    }

    pub fn blocks_written_count(&self) -> u64 {
        self.blocks_written_count
    }

    pub fn write_block(&mut self, index: u64, begin: u64, block: &[u8]) -> io::Result<()> {
        let piece_index = index as usize;
        let block_index = begin as usize / (16 * 1024);
        let block_offset = begin as usize % (16 * 1024);
        let block_end = block_offset + block.len();
        let block_end = if block_end > 16 * 1024 {
            16 * 1024
        } else {
            block_end
        };
        let block_length = block_end - block_offset;

        let mut blocks_written_mutex = self.blocks_written_mutex.lock().unwrap();

        if !self.blocks_written
            [piece_index * (self.piece_length / (16 * 1024)) as usize + block_index]
        {
            self.blocks[piece_index][block_offset..block_end]
                .copy_from_slice(&block[..block_length]);
            self.blocks_written
                [piece_index * (self.piece_length / (16 * 1024)) as usize + block_index] = true;
            self.blocks_written_count += 1;
        }

        if self.blocks_written_count
            == self.pieces.len() as u64 * (self.piece_length / (16 * 1024)) as u64
        {
            let mut bytes = Vec::new();
            for block in &self.blocks {
                bytes.extend_from_slice(block);
            }
            let mut hasher = Sha1::new();
            hasher.update(&bytes);
            let hash = hasher.finalize();

            for (i, piece) in self.pieces.iter().enumerate() {
                if hash[i] != piece[i] {
                    self.blocks_written_mutex = Arc::new(Mutex::new(()));
                    return Ok(());
                }
            }

            self.file.write_all(&bytes)?;
            self.completed_pieces = self.pieces.len() as u64;
            self.completed_blocks = self.blocks_written_count;
            self.blocks_written_mutex = Arc::new(Mutex::new(()));
            return Ok(());
        }

        self.blocks_written_mutex = Arc::new(Mutex::new(()));
        Ok(())
    }

    pub fn write_piece(&mut self, index: u64, piece: &[u8]) -> io::Result<()> {
        let piece_index = index as usize;
        let piece_offset = piece_index * self.piece_length as usize;
        let piece_end = piece_offset + piece.len();
        let piece_end = if piece_end > self.length as usize {
            self.length as usize
        } else {
            piece_end
        };
        let piece_length = piece_end - piece_offset;

        let mut blocks_written_mutex = self.blocks_written_mutex.lock().unwrap();

        if !self.completed[piece_index] {
            self.blocks[piece_index][..piece_length].copy_from_slice(&piece[..piece_length]);
            self.blocks_written[piece_index * (self.piece_length / (16 * 1024)) as usize] = true;
            self.blocks_written_count += 1;
        }

        if self.blocks_written_count
            == self.pieces.len() as u64 * (self.piece_length / (16 * 1024)) as u64
        {
            let mut bytes = Vec::new();
            for block in &self.blocks {
                bytes.extend_from_slice(block);
            }
            let mut hasher = Sha1::new();
            hasher.update(&bytes);
            let hash = hasher.finalize();

            for (i, piece) in self.pieces.iter().enumerate() {
                if hash[i] != piece[i] {
                    self.blocks_written_mutex = Arc::new(Mutex::new(()));
                    return Ok(());
                }
            }

            self.file.write_all(&bytes)?;
            self.completed_pieces = self.pieces.len() as u64;
            self.completed_blocks = self.blocks_written_count;
            self.blocks_written_mutex = Arc::new(Mutex::new(()));
            return Ok(());
        }

        self.blocks_written_mutex = Arc::new(Mutex::new(()));
        Ok(())
    }

    pub fn verify(&mut self) -> io::Result<()> {
        let mut bytes = Vec::new();
        for block in &self.blocks {
            bytes.extend_from_slice(block);
        }
        let mut hasher = Sha1::new();
        hasher.update(&bytes);
        let hash = hasher.finalize();

        for (i, piece) in self.pieces.iter().enumerate() {
            if hash[i] != piece[i] {
                return Ok(());
            }
        }

        self.file.write_all(&bytes)?;
        self.completed_pieces = self.pieces.len() as u64;
        self.completed_blocks = self.blocks_written_count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::torrent::Torrent;
    use crate::DEBIAN_FILE;
    use std::fs::remove_file;


    #[test]
    fn test_storage() {
        let torrent = Torrent::from_file(DEBIAN_FILE).unwrap();