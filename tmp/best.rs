use crate::torrent::Torrent;
use anyhow::{anyhow, Context, Error, Result};
use log::{debug, error, info, trace, warn};
use mio::net::{TcpStream, UdpSocket};
use mio::{Events, Interest, Poll, Token};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_bencode::{from_bytes, to_bytes};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::thread;
use std::time::Duration;
use url::Url;
use urlencoding::{encode, encode_binary};

#[derive(Debug, Deserialize, Serialize)]
pub struct AnnounceRequest {
    /// The info hash of the torrent (20-byte SHA1 hash)
    pub info_hash: Vec<u8>,
    /// The peer ID of the client (20-byte string)
    pub peer_id: Vec<u8>,
    /// The IP address of the client (optional)
    #[serde(default)]
    pub ip: Option<String>,
    /// The port number that the client is listening on
    pub port: u16,
    /// The total amount uploaded (integer)
    pub uploaded: i64,
    /// The total amount downloaded (integer)
    pub downloaded: i64,
    /// The number of bytes the client still has to download until completion (integer)
    pub left: i64,
    /// true if the client accepts a compact response, false otherwise (optional, default: false)
    #[serde(default)]
    pub compact: Option<u8>,
    /// true if the client accepts a no_peer_id response, false otherwise (optional, default: false)
    #[serde(default)]
    pub no_peer_id: Option<u8>,
    /// Indicates that the tracker can omit peer id field in peers dictionary. This option is ignored if compact is enabled. (optional, default: false)
    #[serde(default)]
    pub no_peer_id_: Option<u8>,
    /// Indicates the tracker that the client accepts only IPv4 addresses. This option is ignored if compact is enabled. (optional, default: false)
    #[serde(default)]
    pub ipv4: Option<u8>,
    /// Indicates the tracker that the client accepts only IPv6 addresses. This option is ignored if compact is enabled. (optional, default: false)
    #[serde(default)]
    pub ipv6: Option<u8>,
    /// Indicates the tracker that the client accepts only IPv6 addresses. This option is ignored if compact is enabled. (optional, default: false)
    #[serde(default)]
    pub event: Option<String>,
    /// Indicates the tracker that the client accepts only IPv6 addresses. This option is ignored if compact is enabled. (optional, default: false)
    #[serde(default)]
    pub num_want: Option<i64>,
    /// Indicates the tracker that the client accepts only IPv6 addresses. This option is ignored if compact is enabled. (optional, default: false)
    #[serde(default)]
    pub key: Option<String>,
    /// Indicates the tracker that the client accepts only IPv6 addresses. This option is ignored if compact is enabled. (optional, default: false)
    #[serde(default)]
    pub tracker_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnnounceResponse {
    /// The interval in seconds that the client should wait between sending regular requests to the tracker
    pub interval: i64,
    /// The number of peers with the entire file, i.e. seeders (integer)
    pub complete: i64,
    /// The number of non-seeder peers, aka "leechers" (integer)
    pub incomplete: i64,
    /// (optional) this is an extension to the official specification, offering backwards-compatibility. (list of lists of strings).
    #[serde(default)]
    pub peers: Option<Vec<Peer>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Peer {
    /// The IP address of the peer (string or integer)
    pub ip: String,
    /// The port number of the peer (integer)
    pub port: u16,
    /// The peer ID of the peer (string)
    pub peer_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScrapeRequest {
    /// The info hash of the torrent (20-byte SHA1 hash)
    pub info_hash: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScrapeResponse {
    /// The interval in seconds that the client should wait between sending regular requests to the tracker
    pub interval: i64,
    /// The number of peers with the entire file, i.e. seeders (integer)
    pub complete: i64,
    /// The number of non-seeder peers, aka "leechers" (integer)
    pub incomplete: i64,
    /// (optional) this is an extention to the official specification, offering backwards-compatibility. (list of lists of strings).
    #[serde(default)]
    pub peers: Option<Vec<Peer>>,
}

/*
BitTorrent.org
Home For Users For Developers Developer mailing list Forums (archive)
BEP:	15
Title:	UDP Tracker Protocol for BitTorrent
Version:	023256c7581a4bed356e47caf8632be2834211bd
Last-Modified:	Thu Jan 12 12:29:12 2017 -0800
Author:	Olaf van der Spek <olafvdspek@gmail.com>
Status:	Accepted
Type:	Standards Track
Created:	13-Feb-2008
Post-History:	06-Nov-2016 (the8472.bep@infinite-source.de), specified IPv6 format.
Introduction
To discover other peers in a swarm a client announces it's existance to a tracker. The HTTP protocol is used and a typical request contains the following parameters: info_hash, key, peer_id, port, downloaded, left, uploaded and compact. A response contains a list of peers (host and port) and some other information. The request and response are both quite short. Since TCP is used, a connection has to be opened and closed, introducing additional overhead.

Overhead
Using HTTP introduces significant overhead. There's overhead at the ethernet layer (14 bytes per packet), at the IP layer (20 bytes per packet), at the TCP layer (20 bytes per packet) and at the HTTP layer. About 10 packets are used for a request plus response containing 50 peers and the total number of bytes used is about 1206 [1]. This overhead can be reduced significantly by using a UDP based protocol. The protocol proposed here uses 4 packets and about 618 bytes, reducing traffic by 50%. For a client, saving 1 kbyte every hour isn't significant, but for a tracker serving a million peers, reducing traffic by 50% matters a lot. An additional advantage is that a UDP based binary protocol doesn't require a complex parser and no connection handling, reducing the complexity of tracker code and increasing it's performance.

UDP connections / spoofing
In the ideal case, only 2 packets would be necessary. However, it is possible to spoof the source address of a UDP packet. The tracker has to ensure this doesn't occur, so it calculates a value (connection_id) and sends it to the client. If the client spoofed it's source address, it won't receive this value (unless it's sniffing the network). The connection_id will then be send to the tracker again in packet 3. The tracker verifies the connection_id and ignores the request if it doesn't match. Connection IDs should not be guessable by the client. This is comparable to a TCP handshake and a syn cookie like approach can be used to storing the connection IDs on the tracker side. A connection ID can be used for multiple requests. A client can use a connection ID until one minute after it has received it. Trackers should accept the connection ID until two minutes after it has been send.

Time outs
UDP is an 'unreliable' protocol. This means it doesn't retransmit lost packets itself. The application is responsible for this. If a response is not received after 15 * 2 ^ n seconds, the client should retransmit the request, where n starts at 0 and is increased up to 8 (3840 seconds) after every retransmission. Note that it is necessary to rerequest a connection ID when it has expired.

Examples
Normal announce:

t = 0: connect request
t = 1: connect response
t = 2: announce request
t = 3: announce response
Connect times out:

t = 0: connect request
t = 15: connect request
t = 45: connect request
t = 105: connect request
etc
Announce times out:

t = 0:
t = 0: connect request
t = 1: connect response
t = 2: announce request
t = 17: announce request
t = 47: announce request
t = 107: connect request (because connection ID expired)
t = 227: connect request
etc
Multiple requests:

t = 0: connect request
t = 1: connect response
t = 2: announce request
t = 3: announce response
t = 4: announce request
t = 5: announce response
t = 60: announce request
t = 61: announce response
t = 62: connect request
t = 63: connect response
t = 64: announce request
t = 64: scrape request
t = 64: scrape request
t = 64: announce request
t = 65: announce response
t = 66: announce response
t = 67: scrape response
t = 68: scrape response
UDP tracker protocol
All values are send in network byte order (big endian). Do not expect packets to be exactly of a certain size. Future extensions could increase the size of packets.

Connect
Before announcing or scraping, you have to obtain a connection ID.

Choose a random transaction ID.
Fill the connect request structure.
Send the packet.
connect request:

Offset  Size            Name            Value
0       64-bit integer  protocol_id     0x41727101980 // magic constant
8       32-bit integer  action          0 // connect
12      32-bit integer  transaction_id
16
Receive the packet.
Check whether the packet is at least 16 bytes.
Check whether the transaction ID is equal to the one you chose.
Check whether the action is connect.
Store the connection ID for future use.
connect response:

Offset  Size            Name            Value
0       32-bit integer  action          0 // connect
4       32-bit integer  transaction_id
8       64-bit integer  connection_id
16
Announce
Choose a random transaction ID.
Fill the announce request structure.
Send the packet.
IPv4 announce request:

Offset  Size    Name    Value
0       64-bit integer  connection_id
8       32-bit integer  action          1 // announce
12      32-bit integer  transaction_id
16      20-byte string  info_hash
36      20-byte string  peer_id
56      64-bit integer  downloaded
64      64-bit integer  left
72      64-bit integer  uploaded
80      32-bit integer  event           0 // 0: none; 1: completed; 2: started; 3: stopped
84      32-bit integer  IP address      0 // default
88      32-bit integer  key
92      32-bit integer  num_want        -1 // default
96      16-bit integer  port
98
Receive the packet.
Check whether the packet is at least 20 bytes.
Check whether the transaction ID is equal to the one you chose.
Check whether the action is announce.
Do not announce again until interval seconds have passed or an event has occurred.
Do note that most trackers will only honor the IP address field under limited circumstances.

IPv4 announce response:

Offset      Size            Name            Value
0           32-bit integer  action          1 // announce
4           32-bit integer  transaction_id
8           32-bit integer  interval
12          32-bit integer  leechers
16          32-bit integer  seeders
20 + 6 * n  32-bit integer  IP address
24 + 6 * n  16-bit integer  TCP port
20 + 6 * N
IPv6
IPv6 announces have the same structure as v4 ones, including the used action number except that the stride size of <IP address, TCP port> pairs in the response is 18 bytes instead of 6.

That means the IP address field in the request remains 32bits wide which makes this field not usable under IPv6 and thus should always be set to 0.

Which format is used is determined by the address family of the underlying UDP packet. I.e. packets from a v4 address use the v4 format, those from a v6 address use the v6 format.

Clients that resolve hostnames to v4 and v6 and then announce to both should use the same key for both so that trackers that care about accurate statistics-keeping can match the two announces.

Scrape
Up to about 74 torrents can be scraped at once. A full scrape can't be done with this protocol.

Choose a random transaction ID.
Fill the scrape request structure.
Send the packet.
scrape request:

Offset          Size            Name            Value
0               64-bit integer  connection_id
8               32-bit integer  action          2 // scrape
12              32-bit integer  transaction_id
16 + 20 * n     20-byte string  info_hash
16 + 20 * N
Receive the packet.
Check whether the packet is at least 8 bytes.
Check whether the transaction ID is equal to the one you chose.
Check whether the action is scrape.
scrape response:

Offset      Size            Name            Value
0           32-bit integer  action          2 // scrape
4           32-bit integer  transaction_id
8 + 12 * n  32-bit integer  seeders
12 + 12 * n 32-bit integer  completed
16 + 12 * n 32-bit integer  leechers
8 + 12 * N
If the tracker encounters an error, it might send an error packet.

Receive the packet.
Check whether the packet is at least 8 bytes.
Check whether the transaction ID is equal to the one you chose.
Errors
error response:

Offset  Size            Name            Value
0       32-bit integer  action          3 // error
4       32-bit integer  transaction_id
8       string  message
Existing implementations
Azureus, libtorrent [2], opentracker [3], XBT Client and XBT Tracker support this protocol.

Extensions
Extension bits or a version field are not included. Clients and trackers should not assume packets to be of a certain size. This way, additional fields can be added without breaking compatibility.

See BEP 41 [4] for an extension negotiation protocol.

References and Footnotes
[1]	http://xbtt.sourceforge.net/udp_tracker_protocol.html
[2]	http://www.rasterbar.com/products/libtorrent/udp_tracker_protocol.html
[3]	http://opentracker.blog.h3q.com/
[4]	http://bittorrent.org/beps/bep_0041.html

*/

const UDP_TRACKER_PROTOCOL_ID: i64 = 0x41727101980; // magic constant

#[derive(Debug)]
pub struct UdpTracker {
    pub url: String,
    pub connection_id: Option<i64>,
    pub transaction_id: Option<i64>,
    pub socket: Option<UdpSocket>,
}

impl UdpTracker {
    pub fn new(url: String) -> Self {
        Self {
            url,
            connection_id: None,
            transaction_id: None,
            socket: None,
        }
    }

    pub fn connect(&mut self) -> Result<()> {
        let mut rng = rand::thread_rng();
        let transaction_id = rng.gen::<u32>();
        let mut buf = [0u8; 16];
        let mut socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        let addr = self.url.parse::<SocketAddr>()?;
        socket.connect(addr)?;

        let mut connect_request = [0u8; 16];
        let mut offset = 0;
        connect_request[offset..offset + 8].copy_from_slice(&UDP_TRACKER_PROTOCOL_ID.to_be_bytes()); // protocol_id
        offset += 8;
        connect_request[offset..offset + 4].copy_from_slice(&0i32.to_be_bytes()); // action
        offset += 4;
        connect_request[offset..offset + 4].copy_from_slice(&transaction_id.to_be_bytes()); // transaction_id
        socket.send(&connect_request)?; // send connect request

        let mut events = Events::with_capacity(1024);
        let mut poll = Poll::new()?;
        poll.registry()
            .register(&mut socket, Token(0), Interest::READABLE)?;

        loop {
            poll.poll(&mut events, Some(Duration::from_secs(1)))?;
            for event in events.iter() {
                match event.token() {
                    Token(0) => {
                        let n = socket.recv(&mut buf)?;
                        if n < 16 {
                            return Err(anyhow!("invalid connect response"));
                        }
                        let mut offset = 0;
                        let action =
                            i32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
                        offset += 4;
                        let transaction_id =
                            i32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
                        offset += 4;
                        let connection_id =
                            i64::from_be_bytes(buf[offset..offset + 8].try_into().unwrap());
                        if action != 0 {
                            return Err(anyhow!("invalid action"));
                        }
                        if transaction_id != transaction_id {
                            return Err(anyhow!("invalid transaction id"));
                        }
                        self.connection_id = Some(connection_id);
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn announce(&mut self) -> Result<()> {
        let mut rng = rand::thread_rng();
        let transaction_id = rng.gen::<u32>();
        let mut buf = [0u8; 98];
        let mut socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        let addr = self.url.parse::<SocketAddr>()?;
        socket.connect(addr)?;

        let mut announce_request = [0u8; 98];
        let mut offset = 0;
        announce_request[offset..offset + 8].copy_from_slice(
            &self
                .connection_id
                .ok_or_else(|| anyhow!("no connection id"))?
                .to_be_bytes(),
        );
        offset += 8;
        announce_request[offset..offset + 4].copy_from_slice(&1i32.to_be_bytes()); // action
        offset += 4;
        announce_request[offset..offset + 4].copy_from_slice(&transaction_id.to_be_bytes());
        offset += 4;
        announce_request[offset..offset + 20].copy_from_slice(&[0u8; 20]); // info_hash
        offset += 20;
        announce_request[offset..offset + 20].copy_from_slice(&[0u8; 20]); // peer_id
        offset += 20;
        announce_request[offset..offset + 8].copy_from_slice(&0i64.to_be_bytes()); // downloaded
        offset += 8;
        announce_request[offset..offset + 8].copy_from_slice(&0i64.to_be_bytes()); // left
        offset += 8;
        announce_request[offset..offset + 8].copy_from_slice(&0i64.to_be_bytes()); // uploaded
        offset += 8;
        announce_request[offset..offset + 4].copy_from_slice(&0i32.to_be_bytes()); // event
        offset += 4;
        announce_request[offset..offset + 4].copy_from_slice(&0i32.to_be_bytes()); // ip
        offset += 4;
        announce_request[offset..offset + 4].copy_from_slice(&0i32.to_be_bytes()); // key
        offset += 4;
        announce_request[offset..offset + 4].copy_from_slice(&0i32.to_be_bytes()); // num_want
        offset += 4;
        announce_request[offset..offset + 2].copy_from_slice(&0i16.to_be_bytes()); // port
        offset += 2; // port
        socket.send(&announce_request)?;

        let mut events = Events::with_capacity(1024);
        let mut poll = Poll::new()?;
        poll.registry()
            .register(&mut socket, Token(0), Interest::READABLE)?;

        loop {
            poll.poll(&mut events, Some(Duration::from_secs(1)))?;
            for event in events.iter() {
                match event.token() {
                    Token(0) => {
                        let n = socket.recv(&mut buf)?;
                        if n < 20 {
                            return Err(anyhow!("invalid announce response"));
                        }
                        let mut offset = 0;
                        let action =
                            i32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
                        offset += 4;
                        let transaction_id =
                            i32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
                        offset += 4;
                        let interval =
                            i32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
                        offset += 4;
                        let leechers =
                            i32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
                        offset += 4;
                        let seeders =
                            i32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());

                        if action != 1 {
                            return Err(anyhow!("invalid action"));
                        }
                        if transaction_id != transaction_id {
                            return Err(anyhow!("invalid transaction id"));
                        }

                        debug!("interval: {}", interval);
                        debug!("leechers: {}", leechers);
                        debug!("seeders: {}", seeders);
                        debug!("announce response: {:?}", &buf[..n]);
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }
}

/*
Tracker HTTP/HTTPS Protocol
The tracker is an HTTP/HTTPS service which responds to HTTP GET requests. The requests include metrics from clients that help the tracker keep overall statistics about the torrent. The response includes a peer list that helps the client participate in the torrent. The base URL consists of the "announce URL" as defined in the metainfo (.torrent) file. The parameters are then added to this URL, using standard CGI methods (i.e. a '?' after the announce URL, followed by 'param=value' sequences separated by '&').

Note that all binary data in the URL (particularly info_hash and peer_id) must be properly escaped. This means any byte not in the set 0-9, a-z, A-Z, '.', '-', '_' and '~', must be encoded using the "%nn" format, where nn is the hexadecimal value of the byte. (See RFC1738 for details.)

For a 20-byte hash of \x12\x34\x56\x78\x9a\xbc\xde\xf1\x23\x45\x67\x89\xab\xcd\xef\x12\x34\x56\x78\x9a,
The right encoded form is %124Vx%9A%BC%DE%F1%23Eg%89%AB%CD%EF%124Vx%9A

Tracker Request Parameters
The parameters used in the client->tracker GET request are as follows:

info_hash: urlencoded 20-byte SHA1 hash of the value of the info key from the Metainfo file. Note that the value will be a bencoded dictionary, given the definition of the info key above.
peer_id: urlencoded 20-byte string used as a unique ID for the client, generated by the client at startup. This is allowed to be any value, and may be binary data. There are currently no guidelines for generating this peer ID. However, one may rightly presume that it must at least be unique for your local machine, thus should probably incorporate things like process ID and perhaps a timestamp recorded at startup. See peer_id below for common client encodings of this field.
port: The port number that the client is listening on. Ports reserved for BitTorrent are typically 6881-6889. Clients may choose to give up if it cannot establish a port within this range.
uploaded: The total amount uploaded (since the client sent the 'started' event to the tracker) in base ten ASCII. While not explicitly stated in the official specification, the concensus is that this should be the total number of bytes uploaded.
downloaded: The total amount downloaded (since the client sent the 'started' event to the tracker) in base ten ASCII. While not explicitly stated in the official specification, the consensus is that this should be the total number of bytes downloaded.
left: The number of bytes this client still has to download in base ten ASCII. Clarification: The number of bytes needed to download to be 100% complete and get all the included files in the torrent.
compact: Setting this to 1 indicates that the client accepts a compact response. The peers list is replaced by a peers string with 6 bytes per peer. The first four bytes are the host (in network byte order), the last two bytes are the port (again in network byte order). It should be noted that some trackers only support compact responses (for saving bandwidth) and either refuse requests without "compact=1" or simply send a compact response unless the request contains "compact=0" (in which case they will refuse the request.)
no_peer_id: Indicates that the tracker can omit peer id field in peers dictionary. This option is ignored if compact is enabled.
event: If specified, must be one of started, completed, stopped, (or empty which is the same as not being specified). If not specified, then this request is one performed at regular intervals.
started: The first request to the tracker must include the event key with this value.
stopped: Must be sent to the tracker if the client is shutting down gracefully.
completed: Must be sent to the tracker when the download completes. However, must not be sent if the download was already 100% complete when the client started. Presumably, this is to allow the tracker to increment the "completed downloads" metric based solely on this event.
ip: Optional. The true IP address of the client machine, in dotted quad format or rfc3513 defined hexed IPv6 address. Notes: In general this parameter is not necessary as the address of the client can be determined from the IP address from which the HTTP request came. The parameter is only needed in the case where the IP address that the request came in on is not the IP address of the client. This happens if the client is communicating to the tracker through a proxy (or a transparent web proxy/cache.) It also is necessary when both the client and the tracker are on the same local side of a NAT gateway. The reason for this is that otherwise the tracker would give out the internal (RFC1918) address of the client, which is not routable. Therefore the client must explicitly state its (external, routable) IP address to be given out to external peers. Various trackers treat this parameter differently. Some only honor it only if the IP address that the request came in on is in RFC1918 space. Others honor it unconditionally, while others ignore it completely. In case of IPv6 address (e.g.: 2001:db8:1:2::100) it indicates only that client can communicate via IPv6.
numwant: Optional. Number of peers that the client would like to receive from the tracker. This value is permitted to be zero. If omitted, typically defaults to 50 peers.
key: Optional. An additional identification that is not shared with any other peers. It is intended to allow a client to prove their identity should their IP address change.
trackerid: Optional. If a previous announce contained a tracker id, it should be set here.
Tracker Response
The tracker responds with "text/plain" document consisting of a bencoded dictionary with the following keys:

failure reason: If present, then no other keys may be present. The value is a human-readable error message as to why the request failed (string).
warning message: (new, optional) Similar to failure reason, but the response still gets processed normally. The warning message is shown just like an error.
interval: Interval in seconds that the client should wait between sending regular requests to the tracker
min interval: (optional) Minimum announce interval. If present clients must not reannounce more frequently than this.
tracker id: A string that the client should send back on its next announcements. If absent and a previous announce sent a tracker id, do not discard the old value; keep using it.
complete: number of peers with the entire file, i.e. seeders (integer)
incomplete: number of non-seeder peers, aka "leechers" (integer)
peers: (dictionary model) The value is a list of dictionaries, each with the following keys:
peer id: peer's self-selected ID, as described above for the tracker request (string)
ip: peer's IP address either IPv6 (hexed) or IPv4 (dotted quad) or DNS name (string)
port: peer's port number (integer)
peers: (binary model) Instead of using the dictionary model described above, the peers value may be a string consisting of multiples of 6 bytes. First 4 bytes are the IP address and last 2 bytes are the port number. All in network (big endian) notation.
As mentioned above, the list of peers is length 50 by default. If there are fewer peers in the torrent, then the list will be smaller. Otherwise, the tracker randomly selects peers to include in the response. The tracker may choose to implement a more intelligent mechanism for peer selection when responding to a request. For instance, reporting seeds to other seeders could be avoided.

Clients may send a request to the tracker more often than the specified interval, if an event occurs (i.e. stopped or completed) or if the client needs to learn about more peers. However, it is considered bad practice to "hammer" on a tracker to get multiple peers. If a client wants a large peer list in the response, then it should specify the num_want parameter.

Implementer's Note: Even 30 peers is plenty, the official client version 3 in fact only actively forms new connections if it has less than 30 peers and will refuse connections if it has 55. This value is important to performance. When a new piece has completed download, HAVE messages (see below) will need to be sent to most active peers. As a result the cost of broadcast traffic grows in direct proportion to the number of peers. Above 25, new peers are highly unlikely to increase download speed. UI designers are strongly advised to make this obscure and hard to change as it is very rare to be useful to do so.

Tracker 'scrape' Convention
By convention most trackers support another form of request, which queries the state of a given torrent (or all torrents) that the tracker is managing. This is referred to as the "scrape page" because it automates the otherwise tedious process of "screen scraping" the tracker's stats page.

The scrape URL is also a HTTP GET method, similar to the one described above. However the base URL is different. To derive the scrape URL use the following steps: Begin with the announce URL. Find the last '/' in it. If the text immediately following that '/' isn't 'announce' it will be taken as a sign that that tracker doesn't support the scrape convention. If it does, substitute 'scrape' for 'announce' to find the scrape page.

Examples: (announce URL -> scrape URL)

  ~http://example.com/announce          -> ~http://example.com/scrape
  ~http://example.com/x/announce        -> ~http://example.com/x/scrape
  ~http://example.com/announce.php      -> ~http://example.com/scrape.php
  ~http://example.com/a                 -> (scrape not supported)
  ~http://example.com/announce?x2%0644 -> ~http://example.com/scrape?x2%0644
  ~http://example.com/announce?x=2/4    -> (scrape not supported)
  ~http://example.com/x%064announce     -> (scrape not supported)
Note especially that entity unquoting is not to be done. This standard is documented by Bram in the BitTorrent development list archive: http://groups.yahoo.com/group/BitTorrent/message/3275

The scrape URL may be supplemented by the optional parameter info_hash, a 20-byte value as described above. This restricts the tracker's report to that particular torrent. Otherwise stats for all torrents that the tracker is managing are returned. Software authors are strongly encouraged to use the info_hash parameter when at all possible, to reduce the load and bandwidth of the tracker.

You may also specify multiple info_hash parameters to trackers that support it. While this isn't part of the official specifications it has become somewhat a defacto standard - for example:

 http://example.com/scrape.php?info_hash=aaaaaaaaaaaaaaaaaaaa&info_hash=bbbbbbbbbbbbbbbbbbbb&info_hash=cccccccccccccccccccc
The response of this HTTP GET method is a "text/plain" or sometimes gzip compressed document consisting of a bencoded dictionary, containing the following keys:

files: a dictionary containing one key/value pair for each torrent for which there are stats. If info_hash was supplied and was valid, this dictionary will contain a single key/value. Each key consists of a 20-byte binary info_hash. The value of each entry is another dictionary containing the following:
complete: number of peers with the entire file, i.e. seeders (integer)
downloaded: total number of times the tracker has registered a completion ("event=complete", i.e. a client finished downloading the torrent)
incomplete: number of non-seeder peers, aka "leechers" (integer)
name: (optional) the torrent's internal name, as specified by the "name" file in the info section of the .torrent file
Note that this response has three levels of dictionary nesting. Here's an example:

d5:filesd20:....................d8:completei5e10:downloadedi50e10:incompletei10eeee

Where .................... is the 20 byte info_hash and there are 5 seeders, 10 leechers, and 50 complete downloads.

Unofficial extensions to scrape
Below are the response keys are being unofficially used. Since they are unofficial, they are all optional.

failure reason: Human-readable error message as to why the request failed (string). Clients known to handle this key: Azureus.
flags: a dictionary containing miscellaneous flags. The value of the flags key is another nested dictionary, possibly containing the following:
min_request_interval: The value for this key is an integer specifying how the minimum number of seconds for the client to wait before scraping the tracker again. Trackers known to send this key: BNBT. Clients known to handle this key: Azureus.
*/

#[derive(Debug)]
pub struct HttpTracker {
    pub socket: Option<TcpStream>,
    pub url: String,
}

impl HttpTracker {
    pub fn new() -> Self {
        Self {
            socket: None,
            url: String::new(),
        }
    }

    pub fn announce(&mut self, url: &str) -> Result<()> {
        println!("url: {}", url);
        let mut socket = TcpStream::connect(resolve_url(url)?)?;
        let mut buf = [0u8; 1024];

        // make a GET request to the tracker (HTTP)
        let mut announce_request = String::new();
        announce_request.push_str("GET ");
        announce_request.push_str(url);
        announce_request.push_str(" HTTP/1.1\r\n");
        announce_request.push_str("Host: ");
        announce_request.push_str(&url.parse::<Url>()?.host_str().unwrap());
        announce_request.push_str("\r\n");
        announce_request.push_str("User-Agent: ");
        announce_request.push_str("rust-torrent");
        announce_request.push_str("\r\n");
        announce_request.push_str("Accept: */*\r\n");
        announce_request.push_str("Connection: close\r\n");
        announce_request.push_str("\r\n");

        println!("request: {}", announce_request);

        socket.write_all(announce_request.as_bytes())?;

        // try to read the response, if we get a WouldBlock error, then we need to wait for the socket to become readable
        let mut events = Events::with_capacity(1024);
        let mut poll = Poll::new()?;
        poll.registry()
            .register(&mut socket, Token(0), Interest::READABLE)?;

        loop {
            poll.poll(&mut events, Some(Duration::from_secs(1)))?;
            for event in events.iter() {
                match event.token() {
                    Token(0) => {
                        let n = socket.read(&mut buf)?;
                        if n == 0 {
                            return Ok(());
                        }
                        println!("{}", String::from_utf8_lossy(&buf[..n]));
                    }
                    _ => {}
                }
            }
        }
    }
}

fn resolve_url(url: &str) -> Result<SocketAddr> {
    println!("url: {}", url);
    let url = Url::parse(url).context("Failed to parse URL")?;
    let host = url.host_str().context("Missing host in URL")?;
    let port = url.port().unwrap_or(6969);
    (host, port)
        .to_socket_addrs()
        .context("Failed to resolve host")?
        .next()
        .ok_or_else(|| anyhow!("No address found for host"))
}

fn build_url(t: &Torrent, params: &[(&str, &str)]) -> Result<String> {
    let mut url = String::new();
    url.push_str(&t.announce());
    url.push_str("?info_hash=");
    url.push_str(&encode_binary(&t.info_hash()));
    url.push_str("&peer_id=");
    url.push_str(&encode_binary(&generate_peer_id()));

    for (key, value) in params {
        url.push_str("&");
        url.push_str(key);
        url.push_str("=");
        url.push_str(&encode(value));
    }

    Ok(url)
}

fn generate_peer_id() -> [u8; 20] {
    let mut peer_id = [0u8; 20];
    let mut rng = rand::thread_rng();
    rng.fill(&mut peer_id);
    peer_id
}
