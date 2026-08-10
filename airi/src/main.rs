use std::{collections::HashMap, error::Error, process, time::Duration};

use airi_protocol::{
    Identifier, TextComponent, UUID, VarInt, Vec3, nbt,
    packet::{
        Intent, PROTOCOL_VERSION, Packet, XorY,
        c2s::{handshake::Handshake, login::LoginStart},
    },
};
use log::{debug, warn};

use tokio::{
    net::TcpStream,
    select,
    sync::mpsc::{self, Receiver, Sender},
    task::spawn_blocking,
    time,
};
use winit::event_loop::EventLoop;

use crate::{
    auth::AuthError,
    entity::Entity,
    network_handler::NetworkHandler,
    render::App,
    world_types::{Waypoint, WorldTime},
};

mod asset_gen;
mod auth;
mod entity;
mod network_handler;
mod render;
mod world_types;

const HOST: &str = "localhost";
const PORT: u16 = 30000;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    // tokio is fucking stupid
    spawn_blocking(asset_gen::get_assets).await.unwrap();

    let address = format!("{}:{}", HOST, PORT);
    let stream = TcpStream::connect(address).await?;
    stream.set_nodelay(false)?; //disable nagle

    debug!("connected to server");

    Client::new(stream).await.start().await?;
    Ok(())
}

struct Client {
    sender: Sender<Packet>,
    receiver: Receiver<Packet>,
    entities: HashMap<i32, Entity>,
    registry: HashMap<Identifier, HashMap<Identifier, Option<nbt::Tag>>>,
    // tags: HashMap<i32, Entity>,
    // player: Player,
    time: WorldTime,
    waypoints: HashMap<XorY<UUID, String>, Waypoint>,
}

impl Client {
    async fn new(tcp_stream: TcpStream) -> Self {
        let (receive_tx, receive_rx) = mpsc::channel(128);
        let (send_tx, send_rx) = mpsc::channel(128);
        let mut network_handler = NetworkHandler::new(tcp_stream, send_rx, receive_tx, None);

        // network thread
        tokio::spawn(async move {
            network_handler.network_loop().await;
        });

        Client {
            sender: send_tx,
            receiver: receive_rx,
            entities: HashMap::new(),
            registry: HashMap::new(),
            // tags: HashMap::new(),
            // player: Player::default(),
            time: WorldTime::default(),
            waypoints: HashMap::new(),
        }
    }

    async fn start(mut self) -> Result<(), Box<dyn Error>> {
        let mut client_ticker = time::interval(Duration::from_millis(50));

        // send handshake packets
        self.sender
            .send(Packet::Handshake(Handshake {
                server_address: HOST.to_owned(),
                server_port: PORT,
                protocol_version: VarInt(PROTOCOL_VERSION),
                intent: Intent::Login,
            }))
            .await?;
        self.sender
            .send(Packet::LoginStart(LoginStart {
                name: "bot1sds23".to_owned(),
                player_uuid: UUID(343434343),
            }))
            .await?;

        // build event loop
        let event_loop = EventLoop::with_user_event()
            .build()
            .expect("couldnt build event loop");
        let mut app = App::new().await.expect("couldnt create window");

        // tick thread
        tokio::spawn(async move {
            loop {
                select! {
                    packet = self.receiver.recv() => self.handle_packet(&packet.expect("channel closed")).await,
                    _ = client_ticker.tick() => self.client_tick(),

                }
            }
        });

        // start event loop (block thread)
        event_loop.run_app(&mut app).unwrap();

        Ok(())
    }

    fn client_tick(&self) {}

    fn login(&self) -> Result<(), AuthError> {
        unimplemented!()
    }

    fn disconnect(&self, reason: Option<&TextComponent>, r2: Option<&String>) {
        warn!("disconnected: {:?} {:?}", reason, r2);
        process::exit(1)
    }

    async fn handle_packet(&mut self, packet: &Packet) {
        match packet {
            Packet::SpawnEntity(p) => {
                let entity = Entity {
                    uuid: p.entity_uuid,
                    ty: p.ty.0,
                    pos: p.position,
                    velocity: p.velocity.0,
                    pitch: p.pitch,
                    yaw: p.yaw,
                    head_yaw: p.head_yaw,
                    on_ground: false,
                };
                self.entities.insert(p.entity_id.0, entity);
            }
            Packet::UpdateEntityPosition(p) => {
                let entity = self.entities.get_mut(&p.entity_id.0).unwrap();
                entity.pos = entity.pos.offset(Vec3 {
                    x: p.delta.x as f64 / 4096.,
                    y: p.delta.y as f64 / 4096.,
                    z: p.delta.z as f64 / 4096.,
                });
                entity.on_ground = p.on_ground;
            }
            Packet::UpdateEntityPositionAndRotation(p) => {
                let entity = self.entities.get_mut(&p.entity_id.0).unwrap();
                entity.pos = entity.pos.offset(Vec3 {
                    x: p.delta.x as f64 / 4096.,
                    y: p.delta.y as f64 / 4096.,
                    z: p.delta.z as f64 / 4096.,
                });
                entity.on_ground = p.on_ground;
                entity.yaw = p.yaw;
                entity.pitch = p.pitch;
            }
            Packet::UpdateEntityRotation(p) => {
                let entity = self.entities.get_mut(&p.entity_id.0).unwrap();
                entity.yaw = p.yaw;
                entity.pitch = p.pitch;
            }
            Packet::SetHeadRotation(p) => {
                let entity = self.entities.get_mut(&p.entity_id.0).unwrap();
                entity.head_yaw = p.head_yaw;
            }
            Packet::LoginDisconnect(p) => self.disconnect(None, Some(&p.reason)),
            Packet::DisconnectConfiguration(p) => self.disconnect(Some(&p.reason), None),
            Packet::DisconnectPlay(p) => self.disconnect(Some(&p.reason), None),
            Packet::RegistryData(p) => {
                let registry = self.registry.entry(p.registry_id.clone()).or_default();

                for entry in &p.entries.data {
                    registry.insert(entry.entry_id.clone(), entry.data.clone());
                }
            }
            Packet::UpdateTime(p) => {
                // self.time.time_of_day = p.time_of_day;
                // self.time.world_age = p.world_age;
                // self.time.daylight_cycle = p.time_of_day_increasing;
            }
            Packet::Waypoint(p) => {
                // match p.operation.0 {
                //     0 | 2 => self.waypoints.insert(
                //         p.identifier.clone(),
                //         Waypoint {
                //             icon_style: p.icon_style.clone(),
                //             color: p.color,
                //             waypoint: p.waypoint.clone(),
                //         },
                //     ),
                //     1 => self.waypoints.remove(&p.identifier),
                //     _ => panic!("invalid waypoint operation"),
                // };
            }
            _ => {}
        }
    }
}
