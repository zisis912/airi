# airi-protocol

[![Version info](https://img.shields.io/crates/v/airi-protocol.svg)](https://crates.io/crates/airi-protocol)

Full implementation of the [Minecraft Java Edition Network Protocol Spec](https://minecraft.wiki/w/Java_Edition_protocol/Packets) in Rust.

[Documentation](https://docs.rs/airi-protocol/latest/airi_protocol/) • 

## Version table
| Crate Version | Minecraft Protocol Version |
| --- | --- |
| 1.x.x | [1.21.10 / 773](https://minecraft.wiki/w/Java_Edition_protocol/Packets?oldid=3657983) |

## Tests

```
cargo test
```

The `tests/test.rs` file also serves as a mini showcase file.  
For rigid testing, I'm using captured TCP traffic between a real Minecraft Client and Server (`S2C.bin`, `C2S.bin`).  
The test reads through all of the data, parsing every packet in real time.
If it fails to read any packet, the test fails.
