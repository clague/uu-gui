//! Reyee router admin API: the UU master switch (`devSta` + `uu_acc`).
//!
//! Blocking HTTP via `ureq` (short timeout — LAN host); call sites run these
//! on their own std thread (see `Task::blocking` in main.rs).

use crate::crypto::*;
use serde_json::{json, Value};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::OnceLock;

pub const LOGIN_KEY: &str = "6fd25f2e06efd8169ed99389868ea1bc";

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(8))
            .build()
    })
}

fn salt8() -> [u8; 8] {
    // OS-seeded randomness (RandomState is seeded from the OS), enough for a salt.
    let mut a = RandomState::new().build_hasher();
    a.write_u64(unix_millis() as u64);
    let x = a.finish();
    let mut b = RandomState::new().build_hasher();
    b.write_u64(x);
    let y = b.finish();
    let mut s = [0u8; 8];
    s[..8].copy_from_slice(&x.to_le_bytes());
    s[4..].copy_from_slice(&y.to_le_bytes()[..4]);
    s
}

fn post(ip: &str, path: &str, sid: Option<&str>, body: Value) -> Result<String, String> {
    let mut url = format!("http://{}{}", ip, path);
    if let Some(s) = sid {
        url.push_str("?auth=");
        url.push_str(s);
    }
    match agent()
        .post(&url)
        .set("content-type", "application/json")
        .send_string(&body.to_string())
    {
        Ok(resp) => resp.into_string().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(_, resp)) => {
            resp.into_string().map_err(|e| e.to_string())
        }
        Err(ureq::Error::Transport(e)) => Err(format!("network: {}", e)),
    }
}

#[derive(Debug, Clone, Default)]
pub struct RouterStatus {
    pub appver: String,
    pub acc: String,
    pub enable: String,
}

impl RouterStatus {
    pub fn enabled(&self) -> bool {
        self.enable == "true"
    }
    pub fn summary(&self) -> String {
        format!(
            "appver={}  acc={}  enable={}",
            self.appver, self.acc, self.enable
        )
    }
}

pub fn login(ip: &str, password: &str) -> Result<String, String> {
    let enc = gibberish_encrypt(password, LOGIN_KEY, &salt8());
    let body = json!({
        "method": "login",
        "params": {
            "username": "admin",
            "time": unix_secs(),
            "encry": true,
            "pwd": enc
        }
    });
    let raw = post(ip, "/cgi-bin/luci/api/auth", None, body)?;
    if let Some(sid) = extract_sid(&raw) {
        return Ok(sid);
    }
    // fallback: plaintext login
    let body2 = json!({
        "method": "login",
        "params": {"username": "admin", "password": password}
    });
    let raw2 = post(ip, "/cgi-bin/luci/api/auth", None, body2)?;
    extract_sid(&raw2).ok_or_else(|| format!("login failed: {}", raw))
}

fn extract_sid(raw: &str) -> Option<String> {
    let v: Value = serde_json::from_str(raw).ok()?;
    if let Some(s) = v.get("sid").and_then(|s| s.as_str()) {
        return Some(s.to_string());
    }
    v.get("data")
        .and_then(|d| d.get("sid"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

fn run_cmd(ip: &str, sid: &str, method: &str, mut params: Value) -> Result<String, String> {
    if params.get("device").is_none() {
        params["device"] = json!("pc");
    }
    let body = json!({"method": method, "params": params});
    post(ip, "/cgi-bin/luci/api/cmd", Some(sid), body)
}

pub fn get_status(ip: &str, sid: &str) -> Result<RouterStatus, String> {
    let raw = run_cmd(
        ip,
        sid,
        "devSta.get",
        json!({"module":"uu_acc","data":{"option":"onoff"}}),
    )?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("{}: {}", e, raw))?;
    let d = v.get("data").cloned().unwrap_or(v.clone());
    Ok(RouterStatus {
        appver: d.get("appver").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        acc: d.get("acc").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        enable: d.get("enable").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    })
}

pub fn set_enable(ip: &str, sid: &str, enable: bool) -> Result<(), String> {
    let v = if enable { "true" } else { "false" };
    run_cmd(
        ip,
        sid,
        "devSta.set",
        json!({"module":"uu_acc","data":{"option":"onoff","enable":v}}),
    )?;
    Ok(())
}
