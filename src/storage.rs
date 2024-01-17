use crate::torrent::Torrent;
use anyhow::{bail, Result};
use sha1::{Digest, Sha1};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const BLOCK_SIZE: usize = 16384;

#[derive(Debug)]
pub struct Storage {
    file: File,
    piece_length: usize,
    total_size: usize,
    downloaded: usize,
    piece_hashes: Vec<[u8; 20]>,
}

impl Storage {
    pub fn new(torrent: &Torrent, download_path: &Path) -> Result<Self> {
        let total_size = torrent.length() as usize;
        let piece_length = torrent.piece_length() as usize;
        let piece_hashes = torrent.piece_hashes();

        let mut file_path = PathBuf::from(download_path);
        file_path.push(torrent.name());

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(file_path)?;

        file.set_len(total_size as u64)?;

        Ok(Storage {
            file,
            piece_length,
            total_size,
            downloaded: 0,
            piece_hashes,
        })
    }

    pub fn write_block(&mut self, piece_index: usize, offset: usize, data: &[u8]) -> Result<()> {
        let global_offset = self.piece_length * piece_index + offset;
        if global_offset + data.len() > self.total_size {
            bail!("Write exceeds file size");
        }

        self.file.seek(SeekFrom::Start(global_offset as u64))?;
        self.file.write_all(data)?;

        Ok(())
    }

    pub fn read_block(
        &mut self,
        piece_index: usize,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>> {
        let global_offset = self.piece_length * piece_index + offset;
        if global_offset + length > self.total_size {
            bail!("Read exceeds file size");
        }

        let mut buffer = vec![0u8; length];
        self.file.seek(SeekFrom::Start(global_offset as u64))?;
        self.file.read_exact(&mut buffer)?;

        Ok(buffer)
    }

    pub fn verify_piece(&mut self, piece_index: usize) -> Result<bool> {
        if piece_index >= self.piece_hashes.len() {
            bail!("Invalid piece index");
        }

        let start = self.piece_length * piece_index;
        let end = (start + self.piece_length).min(self.total_size);

        let mut hasher = Sha1::new();
        let mut buffer = vec![0u8; BLOCK_SIZE];

        self.file.seek(SeekFrom::Start(start as u64))?;
        let mut remaining = end - start;
        while remaining > 0 {
            let read_length = remaining.min(BLOCK_SIZE);
            self.file.read_exact(&mut buffer[..read_length])?;
            hasher.update(&buffer[..read_length]);
            remaining -= read_length;
        }

        let hash: [u8; 20] = hasher
            .finalize()
            .try_into()
            .expect("SHA1 hash should be 20 bytes");
        Ok(hash == self.piece_hashes[piece_index])
    }
    // Checks if all pieces have been successfully downloaded
    pub fn is_complete(&self) -> bool {
        // Assuming each piece is of equal length except possibly the last one
        let num_pieces = self.piece_hashes.len();
        let expected_downloaded = num_pieces * self.piece_length;
        let last_piece_length = self.total_size % self.piece_length;

        self.downloaded >= expected_downloaded - self.piece_length + last_piece_length
    }

    // Gets the download progress as a percentage
    pub fn progress(&self) -> f32 {
        (self.downloaded as f32 / self.total_size as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempfile;

    fn setup_test_storage() -> Storage {
        let temp_file = tempfile().unwrap();
        let piece_length = 1024; // Example piece length
        let total_size = piece_length * 10; // Example total size
        let piece_hashes = vec![[0u8; 20]; 10]; // Example piece hashes

        Storage {
            file: temp_file,
            piece_length,
            total_size,
            downloaded: 0,
            piece_hashes,
        }
    }

    #[test]
    fn test_storage_write_and_read_block() {
        let mut storage = setup_test_storage();
        let data = vec![1; 512];
        let piece_index = 0;
        let offset = 0;

        storage.write_block(piece_index, offset, &data).unwrap();
        let read_data = storage.read_block(piece_index, offset, data.len()).unwrap();

        assert_eq!(data, read_data);
    }

    #[test]
    fn test_storage_progress_and_completion() {
        let mut storage = setup_test_storage();
        let data = vec![1; storage.piece_length];

        for i in 0..storage.piece_hashes.len() {
            storage.write_block(i, 0, &data).unwrap();
            // Update downloaded size
            storage.downloaded += storage.piece_length;
        }

        assert!(storage.is_complete());
        assert_eq!(storage.progress(), 100.0);
    }
}
