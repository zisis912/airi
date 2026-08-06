# airi-protocol

[![Version info](https://img.shields.io/crates/v/airi-protocol.svg)](https://crates.io/crates/airi-protocol)

Full implementation of the [Minecraft Java Edition Network Protocol Spec](https://minecraft.wiki/w/Java_Edition_protocol/Packets) in Rust.

## Version table
| Crate Version | Minecraft Protocol Version |
| --- | --- |
| 1.x.x | [1.21.10 / 773](https://minecraft.wiki/w/Java_Edition_protocol/Packets?oldid=3657983) |

## Basic Usage

Note: Async versions of all of these structs exist, such as `AsyncNetworkDecoder` and `AsyncNetworkEncoder`, which implement `AsyncRead` and `AsyncWrite` respectively.

### Reading Packets

Useful functions:

```rust
// creates unencrypted reader
let decoder = NetworkDecoder::new(R: Read);
// enables Zlib decompression with threshold 256
decoder.set_compression(256);
// enables AES256-Cfb8 decryption
decoder.set_encryption(key: &[u8; 16]);
// reads 1 full raw packet from the reader
let RawPacket { id, payload } = decoder.get_raw_packet()?;
// parse the packet
let packet = packet_by_id(State::Play, Direction::Clientbound, id, &mut &payload[..])?;
```

---

### Writing Packets

Useful functions:

```rust
// creates unencrypted writer
let encoder = NetworkEncoder::new(W: Write);
// enables Zlib compression on the writer
encoder.set_compression(256);
// enables AES256-Cfb8 encryption
encoder.set_encryption(key: &[u8; 16]);

// prepare packet payload
let mut buf = Vec::new();
Packet::Handshake(Handshake {
    protocol_version: VarInt(773),
    server_adress: "localhost".to_owned(),
    server_port: 25565,
    intent: Intent::Login,
}).write(&mut buf)?;

// write the packet payload to writer
encoder.write_packet(buf)?;
```
