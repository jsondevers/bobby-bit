# Bobby-Bit

## Description

An implementation of the BitTorrent v1.0 protocol in Rust. This project is still being fine-tuned.

## Design

- Custom async event handler using [`mio`](https://docs.rs/mio/latest/mio/)
  - learn more about async programming in rust without the nice abstraction of `async`/`await` that is provided by the `futures`/`tokio` crates
- Created my own HTTP client that handles the sending, receiving, and parsing of HTTP requests and responses that are encoded using bencode (via `serde_bencode`)
  - learn more about HTTP and bencode, without the nice abstraction of `reqwest`

## Supported Features

- [HTTP](./src/tracker/http.rs) & [UDP](./src/tracker/udp.rs) tracker clients
- IPv4 & IPv6 peer connections
- Concurrent downloads
