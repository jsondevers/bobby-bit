use rand::Rng;

pub fn generate_peer_id() -> [u8; 20] {
    let mut peer_id = [0u8; 20];
    let mut rng = rand::thread_rng();
    rng.fill(&mut peer_id);
    peer_id
}
