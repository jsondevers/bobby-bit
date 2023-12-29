use bobby_bit::torrent::{self, Torrent};
use bobby_bit::utils;
use clap::Parser;

/*
TODO:

- by default we should try to find the announce_list and see if there is a udp tracker, use a udp client, otherwise just use http
- we should always use compact mode for the tracker request to save bandwidth

*/

#[derive(Parser, Debug)]
struct Cli {
    #[clap(short, long, help = "path to *.torrent file")]
    file: String,
    #[clap(short, long, default_value = "6969")]
    port: u16,
    #[clap(short, long, help = "path where to save the downloaded file")]
    out: String,
}

fn main() {
    let args = Cli::parse();
    println!("{:?}", args);

    // generate a random peer id
    let peer_id = utils::generate_peer_id();

    // read the torrent file
    let torrent: Torrent = Torrent::from_file(&args.file).unwrap();

    // find peers (will try to use udp if possible)
    let peers = utils::find_peers(&torrent, peer_id, args.port);
}
