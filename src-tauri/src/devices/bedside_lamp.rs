use crate::config::{self, DeviceSettings};
use aes::Aes128;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        OnceLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

const MIIO_PORT: u16 = 54321;
const LAMP_DEVICE_ID: u32 = 75_416_834;

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct LampConfig {
    pub ip: String,
    pub device_id: u32,
    pub token: String,
}

impl Default for LampConfig {
    fn default() -> Self {
        Self {
            ip: "0.0.0.0".into(),
            device_id: 0,
            token: String::new(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct LampConfigView {
    name: String,
    ip: String,
    device_id: u32,
    has_token: bool,
}

impl LampConfigView {
    fn from_config(name: &str, config: &LampConfig) -> Self {
        Self {
            name: name.into(),
            ip: config.ip.clone(),
            device_id: config.device_id,
            has_token: !config.token.is_empty(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LampConfigInput {
    name: String,
    ip: String,
    device_id: u32,
    token: String,
}

#[derive(Serialize)]
pub(crate) struct LampState {
    power: bool,
    brightness: u8,
    temperature: u16,
}

impl LampConfig {
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.ip
            .parse::<Ipv4Addr>()
            .map_err(|_| "Lamp address must be an IPv4 address")?;
        if !self.token.is_empty() && !valid_token(&self.token) {
            return Err("Lamp token must be 32 hexadecimal characters".into());
        }
        Ok(())
    }

    // Import the token obtained during local setup. It stays in app data, never in the repository.
    #[cfg(target_os = "windows")]
    pub(crate) fn import_existing_token() -> Option<Self> {
        let mut config = Self::default();
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return None;
        };
        let path = PathBuf::from(local)
            .join("BedsideLampLocal")
            .join("token.json");
        let Ok(contents) = fs::read_to_string(path) else {
            return None;
        };
        let Ok(value) = serde_json::from_str::<Value>(&contents) else {
            return None;
        };
        if value["device_id"].as_str() != Some("75416834") {
            return None;
        }
        config.device_id = LAMP_DEVICE_ID;
        if let Some(token) = value["token"].as_str().filter(|token| valid_token(token)) {
            config.token = token.to_owned();
        }
        if let Some(ip) = value["ip"]
            .as_str()
            .filter(|ip| ip.parse::<Ipv4Addr>().is_ok())
        {
            config.ip = ip.to_owned();
        }
        if config.token.is_empty() {
            None
        } else {
            Some(config)
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl LampConfig {
    pub(crate) fn import_existing_token() -> Option<Self> {
        None
    }
}

fn valid_token(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn md5(bytes: &[u8]) -> [u8; 16] {
    Md5::digest(bytes).into()
}

fn next_request_id() -> u32 {
    static NEXT: OnceLock<AtomicU32> = OnceLock::new();
    let counter = NEXT.get_or_init(|| {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u32;
        AtomicU32::new(seed)
    });
    counter.fetch_add(1, Ordering::Relaxed) % 9999 + 1
}

fn send(config: &LampConfig, method: &str, params: Value) -> Result<Value, String> {
    let ip = config
        .ip
        .parse::<Ipv4Addr>()
        .map_err(|_| "Lamp address must be an IPv4 address")?;
    if !valid_token(&config.token) {
        return Err("Set the bedside lamp token in Settings".into());
    }
    let token: [u8; 16] = hex::decode(&config.token)
        .map_err(|_| "Invalid lamp token")?
        .try_into()
        .map_err(|_| "Invalid lamp token")?;
    let address = SocketAddrV4::new(ip, MIIO_PORT);
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;

    let mut hello = [0xff_u8; 32];
    hello[..4].copy_from_slice(&[0x21, 0x31, 0x00, 0x20]);
    socket.send_to(&hello, address).map_err(|e| e.to_string())?;
    let mut response = [0_u8; 4096];
    let (length, source) = socket
        .recv_from(&mut response)
        .map_err(|e| format!("Bedside lamp did not answer discovery: {e}"))?;
    if source != address.into() || length != 32 || response[..4] != hello[..4] {
        return Err("Invalid bedside lamp discovery response".into());
    }
    let device_id = u32::from_be_bytes(response[8..12].try_into().unwrap());
    if device_id != config.device_id {
        return Err("The address belongs to a different Xiaomi device".into());
    }
    let timestamp = u32::from_be_bytes(response[12..16].try_into().unwrap()).wrapping_add(1);

    let mut payload =
        serde_json::to_vec(&json!({"id": next_request_id(), "method": method, "params": params}))
            .map_err(|e| e.to_string())?;
    payload.push(0);
    let key = md5(&token);
    let iv = md5(&[key.as_slice(), token.as_slice()].concat());
    let mut padded = vec![0_u8; payload.len() + 16];
    padded[..payload.len()].copy_from_slice(&payload);
    let cipher = cbc::Encryptor::<Aes128>::new(&key.into(), &iv.into());
    let encrypted = cipher
        .encrypt_padded_mut::<Pkcs7>(&mut padded, payload.len())
        .map_err(|_| "Could not encrypt lamp command")?;
    let packet_length = 32 + encrypted.len();
    let packet_length: u16 = packet_length
        .try_into()
        .map_err(|_| "Lamp command is too long")?;
    let mut packet = Vec::with_capacity(packet_length as usize);
    packet.extend_from_slice(&[0x21, 0x31]);
    packet.extend_from_slice(&packet_length.to_be_bytes());
    packet.extend_from_slice(&[0; 4]);
    packet.extend_from_slice(&device_id.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    let checksum = md5(&[packet.as_slice(), token.as_slice(), encrypted].concat());
    packet.extend_from_slice(&checksum);
    packet.extend_from_slice(encrypted);
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    socket.connect(address).map_err(|e| e.to_string())?;
    socket.send(&packet).map_err(|e| e.to_string())?;

    let length = socket
        .recv(&mut response)
        .map_err(|e| format!("Bedside lamp did not answer command: {e}"))?;
    if length < 48
        || response[..2] != [0x21, 0x31]
        || u16::from_be_bytes(response[2..4].try_into().unwrap()) as usize != length
        || (length - 32) % 16 != 0
    {
        return Err("Invalid bedside lamp command response".into());
    }
    let expected = md5(&[&response[..16], token.as_slice(), &response[32..length]].concat());
    if response[16..32] != expected {
        return Err("Bedside lamp token was rejected".into());
    }
    let mut encrypted = response[32..length].to_vec();
    let cipher = cbc::Decryptor::<Aes128>::new(&key.into(), &iv.into());
    let plaintext = cipher
        .decrypt_padded_mut::<Pkcs7>(&mut encrypted)
        .map_err(|_| "Could not decrypt lamp response")?;
    let plaintext = plaintext.strip_suffix(&[0]).unwrap_or(plaintext);
    let reply: Value = serde_json::from_slice(plaintext)
        .map_err(|e| format!("Invalid bedside lamp response: {e}"))?;
    if !reply["error"].is_null() {
        return Err(format!("Bedside lamp rejected command: {}", reply["error"]));
    }
    Ok(reply["result"].clone())
}

fn property_u16(value: &Value, name: &str) -> Result<u16, String> {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .or_else(|| value.as_u64().and_then(|number| number.try_into().ok()))
        .ok_or_else(|| format!("Lamp did not report {name}"))
}

fn read_state(config: &LampConfig) -> Result<LampState, String> {
    let result = send(config, "get_prop", json!(["power", "bright", "ct"]))?;
    let values = result.as_array().ok_or("Invalid lamp status")?;
    if values.len() != 3 {
        return Err("Incomplete lamp status".into());
    }
    let power = match values[0].as_str() {
        Some("on") => true,
        Some("off") => false,
        _ => return Err("Lamp did not report power".into()),
    };
    Ok(LampState {
        power,
        brightness: property_u16(&values[1], "brightness")?
            .try_into()
            .map_err(|_| "Invalid lamp brightness")?,
        temperature: property_u16(&values[2], "color temperature")?,
    })
}

fn change(
    app: AppHandle,
    device_id: &str,
    method: &str,
    params: Value,
) -> Result<LampState, String> {
    let config = config::lamp(&app, device_id)?;
    let result = send(&config, method, params)?;
    if !result
        .as_array()
        .is_some_and(|items| items.first().and_then(Value::as_str) == Some("ok"))
    {
        return Err(format!("Lamp did not confirm command: {result}"));
    }
    read_state(&config)
}

#[tauri::command]
pub(crate) fn get_lamp_config(app: AppHandle, device_id: String) -> Result<LampConfigView, String> {
    let store = config::read(&app)?;
    let device = store.device(&device_id)?;
    match &device.settings {
        DeviceSettings::BedsideLamp(settings) => {
            Ok(LampConfigView::from_config(&device.name, settings))
        }
        _ => Err("Selected device is not a bedside lamp".into()),
    }
}

#[tauri::command]
pub(crate) fn save_lamp_config(
    app: AppHandle,
    device_id: String,
    config: LampConfigInput,
) -> Result<LampConfigView, String> {
    config::update(&app, |store| {
        let device = store.device_mut(&device_id)?;
        let DeviceSettings::BedsideLamp(stored) = &mut device.settings else {
            return Err("Selected device is not a bedside lamp".into());
        };
        device.name = config.name;
        stored.ip = config.ip;
        stored.device_id = config.device_id;
        if !config.token.is_empty() {
            stored.token = config.token;
        }
        stored.validate()?;
        Ok(LampConfigView::from_config(&device.name, stored))
    })
}

#[tauri::command]
pub(crate) async fn get_lamp_state(app: AppHandle, device_id: String) -> Result<LampState, String> {
    tauri::async_runtime::spawn_blocking(move || read_state(&config::lamp(&app, &device_id)?))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn set_lamp_power(
    app: AppHandle,
    device_id: String,
    on: bool,
) -> Result<LampState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        change(
            app,
            &device_id,
            "set_power",
            json!([if on { "on" } else { "off" }]),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn set_lamp_brightness(
    app: AppHandle,
    device_id: String,
    brightness: u8,
) -> Result<LampState, String> {
    if !(1..=100).contains(&brightness) {
        return Err("Brightness must be 1 to 100".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        change(app, &device_id, "set_bright", json!([brightness]))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn set_lamp_temperature(
    app: AppHandle,
    device_id: String,
    temperature: u16,
) -> Result<LampState, String> {
    if !(1700..=6500).contains(&temperature) {
        return Err("Color temperature must be 1700 to 6500 K".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        change(
            app,
            &device_id,
            "set_ct_abx",
            json!([temperature, "smooth", 500]),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the owner's bedside lamp on the local network"]
    fn live_local_control() {
        let path = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("BedsideLampLocal")
            .join("token.json");
        let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let config = LampConfig {
            ip: std::env::var("FROG_LAMP_TEST_IP").unwrap_or_else(|_| "192.168.31.237".into()),
            device_id: LAMP_DEVICE_ID,
            token: value["token"].as_str().unwrap().into(),
        };
        let before = read_state(&config).unwrap();
        let current = if before.power { "on" } else { "off" };
        assert_eq!(
            send(&config, "set_power", json!([current])).unwrap(),
            json!(["ok"])
        );
        assert_eq!(read_state(&config).unwrap().power, before.power);
    }
}
