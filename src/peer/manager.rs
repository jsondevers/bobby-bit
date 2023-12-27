use crate::peer::connection::Connection;
use crate::peer::message::Message;
use crate::torrent::Torrent;
use crate::tracker::http::HttpTracker;
use crate::utils::{generate_peer_id, get_peers};
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration; // atomic reference counter, mutex

pub struct PeerManager {}

impl PeerManager {}
