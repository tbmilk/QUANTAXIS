use crate::qamarket::qamdgateway::error::{GatewayError, GatewayResult};
use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// 数据源配置（CTP/QQ/Sina等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub name: String,
    /// 接入地址，如 "tcp://180.168.146.187:10131"
    pub front_addr: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub broker_id: String,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub auth_code: String,
    /// 数据源类型："ctp" / "qq" / "sina"
    pub source_type: Option<String>,
}

/// WebSocket 服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// REST API 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestApiConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub cors: CorsConfig,
}

/// CORS 配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CorsConfig {
    #[serde(default)]
    pub allow_all: bool,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    #[serde(default)]
    pub allow_credentials: bool,
}

/// 订阅配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriptionConfig {
    /// 启动时默认订阅的合约
    #[serde(default)]
    pub default_instruments: Vec<String>,
}

/// 网关整体配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub brokers: HashMap<String, BrokerConfig>,
    pub default_broker: String,
    pub websocket: WebSocketConfig,
    pub rest_api: RestApiConfig,
    #[serde(default)]
    pub subscription: SubscriptionConfig,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> GatewayResult<Self> {
        let mut file = File::open(path).map_err(GatewayError::IoError)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(GatewayError::IoError)?;
        serde_json::from_str(&contents).map_err(GatewayError::JsonError)
    }

    pub fn get_broker(&self, broker_name: Option<&str>) -> GatewayResult<&BrokerConfig> {
        let name = broker_name.unwrap_or(&self.default_broker);
        self.brokers.get(name).ok_or_else(|| {
            GatewayError::ConfigError(format!("Broker config not found: {}", name))
        })
    }

    pub fn load() -> GatewayResult<Self> {
        if let Ok(json) = env::var("QAMDGATEWAY_CONFIG") {
            return serde_json::from_str(&json).map_err(GatewayError::JsonError);
        }
        let path = env::var("QAMDGATEWAY_CONFIG_PATH").unwrap_or_else(|_| {
            for p in ["./config.json", "./qamdgateway.json"] {
                if Path::new(p).exists() {
                    return p.to_string();
                }
            }
            "./config.json".to_string()
        });
        Self::from_file(path)
    }
}
