use std::time::Duration;

use mini_moka::sync::Cache;
use uuid::Uuid;

use crate::config::scene::Scene;

const MAXIMUM_CACHE_SIZE: u16 = 1000;
const MAXIMUM_CACHE_DURATION: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct CurrentSceneEntry {
  pub scene: Scene,
  pub brightness: Option<f32>,
  pub on: bool
}

impl CurrentSceneEntry {
  pub fn new(scene: Scene, brightness: Option<f32>, on: bool) -> Self {
    Self { scene, brightness, on}
  }
}

pub type CurrentSceneCache = Cache<Uuid, CurrentSceneEntry>;

pub fn new_cache() -> CurrentSceneCache {
  Cache::builder()
    .max_capacity(MAXIMUM_CACHE_SIZE.into())
    .time_to_live(MAXIMUM_CACHE_DURATION)
    .time_to_idle(MAXIMUM_CACHE_DURATION)
    .build()
}
