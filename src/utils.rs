use crate::torrent::Torrent;
use crate::tracker::http::HttpTracker;

use rand::Rng;

pub fn generate_peer_id() -> [u8; 20] {
    let mut peer_id = [0u8; 20];
    let mut rng = rand::thread_rng();
    rng.fill(&mut peer_id);
    peer_id
}

pub fn get_peers(torrent_file: &str) -> Result<Vec<std::net::SocketAddr>, anyhow::Error> {
    let torrent = Torrent::from_file(torrent_file).unwrap();
    let peer_id = generate_peer_id();
    let port = 6969;
    let compact = Some(1);

    let mut client = HttpTracker::new().unwrap();

    let response = client.announce(&torrent, peer_id, port, compact).unwrap();

    let peers = response.peers();

    log::debug!("Peers: {:?}", peers);

    Ok(peers)
}

pub fn get_pieces(torrent_file: &str) -> Result<Vec<[u8; 20]>, anyhow::Error> {
    let torrent = Torrent::from_file(torrent_file).unwrap();
    let pieces = torrent.piece_hashes();
    Ok(pieces)
}
