use crate::paths;
use crate::util::{load_json, write_json, Video};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{boxed::Box, error::Error};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Data {
    pub videos: Vec<Video>,
    pub last_update: Option<DateTime<Utc>>,
    pub current_playlist: Option<Uuid>,
    pub active_schedule_ends: Option<DateTime<Utc>>,
    pub next_schedule_starts: Option<DateTime<Utc>>,
    pub next_playlist_id: Option<Uuid>,
    pub fallback_playlist_id: Option<Uuid>,
    pub update_content: Option<bool>,
}
impl Data {
    pub fn new() -> Self {
        Data::default()
    }

    /// Loads `Data` from `<data dir>/data.json` (see `paths::data_dir`).
    pub async fn load(&mut self) -> Result<(), Box<dyn Error>> {
        println!("Reading data.json: ");
        load_json(self, &paths::data_dir()?, paths::DATA_FILE).await
    }
    /// Writes `Data` to `<data dir>/data.json` (see `paths::data_dir`).
    pub async fn write(&self) -> Result<(), Box<dyn Error>> {
        println!("Writing to data.json:");
        write_json(
            self,
            &format!("{}/{}", paths::data_dir()?, paths::DATA_FILE),
        )
        .await
    }
}
