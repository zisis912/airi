use mc_rust_protocol::{Identifier, packet::WaypointData};

#[derive(Default)]
pub struct WorldTime {
    pub world_age: i64,
    pub time_of_day: i64,
    pub daylight_cycle: bool,
}

#[derive(Clone)]
pub struct Waypoint {
    pub icon_style: Identifier,
    pub color: Option<(u8, u8, u8)>,
    pub waypoint: WaypointData,
}
