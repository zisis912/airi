use airi_protocol::{
    RawPacket,
    async_packet_decoder::AsyncNetworkDecoder,
    async_packet_encoder::AsyncNetworkEncoder,
    packet::{
        Direction, Packet, PacketType, State,
        c2s::{
            configuration::{
                AcknowledgeFinishConfiguration, PongConfiguration,
                ServerboundKeepAliveConfiguration, ServerboundKnownPacks,
            },
            login::{EncryptionResponse, LoginAcknowledged},
            play::{PongPlay, ServerboundKeepAlivePlay},
        },
        packet_by_id,
    },
};
use log::{debug, trace};
use rsa::{Pkcs1v15Encrypt, RsaPublicKey, pkcs8::DecodePublicKey, rand_core::RngCore};
use tokio::{
    io::{BufReader, BufWriter},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    select,
    sync::mpsc::{Receiver, Sender},
};

use crate::auth::Profile;

// 0-9
const COMPRESSION: u32 = 9;

pub struct NetworkHandler {
    pub profile: Option<Profile>,
    pub state: State,
    pub s2c_send: Sender<Packet>,
    pub c2s_recv: Receiver<Packet>,
    pub network_writer: AsyncNetworkEncoder<BufWriter<OwnedWriteHalf>>,
    pub network_reader: AsyncNetworkDecoder<BufReader<OwnedReadHalf>>,
}

impl NetworkHandler {
    pub fn new(
        tcp_stream: TcpStream,
        c2s_recv: Receiver<Packet>,
        s2c_send: Sender<Packet>,
        profile: Option<Profile>,
    ) -> Self {
        let (read, write) = tcp_stream.into_split();
        let network_reader = AsyncNetworkDecoder::new(BufReader::new(read));
        let network_writer = AsyncNetworkEncoder::new(BufWriter::new(write));

        NetworkHandler {
            state: State::Handshake,
            s2c_send,
            c2s_recv,
            network_writer,
            network_reader,
            profile,
        }
    }

    pub async fn network_loop(&mut self) {
        loop {
            select! {
                receive = self.network_reader.get_raw_packet() => {
                    // debug!("receiving packet");

                    let raw_packet = receive.expect("received malformed packet");
                    let packet = self.parse_raw_packet(raw_packet);
                    self.handle_s2c_internal(&packet).await;
                    self.s2c_send.send(packet).await.expect("mspc send fail");
                }
                send = self.c2s_recv.recv() => {
                    // debug!("sending packet");

                    let packet = send.expect("mpsc channel closed");

                    self.handle_c2s_internal(&packet);
                    self.send_packet(packet).await;
                }
            };
        }
    }

    async fn handle_s2c_internal(&mut self, packet: &Packet) {
        trace!("{:?}", packet);

        match packet {
            Packet::EncryptionRequest(p) => {
                let mut rng = rsa::rand_core::OsRng;

                let server_public_key = RsaPublicKey::from_public_key_der(&p.public_key.data)
                    .expect("invalid server public key");

                // shared secret = aes256 key
                let mut shared_secret = [0u8; 16];
                rng.fill_bytes(&mut shared_secret);

                // Auth
                if p.should_authenticate
                    && let Some(profile) = &self.profile
                {
                    debug!("attempting auth");
                    profile
                        .join_server(&p.server_id, &shared_secret, &p.public_key.data)
                        .await
                        .expect("failed authentication");
                }

                let enc_shared_secret = server_public_key
                    .encrypt(&mut rng, Pkcs1v15Encrypt, &shared_secret)
                    .unwrap();
                let enc_verify_token = server_public_key
                    .encrypt(&mut rng, Pkcs1v15Encrypt, &p.verify_token.data)
                    .unwrap();

                self.send_packet_now(EncryptionResponse {
                    shared_secret: enc_shared_secret.into(),
                    verify_token: enc_verify_token.into(),
                })
                .await;

                self.set_encryption(&shared_secret);
                debug!("enabling encryption");
            }
            Packet::SetCompression(p) => {
                self.set_compression(CompressionInfo {
                    threshold: p.theshold.0 as u32,
                    level: COMPRESSION,
                });

                debug!("enabling compression, threshold = {}", p.theshold.0)
            }
            Packet::LoginSuccess(_) => {
                self.state = State::Configuration;
                self.send_packet_now(LoginAcknowledged {}).await;
                debug!("switching state to configuration")
            }
            Packet::PingPlay(p) => {
                self.send_packet_now(PongPlay { id: p.id }).await;
            }
            Packet::PingConfiguration(p) => {
                self.send_packet_now(PongConfiguration { id: p.id }).await
            }
            Packet::ClientboundPluginMessageConfiguration(p) => {
                // TODO: implement enum for the packet
                if p.channel.to_string() == "minecraft:register"
                    || p.channel.to_string() == "minecraft:unregister"
                {
                    debug!(
                        "s2c message: {:?}",
                        str::from_utf8(&p.data.0)
                            .unwrap()
                            .split("\u{0000}")
                            .collect::<Vec<&str>>()
                    )
                }
                if p.channel.to_string() == "minecraft:brand" {
                    debug!("s2c message: {:?}", str::from_utf8(&p.data.0).unwrap())
                }
            }
            Packet::ClientboundKnownPacks(p) => {
                self.send_packet_now(ServerboundKnownPacks {
                    known_packs: p.known_packs.clone(),
                })
                .await;
            }
            Packet::FinishConfiguration(_) => {
                self.send_packet_now(AcknowledgeFinishConfiguration {})
                    .await;
                self.state = State::Play;
                debug!("switching state to play");
            }
            Packet::ClientboundKeepAliveConfiguration(p) => {
                self.send_packet_now(ServerboundKeepAliveConfiguration {
                    keep_alive_id: p.keep_alive_id,
                })
                .await;
            }
            Packet::ClientboundKeepAlivePlay(p) => {
                self.send_packet_now(ServerboundKeepAlivePlay {
                    keep_alive_id: p.keep_alive_id,
                })
                .await;
            }
            _ => {}
        }
    }

    fn parse_raw_packet(&mut self, p: RawPacket) -> Packet {
        packet_by_id(
            self.state,
            Direction::Clientbound,
            p.id,
            &mut &p.payload[..],
        )
        .unwrap()
    }

    fn handle_c2s_internal(&mut self, packet: &Packet) {
        if let Packet::Handshake(p) = packet {
            self.state = p.intent.into()
        }
    }

    async fn send_packet(&mut self, packet: Packet) {
        trace!("{:?}", packet);

        let mut packet_buf = Vec::new();
        packet.write(&mut packet_buf).unwrap();
        self.send_packet_now_data(&packet_buf).await;
    }

    async fn send_packet_now<P: PacketType>(&mut self, packet: P) {
        trace!("{:?}", packet);

        let mut packet_buf = Vec::new();
        packet.write(&mut packet_buf).unwrap();
        self.send_packet_now_data(&packet_buf).await;
    }

    async fn send_packet_now_data(&mut self, packet: &[u8]) {
        self.network_writer
            .write_packet(packet)
            .await
            .expect("failed to send packet data")
    }

    fn set_encryption(&mut self, shared_secret: &[u8]) {
        let crypt_key: [u8; 16] = shared_secret
            .try_into()
            .expect("shared secret wrong length");
        self.network_reader.set_encryption(&crypt_key);
        self.network_writer.set_encryption(&crypt_key);
    }

    fn set_compression(&mut self, compression: CompressionInfo) {
        if compression.level > 9 {
            panic!("invalid compression level")
        }

        self.network_reader
            .set_compression(compression.threshold as usize);

        self.network_writer
            .set_compression((compression.threshold as usize, compression.level));
    }
}

/// We have this in a separate struct so we can use it outside of the config.
pub struct CompressionInfo {
    /// The compression threshold used when compression is enabled.
    pub threshold: u32,
    /// A value between `0..9`.
    /// `1` = Optimize for the best speed of encoding.
    /// `9` = Optimize for the size of data being encoded.
    pub level: u32,
}
