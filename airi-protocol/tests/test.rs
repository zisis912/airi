use std::{error::Error, fs::File};

use airi_protocol::{
    RawPacket,
    packet::{self, Direction, Packet, State},
    packet_decoder::{NetworkDecoder, PacketDecodeError},
};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, pkcs8::DecodePrivateKey};
use thiserror::Error;

#[derive(Error, Debug)]
enum TestError {
    #[error("couldnt obtain aes key from c2s EncryptionResponse packet")]
    AesKeyMissing,
}

#[test]
fn testing() {
    // first decrypt aes key as the server, then use it in the client
    let mut aes_key = None;

    let _ = sample_data(Direction::Serverbound, &mut aes_key);
    let _ = sample_data(Direction::Clientbound, &mut aes_key);
}

fn sample_data(
    decrypt_dir: Direction,
    aes_key_g: &mut Option<[u8; 16]>,
) -> Result<(), Box<dyn Error>> {
    let c2s = File::open("tests/sample_data/C2S.bin").unwrap();
    let s2c = File::open("tests/sample_data/S2C.bin").unwrap();

    // pop newline at the end
    let key = include_str!("sample_data/rsa_key.txt").replace('\n', "");

    let server_private_key = RsaPrivateKey::from_pkcs8_der(&hex::decode(key).unwrap()).unwrap();

    let (decoder, mut state) = match decrypt_dir {
        Direction::Clientbound => (&mut NetworkDecoder::new(&s2c), State::Login),
        Direction::Serverbound => (&mut NetworkDecoder::new(&c2s), State::Handshake),
    };

    println!("DIRECTION: {:?}", decrypt_dir);
    println!("setting state to login");

    loop {
        let res = decoder.get_raw_packet();
        // if we reach eof successfully test passed
        if let Err(PacketDecodeError::FailedDecompression(ref e)) = res
            && e == "IO error: failed to fill whole buffer"
        {
            return Ok(());
        }
        let RawPacket { id, payload } = res.unwrap();

        println!("id: {:#04x}", id);
        println!("length: {}", payload.len());

        let mut payload_r = &payload[..];
        let packet = packet::packet_by_id(state, decrypt_dir, id, &mut payload_r).unwrap();

        println!("{:#?}", packet);

        if !payload_r.is_empty() {
            panic!("didnt read full packet: {} bytes left", payload_r.len());
        }

        match packet {
            Packet::Handshake(p) => state = p.intent.into(),
            Packet::EncryptionRequest(_p) => {
                // server_public_key =
                //     Some(RsaPublicKey::from_public_key_der(&p.public_key.data).unwrap());
                // let aes_key = hex::decode("7532710be168544415a69d2a122b4230").unwrap().try_into().map_err(|_|TestError::InvalidAesKeyLength)?;

                // unwrap cannot fail realistically
                decoder.set_encryption(&aes_key_g.ok_or(TestError::AesKeyMissing).unwrap());
            }
            Packet::EncryptionResponse(p) => {
                let aes_key: [u8; 16] = server_private_key
                    .decrypt(Pkcs1v15Encrypt, &p.shared_secret.data)
                    .unwrap()[0..16]
                    .try_into()?;
                println!("acquired AES key: {:#?}", hex::encode(aes_key));
                *aes_key_g = Some(aes_key);
                decoder.set_encryption(&aes_key);
                decoder.set_compression(256);
            }
            Packet::SetCompression(p) => {
                println!("acquired compression value: {:?}", p.theshold.0);
                decoder.set_compression(p.theshold.0.try_into().unwrap());
            }
            Packet::LoginSuccess(_p) => {
                state = State::Configuration;
                println!("set state to config");
            }
            Packet::LoginAcknowledged(_p) => {
                state = State::Configuration;
                println!("set state to config");
            }
            Packet::FinishConfiguration(_p) => {
                state = State::Play;
                println!("set state to play");
            }
            Packet::AcknowledgeFinishConfiguration(_p) => {
                state = State::Play;
                println!("set state to play");
            }
            _ => {}
        }
    }

    // Ok(())
}
