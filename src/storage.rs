use crate::torrent::Torrent;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

const BLOCK_LEN: usize = 16384;

struct Piece {
    index: usize,
    length: usize,
    blocks: Vec<Block>,
}

impl Piece {
    fn is_complete(&self) -> bool {
        self.blocks
            .iter()
            .all(|block| block.data.len() == BLOCK_LEN)
    }
}

struct Block {
    offset: usize,
    data: Vec<u8>,
}

pub struct Downloader {
    torrent: Torrent,
    file: Arc<Mutex<File>>,
    pieces: Arc<Mutex<HashMap<usize, Piece>>>,
}

impl Downloader {
    pub fn new(torrent_path: &str, download_path: &str) -> io::Result<Self> {
        let torrent =
            Torrent::from_path(Path::new(torrent_path)).expect("Failed to load torrent file");

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(download_path)?;

        Ok(Self {
            torrent,
            file: Arc::new(Mutex::new(file)),
            pieces: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn download_piece(&self, piece_index: usize) {
        let piece_length = self.torrent.piece_length() as usize;
        let block_data = vec![0; BLOCK_LEN]; // Placeholder for block data
        let num_blocks = (piece_length + BLOCK_LEN - 1) / BLOCK_LEN;

        let blocks = (0..num_blocks)
            .map(|i| Block {
                offset: i * BLOCK_LEN,
                data: block_data.clone(),
            })
            .collect();

        let mut pieces = self.pieces.lock().unwrap();
        pieces.insert(
            piece_index,
            Piece {
                index: piece_index,
                length: piece_length,
                blocks,
            },
        );
    }

    pub fn write_to_disk(&self) -> io::Result<()> {
        let pieces = self.pieces.lock().unwrap();
        let mut file = self.file.lock().unwrap();

        for piece in pieces.values() {
            for block in &piece.blocks {
                let file_offset = piece.index * self.torrent.piece_length() as usize + block.offset;
                file.seek(SeekFrom::Start(file_offset as u64))?;
                file.write_all(&block.data)?;
            }
        }
        Ok(())
    }
}

pub fn spawn_download_threads(
    torrent_path: &str,
    download_path: &str,
    num_threads: usize,
) -> io::Result<()> {
    let downloader = Arc::new(Downloader::new(torrent_path, download_path)?);

    let mut handles = vec![];
    for i in 0..num_threads {
        let downloader_clone = Arc::clone(&downloader);
        let handle = thread::spawn(move || {
            downloader_clone.download_piece(i); // Each thread downloads a different piece
            downloader_clone
                .write_to_disk()
                .expect("Failed to write to disk");
        });
        handles.push(handle);
    }

    // Join all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::remove_file;

    #[test]
    fn test_storage_downloader() {
        let torrent_path = "sample/debian.torrent";
        let download_path = "sample/debian.iso";
        let num_threads = 4;

        // Remove file if it already exists
        if Path::new(download_path).exists() {
            remove_file(download_path).expect("Failed to remove file");
        }

        spawn_download_threads(torrent_path, download_path, num_threads)
            .expect("Failed to spawn download threads");

        assert!(Path::new(download_path).exists());
    }
}
