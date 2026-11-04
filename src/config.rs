use crate::paths;
use crate::util::{load_json, write_json};
use serde::{Deserialize, Serialize};
use std::{boxed::Box, error::Error};

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Config {
    pub url: String,
    pub id: String,
    pub username: String,
    pub password: String,
    pub key: Option<String>,
}

impl Config {
    pub fn new() -> Self {
        Config::default()
    }
    /// Loads `Config` from `<config dir>/signage.json` (see `paths::config_dir`).
    pub async fn load(&mut self) -> Result<(), Box<dyn Error>> {
        println!("Reading signage.json: ");
        load_json(self, &paths::config_dir()?, paths::CONFIG_FILE).await
    }

    /// Writes `Config` to `<config dir>/signage.json` (see `paths::config_dir`).
    pub async fn write(&self) -> Result<(), Box<dyn Error>> {
        println!("Writing to signage.json");
        write_json(
            self,
            &format!("{}/{}", paths::config_dir()?, paths::CONFIG_FILE),
        )
        .await
    }
}
