/*
src/pick.rs

this is the piece picker module that will be used to pick the next piece to download

*/

use crate::bitfield::BitField;
use crate::peer::connection::Connection;
use crate::storage::Downloader;
use crate::torrent::Torrent;
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
