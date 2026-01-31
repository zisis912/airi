# mc-rust-protocol

Full implementation of the [Minecraft Java Edition network protocol spec](https://minecraft.wiki/w/Java_Edition_protocol/Packets) in Rust.

Protocol version: **1.21.10, protocol 773**

## Tests/Examples

```
cargo test -- --nocapture > output.txt
```

The `tests/test.rs` file also serves as a usage example file.  
For rigid testing, I'm using captured TCP traffic between a real Minecraft Client and Server (`S2C.bin`, `C2S.bin`).  
The test reads through all of the data, printing the deserialized packets in a file.  
If it fails to read any packet, the test fails.

```
cargo test
```

Faster alternative to the above test, doesn't print data to a file.

## Basic Usage

Note: Async versions of all of these structs exist, such as `AsyncNetworkDecoder` and `AsyncNetworkEncoder`

### Reading Packets

Useful functions:

```rust
// (creates unencrypted reader)
let decoder = NetworkDecoder::new(R: Read);
// (enables Zlib decompression with threshold n)
decoder.set_compression(n: usize);
// (enables AES256-Cfb8 decryption)
decoder.set_encryption(key: &[u8; 16]);
// reads 1 full raw packet from the reader
let RawPacket { id, payload } = decoder.get_raw_packet()?;
// parse the packet
let packet = packet_by_id(State::Play, Direction::Clientbound, id, &mut &payload[..])?;
```

---

### Writing Packets

#### Useful functions:

```rust
// (creates unencrypted writer)
let decoder = NetworkEncoder::new(W: Write);
// (enables Zlib compression with threshold n)
encoder.set_compression(n: usize);
// (enables AES256-Cfb8 encryption)
encoder.set_encryption(key: &[u8; 16]);

// prepare packet payload
let mut buf = Vec::new();
VarInt(Handshake::ID).write_to(&mut buf)?;
Packet::Handshake(Handshake {
    protocol_version: VarInt(773),
    server_adress: "localhost".to_owned(),
    server_port: 25565,
    intent: Intent::Login,
}).write(&mut buf)?;

// write the packet payload to writer
encoder.write_packet(buf)?;
```
