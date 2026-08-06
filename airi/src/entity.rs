use airi_protocol::{Angle, UUID, VarInt, Vec3};

#[derive(Default, Debug)]
pub struct Entity {
    pub uuid: UUID,
    pub ty: i32,
    pub pos: Vec3<f64>,
    pub velocity: Vec3<f64>,
    pub pitch: Angle,
    pub yaw: Angle,
    pub head_yaw: Angle,
    pub on_ground: bool,
    // pub attributes: Vec<EntityAt
}

#[derive(Default, Debug)]
pub struct Player {
    pub entity: Entity,
    pub xp: PlayerXp,
}

#[derive(Default, Debug)]
pub struct PlayerXp {
    pub experience_bar: f64,
    pub level: VarInt,
    pub total_experience: VarInt,
}
