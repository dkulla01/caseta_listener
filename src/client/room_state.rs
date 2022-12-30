use std::time::Duration;

use mini_moka::sync::Cache;
use uuid::Uuid;

use crate::config::scene::Scene;

const MAXIMUM_CACHE_SIZE: u16 = 1000;
const MAXIMUM_CACHE_DURATION: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct CurrentRoomState {
    pub scene: Option<Scene>,
    pub brightness: Option<f32>,
    pub on: bool,
}

impl CurrentRoomState {
    pub fn new(scene: Option<Scene>, brightness: Option<f32>, on: bool) -> Self {
        Self {
            scene,
            brightness,
            on,
        }
    }
}

pub type CurrentRoomStateCache = Cache<Uuid, CurrentRoomState>;

pub fn new_cache() -> CurrentRoomStateCache {
    Cache::builder()
        .max_capacity(MAXIMUM_CACHE_SIZE.into())
        .time_to_live(MAXIMUM_CACHE_DURATION)
        .time_to_idle(MAXIMUM_CACHE_DURATION)
        .build()
}
