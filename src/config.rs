//! Local configuration (router credentials, phone number, cloud session).

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_ip")]
    pub router_ip: String,
    #[serde(default)]
    pub router_pwd_b64: String,
    #[serde(default)]
    pub mobile: String,
    #[serde(default = "default_sn")]
    pub sn: String,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub avatar_url: String,
}

fn default_ip() -> String {
    String::new()
}
fn default_sn() -> String {
    String::new()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            router_ip: default_ip(),
            router_pwd_b64: String::new(),
            mobile: String::new(),
            sn: default_sn(),
            session: String::new(),
            user: String::new(),
            avatar_url: String::new(),
        }
    }
}

impl Config {
    pub fn password(&self) -> String {
        if self.router_pwd_b64.is_empty() {
            return String::new();
        }
        B64.decode(&self.router_pwd_b64)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default()
    }

    pub fn set_password(&mut self, pwd: &str) {
        self.router_pwd_b64 = B64.encode(pwd.as_bytes());
    }
}

fn config_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".uu-gui")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load() -> Config {
    let path = config_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &Config) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("config.json"), text).map_err(|e| e.to_string())
}
