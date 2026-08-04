use std::error::Error;

use mc_rust_protocol::{
    RawPacket,
    packet::{
        self, Direction, Packet, State,
    },
    packet_decoder::NetworkDecoder,
};
use rsa::{
    Pkcs1v15Encrypt, RsaPrivateKey,
    pkcs8::DecodePrivateKey,
};


#[test]
fn testing() {
    let _ = sample_data(Direction::Clientbound);
    let _ = sample_data(Direction::Serverbound);
}

fn sample_data(decrypt_dir: Direction) -> Result<(), Box<dyn Error>> {
    // let c2s = File::open("tests/sample_data/C2S.bin")?;
    let c2s = include_bytes!("sample_data/C2S.bin");
    // let s2c = File::open("tests/sample_data/S2C.bin")?;
    let s2c = include_bytes!("sample_data/S2C.bin");

    let server_private_key = RsaPrivateKey::from_pkcs8_der(&hex::decode("30820276020100300d06092a864886f70d0101010500048202603082025c02010002818100b8e9bff8624c3ae888ab0cfceeebdc509d452f1a15a140614a5cc3b6387120649da5d53b6b3fe250d07d18ccca0bf14467cd8217346bdbfe7a6ff1736ecfac80d8bcb80940be1cb859e0a33ff1814664dd46defddda6fa3abdd063ca6e933da9cb2710e4b17b5b4cf96ac0fa9b8d1d780105db1b471e77ea3de3a87d373772e10203010001028180549547e4bc4216682babe2a3083f076630aa66e34da5972769b689279f25d025761f572c78e09e0b4d730b97118ce8eddb759bde3572690d3cc05bf7eb663f875f8343a634f33bb87f99f73f6ab95e042e2543d0d4b777e090de457bf8d409e1b65469f9c952a98f3cb0217557a06f1d2729469c57562935fc01152dfc38b509024100fb24a657710b710072a0da2a9637a08ea559c1c85a89c46c526520540ae0a8302b0f0c47c5857d94d2301d2d6a6df58631d28193ca039440b5b1e6e910100a6b024100bc7d35f0ee4f0209f5216db2ac5b1d0dac57bd64d9e413413c94a914e1b530e17b6c78d5cd29ad443c1231f0666064688f03690eb8e0bf3f8736342765d422e30240290e92cb14c6041148ac173e8314510140f2ed852d97fc2ea141bb094245fbf8f3f11fd6d3e9c0e00584ac207297cb5dc6e35d1fa614f3b5a87e8efb670ed845024027394f2e520932fd6b7b875e752b88c23da90c8a9e252e34972cc07acdf56cb49f80952cb8c301817f96b1b9bb3437f0e241ed6cd8e03c2c3630fb6d6f6d53cd024100a80c554e9f3c4e6e3854c01894f6993a336fd6675912089db1a7c7a98a161f1d4009b526d4a7b0caa1f607af5587778f4de0eee9ac887f4ab4317d22dbaf1cca").unwrap()).unwrap();
    // let mut server_public_key: Option<RsaPublicKey> = None;

    let (decoder, mut state) = match decrypt_dir {
        Direction::Clientbound => (&mut NetworkDecoder::new(&s2c[..]), State::Login),
        Direction::Serverbound => (&mut NetworkDecoder::new(&c2s[..]), State::Handshake),
    };

    println!("DIRECTION: {:?}", decrypt_dir);
    println!("setting state to login");

    loop {
        let RawPacket { id, payload } = decoder.get_raw_packet()?;

        println!("id: {:#04x}", id);
        println!("length: {}", payload.len());

        let packet = packet::packet_by_id(state, decrypt_dir, id, &mut &payload[..]).unwrap();

        println!("{:#?}", packet);

        match packet {
            Packet::Handshake(p) => state = p.intent.into(),
            Packet::EncryptionRequest(_p) => {
                // server_public_key =
                //     Some(RsaPublicKey::from_public_key_der(&p.public_key.data)?);
                let aes_key = hex::decode("7532710be168544415a69d2a122b4230")
                    .unwrap()
                    .try_into()
                    .unwrap();

                decoder.set_encryption(&aes_key);
            }
            Packet::EncryptionResponse(p) => {
                let aes_key: [u8; 16] = server_private_key
                    .decrypt(Pkcs1v15Encrypt, &p.shared_secret.data)
                    .unwrap()[0..16]
                    .try_into()
                    .unwrap();
                println!("acquired AES key: {:#?}", hex::encode(aes_key));
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
