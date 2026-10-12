//! UU (网易UU) cloud API client: login, devices, status, games.
//!
//! Blocking HTTP via `ureq`; call sites run these on their own std thread
//! (see `Task::blocking` in main.rs).

use crate::crypto::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

pub const HOST: &str = "https://router.uu.163.com";

/// md5(hex(APK signing cert)) - constant for the official app.
pub const SIGNING_KEY: &str = "60c3ccbe5ad4748a579fa80fef7161a6";
/// Device id (AES key) recovered from the `EAD` header.
pub const DEVICE_ID: &str = "03d0a98824ae0e37";
pub const EAD: &str = "25f2c10046cg2g59";

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(25))
            .build()
    })
}

fn common_headers() -> Vec<(&'static str, String)> {
    vec![
        ("locale", "zh_CN".into()),
        ("operator", "46001".into()),
        ("rn-version", "242".into()),
        ("product", "dm3qzcx".into()),
        ("systemtype", "android".into()),
        ("systemversion", "36".into()),
        ("imei", "".into()),
        ("appversion", "640".into()),
        ("screendpi", "600".into()),
        ("manufacturer", "samsung".into()),
        ("uubarcommit", "a60b9ae".into()),
        ("screensize", "6.7809258".into()),
        ("root", "0".into()),
        ("connectmode", "domain".into()),
        ("channel", "qq".into()),
        ("oaid", "".into()),
        ("ead", EAD.into()),
        ("brand", "samsung".into()),
        ("rom", "BP4A.251205.006.S9180ZCS8FZI1".into()),
        ("bundleid", "com.netease.uurouter".into()),
        ("appversioncode", "6.5.0".into()),
        ("model", "SM-S9180".into()),
        ("resolution", "1440x3086".into()),
    ]
}

fn cgi_headers() -> Vec<(&'static str, String)> {
    vec![
        ("Client-Origin", "3001".into()),
        ("AppVersion", "10.6.1".into()),
        ("RN-Version", "10601".into()),
    ]
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Compute the request signature.
pub fn sign(params: &str, seed: &str, post_content: Option<&str>) -> String {
    let mut buf = String::new();
    if !params.is_empty() {
        buf.push_str(params);
        buf.push('&');
    }
    buf.push_str("seed=");
    buf.push_str(seed);
    buf.push_str("&key=");
    buf.push_str(SIGNING_KEY);
    if let Some(pc) = post_content {
        buf.push_str("&post_content=");
        buf.push_str(&urlencode(pc));
    }
    md5_hex(buf.as_bytes()).to_uppercase()
}

/// Take a `ureq` result, treating any HTTP status as data (the legacy CGI
/// endpoints reply with encrypted bodies even on 4xx; the old reqwest code
/// ignored status codes entirely).
fn finish(resp: Result<ureq::Response, ureq::Error>) -> Result<(Option<String>, String), String> {
    let resp = match resp {
        Ok(resp) => resp,
        Err(ureq::Error::Status(_, resp)) => resp,
        Err(ureq::Error::Transport(e)) => return Err(format!("network: {}", e)),
    };
    let ntes = resp.header("Ntes-UU").map(|s| s.to_string());
    let body = resp
        .into_string()
        .map(|s| s.lines().collect::<String>())
        .unwrap_or_default();
    let body = aes_ecb_decrypt_b64(DEVICE_ID.as_bytes(), &body).unwrap_or(body);
    Ok((ntes, body))
}

/// Signed, encrypted `/app/*` call. Returns (response `Ntes-UU` header, decrypted body).
pub fn app_call(
    path: &str,
    obj: Option<Value>,
    sess: Option<&str>,
) -> Result<(Option<String>, String), String> {
    let plain = obj.map(|o| serde_json::to_string(&o).unwrap_or_default());
    let seed = unix_secs();
    let sig = sign("", &seed, plain.as_deref());
    let mut req = agent().post(&format!("{}{}", HOST, path));
    for (k, v) in common_headers() {
        req = req.set(k, &v);
    }
    req = req
        .set("content-type", "text/plain; charset=utf-8")
        .set("seed", &seed)
        .set("sign", &sig);
    if let Some(s) = sess {
        req = req.set("Ntes-UU", s);
    }
    let resp = match plain {
        Some(p) => req.send_string(&aes_ecb_encrypt_b64(DEVICE_ID.as_bytes(), &p)),
        None => req.send_string(""),
    };
    finish(resp)
}

/// `/api/*` call with the minimal header set. Returns (response `Ntes-UU`
/// header, plaintext body).
pub fn api_call(
    method: &str,
    path: &str,
    obj: Option<Value>,
    sess: Option<&str>,
    router_sess: Option<&str>,
) -> Result<(Option<String>, String), String> {
    let mut url = format!("{}{}", HOST, path);
    if method == "GET" {
        url.push(if path.contains('?') { '&' } else { '?' });
        url.push_str(&format!("t={}", unix_millis()));
    }
    let mut req = if method == "GET" {
        agent().get(&url)
    } else {
        agent().post(&url)
    };
    for (k, v) in cgi_headers() {
        req = req.set(k, &v);
    }
    req = req.set("content-type", "application/json");
    if let Some(s) = sess {
        req = req.set("UU-SESS-ID", s).set("Ntes-Uu", s);
    }
    if let Some(rs) = router_sess {
        req = req.set("sess", rs);
    }
    let resp = match obj {
        Some(o) => req.send_string(&o.to_string()),
        None => req.call(),
    };
    // /api/* bodies are plain text/JSON.
    let resp = match resp {
        Ok(resp) => resp,
        Err(ureq::Error::Status(_, resp)) => resp,
        Err(ureq::Error::Transport(e)) => return Err(format!("network: {}", e)),
    };
    let ntes = resp.header("Ntes-UU").map(|s| s.to_string());
    let body = resp
        .into_string()
        .map(|s| s.lines().collect::<String>())
        .unwrap_or_default();
    Ok((ntes, body))
}

/// Fetch account profile (nickname + avatar URL) using the current session.
pub fn userinfo(sess: &str) -> Result<(String, String), String> {
    let (_, raw) = api_call("GET", "/api/userinfo", None, Some(sess), None)?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let name = v
        .get("nickname")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let face = v
        .get("face_url")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Ok((name, face))
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeviceInfo {
    #[serde(default)]
    pub mac: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub acced: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Device {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub deviceinfo: DeviceInfo,
    #[serde(default)]
    pub custom: Value,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DeviceGroup {
    #[serde(default)]
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AllDevices {
    #[serde(default)]
    pub all_devices: Vec<DeviceGroup>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Game {
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub game_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Zone {
    #[serde(default)]
    pub zone_id: String,
    #[serde(default)]
    pub zone_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Server {
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub server_name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GameList {
    #[serde(default)]
    pub gamelist: HashMap<String, HashMap<String, Vec<Game>>>,
    #[serde(default)]
    pub zonelist: HashMap<String, Vec<Zone>>,
    #[serde(default)]
    pub serverlist: HashMap<String, Vec<Server>>,
}

impl GameList {
    pub fn flat(&self) -> Vec<Game> {
        let mut out = Vec::new();
        for buckets in self.gamelist.values() {
            for games in buckets.values() {
                for g in games {
                    if !g.game_name.is_empty() {
                        out.push(g.clone());
                    }
                }
            }
        }
        out.sort_by(|a, b| a.game_name.cmp(&b.game_name));
        out.dedup_by(|a, b| a.game_id == b.game_id);
        out
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModeSetting {
    #[serde(default)]
    pub game_name: String,
    #[serde(default)]
    pub zone_name: String,
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub validate: Value,
}

impl ModeSetting {
    pub fn is_valid(&self) -> bool {
        self.validate
            .get("valid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GameModeSetting {
    #[serde(default)]
    pub device_setting: HashMap<String, ModeSetting>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Status {
    #[serde(default)]
    pub game_mode_setting: GameModeSetting,
}

// ---- high level operations ----

pub fn auth() -> Result<String, String> {
    let (sess, _) = app_call("/app/auth", None, None)?;
    sess.ok_or_else(|| "no session header".into())
}

pub fn send_code(sess: &str, mobile: &str) -> Result<String, String> {
    let (_, body) = app_call(
        "/app/code",
        Some(json!({"country_code":"86","mobile":mobile,"type":0,"for":0,"times":1})),
        Some(sess),
    )?;
    Ok(body)
}

pub fn login(sess: &str, mobile: &str, code: &str) -> Result<Value, String> {
    let (_, body) = app_call(
        "/app/account/login",
        Some(json!({"country_code":"86","mobile":mobile,"code":code})),
        Some(sess),
    )?;
    serde_json::from_str(&body).map_err(|e| format!("{}: {}", e, body))
}

pub fn get_router_sess(sn: &str) -> Result<String, String> {
    let (_, body) = api_call(
        "POST",
        "/api/sess",
        Some(json!({"type":"ruijie","sn":sn})),
        None,
        None,
    )?;
    let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    v.get("sess")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| body)
}

pub fn bind_router(sess: &str, router_sess: &str) -> Result<(), String> {
    let _ = api_call(
        "POST",
        "/api/login-uu-sess",
        Some(json!({"uu_sess_id": sess})),
        Some(sess),
        Some(router_sess),
    )?;
    Ok(())
}

pub fn get_devices(sess: &str, router_sess: Option<&str>) -> Result<AllDevices, String> {
    let (_, body) = api_call(
        "GET",
        "/api/devicesetting/all-devices",
        None,
        Some(sess),
        router_sess,
    )?;
    if body.contains("not_login") || body.contains("invalid_sess") {
        return Err(format!("会话无效，请重新登录 ({})", body.trim()));
    }
    serde_json::from_str(&body).map_err(|e| format!("{}: {}", e, body))
}

pub fn get_status(sess: &str, router_sess: &str) -> Result<Status, String> {
    let (_, body) = api_call("GET", "/api/status", None, Some(sess), Some(router_sess))?;
    serde_json::from_str(&body).map_err(|e| format!("{}: {}", e, body))
}

/// Turn a CGI response body into a friendly error when it reports a failure.
fn check_status(body: &str) -> Result<String, String> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Ok(body.to_string());
    };
    let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("");
    if status.is_empty() || status == "ok" || status == "acced" || status == "success" {
        return Ok(body.to_string());
    }
    let msg = v
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .trim();
    let friendly = match status {
        "exceed_max_amount_mobile" => {
            "移动端同时只能加速 1 台（已有其他手机在加速）。请先停止它，或在第二台手机上用 UU App 自行加速。"
        }
        "exceed_max_amount" => "同时加速的设备数已达上限（本账号最多 2 台）。",
        "not_login" | "invalid_sess" => "登录状态已失效，请重新登录 UU 账号。",
        _ => "",
    };
    if !friendly.is_empty() {
        return Err(friendly.to_string());
    }
    if msg.is_empty() {
        Err(format!("操作失败 ({})", status))
    } else {
        Err(msg.to_string())
    }
}

pub fn auto_acc(
    sess: &str,
    router_sess: &str,
    info: &DeviceInfo,
    on: bool,
) -> Result<String, String> {
    let (_, body) = api_call(
        "POST",
        "/api/devicesetting/auto-acc",
        Some(json!({"deviceinfo": info, "auto_acc": on})),
        Some(sess),
        Some(router_sess),
    )?;
    check_status(&body)
}

/// Start accelerating a device now.
pub fn start_acc(sess: &str, router_sess: &str, info: &DeviceInfo) -> Result<String, String> {
    let (_, body) = api_call(
        "POST",
        "/api/acc",
        Some(json!({"deviceinfo": info})),
        Some(sess),
        Some(router_sess),
    )?;
    check_status(&body)
}

/// Stop accelerating a device now.
pub fn stop_acc(sess: &str, router_sess: &str, info: &DeviceInfo) -> Result<String, String> {
    let (_, body) = api_call(
        "POST",
        "/api/stop-acc",
        Some(json!({"deviceinfo": info})),
        Some(sess),
        Some(router_sess),
    )?;
    check_status(&body)
}

pub fn gamelist(sess: &str, platform: &str) -> Result<GameList, String> {
    let (_, body) = api_call(
        "GET",
        &format!("/api/gamelist?platform={}", platform),
        None,
        Some(sess),
        None,
    )?;
    serde_json::from_str(&body).map_err(|e| format!("{}: {}", e, body))
}

pub fn set_game(
    sess: &str,
    router_sess: &str,
    key: &str,
    game: &Game,
    zone: Option<&Zone>,
    server: Option<&Server>,
) -> Result<String, String> {
    let mut entry = json!({
        "key": key,
        "key_type": "device",
        "acc_mode": "game",
        "game_id": game.game_id,
        "game_name": game.game_name,
    });
    match zone {
        Some(z) => {
            entry["zone_id"] = json!(z.zone_id);
            entry["zone_name"] = json!(z.zone_name);
            entry["zone_auto"] = json!(false);
            match server {
                Some(s) => {
                    entry["server_id"] = json!(s.server_id);
                    entry["server_name"] = json!(s.server_name);
                    entry["server_auto"] = json!(false);
                }
                None => entry["server_auto"] = json!(true),
            }
        }
        None => {
            entry["zone_auto"] = json!(true);
            entry["server_auto"] = json!(true);
        }
    }
    let (_, body) = api_call(
        "POST",
        "/api/acc-mode-setting",
        Some(json!({"acc_mode":"game","game_mode_setting":[entry]})),
        Some(sess),
        Some(router_sess),
    )?;
    if let Ok(v) = serde_json::from_str::<Value>(&body) {
        if let Some(arr) = v.get("game_mode_setting").and_then(|a| a.as_array()) {
            for e in arr {
                let valid = e
                    .get("validate")
                    .and_then(|x| x.get("valid"))
                    .and_then(|b| b.as_bool());
                if valid == Some(false) {
                    let reason = e
                        .get("validate")
                        .and_then(|x| x.get("reason"))
                        .and_then(|r| r.as_str())
                        .unwrap_or("invalid");
                    return Err(match reason {
                        "invalid_server" => "所选服务器无效，请重新选择区域/服务器".into(),
                        other => format!("游戏设置无效 ({})", other),
                    });
                }
            }
        }
    }
    check_status(&body)
}
