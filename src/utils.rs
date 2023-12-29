use crate::torrent::Torrent;
use crate::tracker::http::HttpTracker;
use crate::tracker::udp::UdpTracker;
use rand::Rng;

pub fn generate_peer_id() -> [u8; 20] {
    let mut peer_id = [0u8; 20];
    let mut rng = rand::thread_rng();
    rng.fill(&mut peer_id);
    peer_id
}

pub fn get_pieces(torrent_file: &str) -> Result<Vec<[u8; 20]>, anyhow::Error> {
    let torrent = Torrent::from_file(torrent_file).unwrap();
    let pieces = torrent.piece_hashes();
    Ok(pieces)
}

pub fn find_peers(torrent: &Torrent, peer_id: [u8; 20], port: u16) -> Vec<std::net::SocketAddr> {
    // check for udp trackers
    if torrent.has_udp_trackers() {
        log::info!("udp trackers found");
        let udp_tracker = UdpTracker::new();
        let announce_list = torrent.announce_list();
        let tracker_response = udp_tracker
            .expect("udp tracker")
            .announce(announce_list[0], &torrent)
            .unwrap();
        log::info!("tracker response: {:?}", tracker_response);

        tracker_response.peers()
    } else {
        log::info!("no udp trackers found, using http");
        let http_tracker = HttpTracker::new();
        let tracker_response = http_tracker
            .expect("http tracker")
            .announce(&torrent, peer_id, port, Some(1))
            .unwrap();
        log::info!("tracker response: {:?}", tracker_response);

        tracker_response.peers()
    }
}
