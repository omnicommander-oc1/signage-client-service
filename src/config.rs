use crate::paths;
use crate::util::{load_json, write_json};
use serde::{Deserialize, Serialize};
use std::{boxed::Box, error::Error};

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Config {
    pub url: String,
    pub id: String,
    /// Shared fleet Basic-auth user, only used by `signaged` for `get-new-key`. A device linked
    /// through the claim flow has none (OCS-1061), so it is empty when absent and is not written
    /// back when empty (a legacy file with credentials is written exactly as before).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    /// Shared fleet Basic-auth password; see `username`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn claimed_file_parses_and_round_trips_without_credentials() {
        let claimed = json!({"url": "https://api.example.com", "id": "dev-1", "key": "k1"});
        let config: Config = serde_json::from_slice(claimed.to_string().as_bytes()).unwrap();
        assert_eq!(config.url, "https://api.example.com");
        assert_eq!(config.id, "dev-1");
        assert_eq!(config.username, "");
        assert_eq!(config.password, "");
        assert_eq!(config.key.as_deref(), Some("k1"));

        let written = serde_json::to_value(&config).unwrap();
        assert_eq!(written, claimed, "no username/password invented on write");
    }

    #[test]
    fn legacy_file_parses_and_round_trips_with_the_same_fields() {
        let legacy = json!({
            "url": "https://api.example.com",
            "id": "dev-1",
            "username": "fleet",
            "password": "secret",
            "key": "k1"
        });
        let config: Config = serde_json::from_slice(legacy.to_string().as_bytes()).unwrap();
        assert_eq!(config.username, "fleet");
        assert_eq!(config.password, "secret");
        assert_eq!(config.key.as_deref(), Some("k1"));

        let written = serde_json::to_value(&config).unwrap();
        assert_eq!(written, legacy, "legacy file is written back unchanged");
    }

    #[test]
    fn legacy_file_without_key_still_writes_key_null() {
        let legacy = json!({"url": "u", "id": "i", "username": "fleet", "password": "secret"});
        let config: Config = serde_json::from_value(legacy).unwrap();
        let written = serde_json::to_value(&config).unwrap();
        assert_eq!(written["key"], Value::Null);
        assert_eq!(written["username"], "fleet");
        assert_eq!(written["password"], "secret");
    }
}
