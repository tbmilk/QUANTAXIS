use std::{fs, path::Path};
use log::debug;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Config {
    pub common: Common,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path = path.as_ref();
        debug!("Reading configuration from {}", path.display());
        let data = fs::read_to_string(path).map_err(|e| e.to_string())?;
        toml::from_str(&data).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct MongoConfig {
    pub uri: String,
    pub db: String,
}

impl Default for MongoConfig {
    fn default() -> Self {
        Self {
            uri: "mongodb://localhost:27017".to_owned(),
            db: String::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct MQConfig {
    pub uri: String,
    pub exchange: String,
    pub routing_key: String,
}

impl Default for MQConfig {
    fn default() -> Self {
        Self {
            uri: "amqp://admin:admin@localhost:5672/".to_owned(),
            exchange: String::new(),
            routing_key: String::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Common {
    pub log_level: String,
    pub account: String,
    pub password: String,
    pub broker: String,
    pub wsuri: String,
    pub eventmq_ip: String,
    pub database_ip: String,
    pub ping_gap: i32,
    pub taskid: String,
    pub portfolio: String,
    pub bank_password: String,
    pub capital_password: String,
    pub appid: String,
}
