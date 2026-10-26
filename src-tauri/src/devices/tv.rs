use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use russh::{client, keys::{self, PublicKeyOrCertificate}, ChannelMsg};
use tauri::{AppHandle, Manager};
#[cfg(not(target_os = "android"))]
use tauri::State;
use crate::config::{self, DeviceSettings};
#[cfg(not(target_os = "android"))]
use tauri_plugin_shell::{process::{CommandChild, CommandEvent}, ShellExt};
#[cfg(not(target_os = "android"))]
use tokio::sync::{mpsc, Mutex};
#[cfg(target_os = "android")]
use tauri_plugin_tv_python::TvPythonExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TvConfig {
    pub tv_ip: String,
    pub tv_mac: String,
    pub broadcast_ip: String,
    pub preferred_bluetooth_address: String,
    #[serde(default, skip_serializing)]
    pub root_key_path: String,
    #[serde(default)]
    pub root_private_key: String,
    pub webos_client_key: String,
}

impl Default for TvConfig {
    fn default() -> Self {
        Self {
            tv_ip: "0.0.0.0".into(),
            tv_mac: "00:00:00:00:00:00".into(),
            broadcast_ip: "255.255.255.255".into(),
            preferred_bluetooth_address: "00:00:00:00:00:00".into(),
            root_key_path: String::new(),
            root_private_key: String::new(),
            webos_client_key: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ConfigView {
    name: String,
    tv_ip: String,
    tv_mac: String,
    broadcast_ip: String,
    preferred_bluetooth_address: String,
    has_root_private_key: bool,
    has_webos_client_key: bool,
}

impl ConfigView {
    fn from_config(name: &str, config: &TvConfig) -> Self {
        Self {
            name: name.into(),
            tv_ip: config.tv_ip.clone(),
            tv_mac: config.tv_mac.clone(),
            broadcast_ip: config.broadcast_ip.clone(),
            preferred_bluetooth_address: config.preferred_bluetooth_address.clone(),
            has_root_private_key: !config.root_private_key.is_empty(),
            has_webos_client_key: !config.webos_client_key.is_empty(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ConfigInput {
    name: String,
    tv_ip: String,
    tv_mac: String,
    broadcast_ip: String,
    preferred_bluetooth_address: String,
    root_private_key: String,
}

#[derive(Serialize)]
pub(crate) struct AudioState {
    volume: u8,
    sound_output: String,
}

#[cfg(not(target_os = "android"))]
struct RemoteProcess {
    child: CommandChild,
    responses: mpsc::Receiver<String>,
}

#[derive(Default)]
pub(crate) struct AppState {
    #[cfg(not(target_os = "android"))]
    remote: Arc<Mutex<Option<RemoteProcess>>>,
}

fn app_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

impl TvConfig {
pub(crate) fn validate(&self) -> Result<(), String> {
    let config = self;
    config.tv_ip.parse::<IpAddr>().map_err(|_| "TV address must be an IP address".to_string())?;
    config.broadcast_ip.parse::<IpAddr>().map_err(|_| "Broadcast address must be an IP address".to_string())?;
    if !valid_address(&config.tv_mac.replace('-', ":")) {
        return Err("TV MAC address must contain six hex bytes".into());
    }
    if !valid_address(&config.preferred_bluetooth_address) {
        return Err("Headphone Bluetooth address must contain six hex bytes".into());
    }
    Ok(())
}
}

fn valid_address(address: &str) -> bool {
    let parts: Vec<_> = address.split(':').collect();
    parts.len() == 6 && parts.iter().all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

struct TvHostKey {
    address: String,
    known_hosts: PathBuf,
}

impl client::Handler for TvHostKey {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, server_key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return Ok(false);
        };
        if keys::known_hosts::check_known_hosts_path(&self.address, 22, key, &self.known_hosts)? {
            return Ok(true);
        }
        // Match the previous OpenSSH StrictHostKeyChecking=accept-new behavior.
        keys::known_hosts::learn_known_hosts_path(&self.address, 22, key, &self.known_hosts)?;
        Ok(true)
    }
}

async fn run_tv_script(address: &str, private_key: &str, known_hosts: PathBuf, script: String) -> Result<Vec<u8>, String> {
    let key = keys::decode_secret_key(private_key, None).map_err(|e| format!("Invalid TV SSH key: {e}"))?;
    let handler = TvHostKey { address: address.to_owned(), known_hosts };
    let connection = client::connect(Arc::new(client::Config::default()), (address, 22), handler);
    let mut session = tokio::time::timeout(Duration::from_secs(5), connection)
        .await.map_err(|_| "TV SSH connection timed out".to_string())?
        .map_err(|e| format!("TV SSH connection failed: {e}"))?;
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let auth = session.authenticate_publickey(
            "root",
            keys::PrivateKeyWithHashAlg::new(
                Arc::new(key),
                session.best_supported_rsa_hash().await.map_err(|e| e.to_string())?.flatten(),
            ),
        ).await.map_err(|e| e.to_string())?;
        if !auth.success() { return Err("TV SSH key was rejected".to_string()); }
        let mut channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
        channel.exec(true, "sh -s").await.map_err(|e| e.to_string())?;
        channel.data(script.as_bytes()).await.map_err(|e| e.to_string())?;
        channel.eof().await.map_err(|e| e.to_string())?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
        }
        if status != Some(0) || stdout.is_empty() {
            let error = String::from_utf8_lossy(&stderr).trim().to_owned();
            return Err(if error.is_empty() { "TV did not answer the SSH command".into() } else { error });
        }
        Ok(stdout)
    }).await.map_err(|_| "TV SSH command timed out".to_string())?;
    let _ = session.disconnect(russh::Disconnect::ByApplication, "Done", "").await;
    result
}

fn luna_call(app: &AppHandle, config: &TvConfig, service: &str, method: &str, payload: Value) -> Result<Value, String> {
    luna_call_at(&app_dir(app)?, config, service, method, payload)
}

fn luna_call_at(data_dir: &Path, config: &TvConfig, service: &str, method: &str, payload: Value) -> Result<Value, String> {
    if config.root_private_key.is_empty() {
        return Err("Add the root SSH private key in Settings".into());
    }
    if !service.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
        || !method.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'/') {
        return Err("Invalid TV service".into());
    }
    let known_hosts = data_dir.join("known_hosts");
    let data = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    // Calls only receive allowlisted, validated values, so the shell quoting stays fixed.
    let script = format!("luna-send -w 5000 -n 1 -f luna://{service}/{method} '{data}' </dev/null\n");
    let output = tauri::async_runtime::block_on(run_tv_script(
        &config.tv_ip, &config.root_private_key, known_hosts, script,
    ))?;
    let response: Value = serde_json::from_slice(&output).map_err(|e| format!("Invalid TV response: {e}"))?;
    if response["returnValue"] != true {
        return Err(response["errorText"].as_str().unwrap_or("TV rejected the command").into());
    }
    Ok(response)
}

fn audio_state(app: &AppHandle, config: &TvConfig) -> Result<AudioState, String> {
    audio_state_at(&app_dir(app)?, config)
}

fn audio_state_at(data_dir: &Path, config: &TvConfig) -> Result<AudioState, String> {
    let result = luna_call_at(data_dir, config, "com.webos.service.audio", "master/getVolume", json!({}))?;
    let status = &result["volumeStatus"];
    Ok(AudioState {
        volume: status["volume"].as_u64().ok_or("TV did not report volume")? as u8,
        sound_output: status["soundOutput"].as_str().ok_or("TV did not report sound output")?.into(),
    })
}

#[cfg(target_os = "android")]
pub(crate) fn widget_audio(config: &TvConfig, data_dir: &Path, action: &str) -> Result<AudioState, String> {
    match action {
        "refresh" => {}
        "up" | "down" => {
            let current = audio_state_at(data_dir, config)?;
            let volume = if action == "up" { current.volume.saturating_add(1).min(100) }
                else { current.volume.saturating_sub(1) };
            luna_call_at(data_dir, config, "com.webos.service.audio", "master/setVolume",
                json!({"soundOutput":current.sound_output,"volume":volume}))?;
        }
        "speaker" => {
            luna_call_at(data_dir, config, "com.webos.service.apiadapter", "audio/changeSoundOutput",
                json!({"output":"tv_speaker"}))?;
        }
        "headphones" => {
            let address = &config.preferred_bluetooth_address;
            let paired = luna_call_at(data_dir, config, "com.webos.service.bluetooth", "gap/getTrustedDevices",
                json!({"service":"audio"}))?;
            if !paired["device"].as_array().is_some_and(|devices| devices.iter().any(|device|
                device["address"].as_str().is_some_and(|found| found.eq_ignore_ascii_case(address)))) {
                return Err("Configured headphones are not paired with the TV".into());
            }
            luna_call_at(data_dir, config, "com.webos.service.bluetooth", "service/connect",
                json!({"service":"audio","address":address}))?;
            for attempt in 0..12 {
                std::thread::sleep(Duration::from_secs(1));
                let paired = luna_call_at(data_dir, config, "com.webos.service.bluetooth", "gap/getTrustedDevices",
                    json!({"service":"audio"}))?;
                let connected = paired["device"].as_array().is_some_and(|devices| devices.iter().any(|device|
                    device["address"].as_str().is_some_and(|found| found.eq_ignore_ascii_case(address))
                        && device["state"] == true));
                let state = audio_state_at(data_dir, config)?;
                if connected && state.sound_output == "bt_soundbar" { return Ok(state); }
                if connected && attempt >= 2 && state.sound_output != "bt_soundbar" {
                    luna_call_at(data_dir, config, "com.webos.service.apiadapter", "audio/changeSoundOutput",
                        json!({"output":"bt_soundbar"}))?;
                }
            }
            return Err("Headphones did not connect; check that they are powered on".into());
        }
        _ => return Err("Unsupported TV widget action".into()),
    }
    audio_state_at(data_dir, config)
}

#[cfg(target_os = "android")]
impl AudioState {
    pub(crate) fn widget_volume(&self) -> u8 { self.volume }
    pub(crate) fn widget_output(&self) -> &str { &self.sound_output }
}

fn paired_devices(app: &AppHandle, config: &TvConfig) -> Result<Vec<Value>, String> {
    let result = luna_call(app, config, "com.webos.service.bluetooth", "gap/getTrustedDevices", json!({"service":"audio"}))?;
    Ok(result["device"].as_array().cloned().unwrap_or_default())
}

fn change_output(app: &AppHandle, config: &TvConfig, output: &str) -> Result<(), String> {
    if output != "tv_speaker" && output != "bt_soundbar" {
        return Err("Unsupported sound output".into());
    }
    luna_call(app, config, "com.webos.service.apiadapter", "audio/changeSoundOutput", json!({"output":output}))?;
    Ok(())
}

#[tauri::command]
pub(crate) fn get_config(app: AppHandle, device_id: String) -> Result<ConfigView, String> {
    let store = config::read(&app)?;
    let device = store.device(&device_id)?;
    match &device.settings {
        DeviceSettings::Tv(settings) => Ok(ConfigView::from_config(&device.name, settings)),
        _ => Err("Selected device is not a TV".into()),
    }
}

#[tauri::command]
pub(crate) fn save_config(app: AppHandle, device_id: String, config: ConfigInput) -> Result<ConfigView, String> {
    config::update(&app, |store| {
        let device = store.device_mut(&device_id)?;
        let DeviceSettings::Tv(stored) = &mut device.settings else { return Err("Selected device is not a TV".into()) };
        device.name = config.name;
        stored.tv_ip = config.tv_ip;
        stored.tv_mac = config.tv_mac;
        stored.broadcast_ip = config.broadcast_ip;
        stored.preferred_bluetooth_address = config.preferred_bluetooth_address;
        if !config.root_private_key.is_empty() { stored.root_private_key = config.root_private_key; }
        stored.validate()?;
        Ok(ConfigView::from_config(&device.name, stored))
    })
}

#[tauri::command]
pub(crate) async fn get_state(app: AppHandle, device_id: String) -> Result<AudioState, String> {
    tauri::async_runtime::spawn_blocking(move || audio_state(&app, &config::tv(&app, &device_id)?))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn set_volume(app: AppHandle, device_id: String, volume: u8) -> Result<AudioState, String> {
    if volume > 100 { return Err("Volume must be 0 to 100".into()); }
    #[cfg(target_os = "android")]
    let refresh_app = app.clone();
    let state = tauri::async_runtime::spawn_blocking(move || {
        let config = config::tv(&app, &device_id)?;
        let output = audio_state(&app, &config)?.sound_output;
        luna_call(&app, &config, "com.webos.service.audio", "master/setVolume", json!({"soundOutput":output,"volume":volume}))?;
        audio_state(&app, &config)
    }).await.map_err(|e| e.to_string())??;
    #[cfg(target_os = "android")]
    let _ = refresh_app.tv_python().refresh_widgets();
    Ok(state)
}

#[tauri::command]
pub(crate) async fn set_output(app: AppHandle, device_id: String, output: String) -> Result<AudioState, String> {
    #[cfg(target_os = "android")]
    let refresh_app = app.clone();
    let state = tauri::async_runtime::spawn_blocking(move || {
        let config = config::tv(&app, &device_id)?;
        change_output(&app, &config, &output)?;
        audio_state(&app, &config)
    }).await.map_err(|e| e.to_string())??;
    #[cfg(target_os = "android")]
    let _ = refresh_app.tv_python().refresh_widgets();
    Ok(state)
}

#[tauri::command]
pub(crate) async fn get_paired_devices(app: AppHandle, device_id: String) -> Result<Vec<Value>, String> {
    tauri::async_runtime::spawn_blocking(move || paired_devices(&app, &config::tv(&app, &device_id)?))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn connect_headphones(app: AppHandle, device_id: String) -> Result<AudioState, String> {
    #[cfg(target_os = "android")]
    let refresh_app = app.clone();
    let state = tauri::async_runtime::spawn_blocking(move || -> Result<AudioState, String> {
        let config = config::tv(&app, &device_id)?;
        let address = &config.preferred_bluetooth_address;
        if !paired_devices(&app, &config)?.iter().any(|device| device["address"].as_str()
            .is_some_and(|found| found.eq_ignore_ascii_case(address))) {
            return Err("Configured headphones are not paired with the TV".into());
        }
        luna_call(&app, &config, "com.webos.service.bluetooth", "service/connect",
            json!({"service":"audio","address":address}))?;
        let mut stable = 0;
        for attempt in 0..12 {
            std::thread::sleep(Duration::from_secs(1));
            let connected = paired_devices(&app, &config)?.iter().any(|device| device["address"].as_str()
                .is_some_and(|found| found.eq_ignore_ascii_case(address)) && device["state"] == true);
            let state = audio_state(&app, &config)?;
            if connected && state.sound_output == "bt_soundbar" {
                stable += 1;
                if stable >= 2 { return Ok(state); }
            } else {
                stable = 0;
            }
            if connected && attempt >= 2 && state.sound_output != "bt_soundbar" {
                change_output(&app, &config, "bt_soundbar")?;
            }
        }
        Err("Headphones did not connect; check that they are powered on".into())
    }).await.map_err(|e| e.to_string())??;
    #[cfg(target_os = "android")]
    let _ = refresh_app.tv_python().refresh_widgets();
    Ok(state)
}

#[cfg(not(target_os = "android"))]
async fn start_remote(app: &AppHandle) -> Result<RemoteProcess, String> {
    let (mut events, child) = app.shell().sidecar("tv-python")
        .map_err(|e| e.to_string())?.spawn().map_err(|e| e.to_string())?;
    let (tx, responses) = mpsc::channel(16);
    tauri::async_runtime::spawn(async move {
        let mut buffer = String::new();
        while let Some(event) = events.recv().await {
            if let CommandEvent::Stdout(bytes) = event {
                buffer.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(end) = buffer.find('\n') {
                    let line: String = buffer.drain(..=end).collect();
                    if tx.send(line).await.is_err() { return; }
                }
            }
        }
    });
    Ok(RemoteProcess { child, responses })
}

#[cfg(not(target_os = "android"))]
pub(crate) async fn pair_ip(app: &AppHandle, ip: &str) -> Result<String, String> {
    let mut remote = start_remote(app).await?;
    let request = json!({"id":1,"action":"pair","tv_ip":ip,"client_key":""});
    remote.child.write(format!("{}\n", request).as_bytes()).map_err(|e| e.to_string())?;
    let line = tokio::time::timeout(Duration::from_secs(75), remote.responses.recv())
        .await.map_err(|_| "TV pairing timed out".to_string())?.ok_or("TV pairing process ended")?;
    let response: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if response["ok"] != true { return Err(response["error"].as_str().unwrap_or("TV pairing failed").into()); }
    response["client_key"].as_str().filter(|key| !key.is_empty()).map(str::to_owned)
        .ok_or_else(|| "TV did not provide a pairing key".into())
}

#[cfg(target_os = "android")]
fn mobile_request(app: &AppHandle, request: Value) -> Result<Value, String> {
    let response = app.tv_python().request(&request.to_string()).map_err(|e| e.to_string())?;
    serde_json::from_str(&response).map_err(|e| format!("Invalid TV response: {e}"))
}

#[cfg(target_os = "android")]
pub(crate) async fn pair_ip(app: &AppHandle, ip: &str) -> Result<String, String> {
    let app = app.clone();
    let ip = ip.to_owned();
    let response = tauri::async_runtime::spawn_blocking(move ||
        mobile_request(&app, json!({"id":1,"action":"pair","tv_ip":ip,"client_key":""})))
        .await.map_err(|e| e.to_string())??;
    if response["ok"] != true { return Err(response["error"].as_str().unwrap_or("TV pairing failed").into()); }
    response["client_key"].as_str().filter(|key| !key.is_empty()).map(str::to_owned)
        .ok_or_else(|| "TV did not provide a pairing key".into())
}

const BUTTONS: &[&str] = &["UP","DOWN","LEFT","RIGHT","ENTER","BACK","EXIT","HOME","MENU","POWER","INPUT_HUB",
    "MUTE","VOLUMEUP","VOLUMEDOWN","CHANNELUP","CHANNELDOWN","PLAY","PAUSE","STOP",
    "REWIND","FASTFORWARD","INFO","GUIDE","RED","GREEN","YELLOW","BLUE",
    "0","1","2","3","4","5","6","7","8","9"];

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub(crate) async fn remote_button(app: AppHandle, state: State<'_, AppState>, device_id: String, button: String) -> Result<(), String> {
    if !BUTTONS.contains(&button.as_str()) { return Err("Unsupported remote button".into()); }
    let remote = state.remote.clone();
    let mut guard = remote.lock().await;
    if guard.is_none() { *guard = Some(start_remote(&app).await?); }
    let process = guard.as_mut().ok_or("Remote process unavailable")?;
    let config = config::tv(&app, &device_id)?;
    let request = json!({"id":1,"action":"button","button":button,"tv_ip":config.tv_ip,
        "client_key":config.webos_client_key});
    process.child.write(format!("{}\n", request).as_bytes()).map_err(|e| e.to_string())?;
    let line = tokio::time::timeout(Duration::from_secs(20), process.responses.recv())
        .await.map_err(|_| "Remote request timed out".to_string())?.ok_or("Remote process ended")?;
    let response: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if response["ok"] != true { return Err(response["error"].as_str().unwrap_or("Remote command failed").into()); }
    if let Some(key) = response["client_key"].as_str() {
        if key != config.webos_client_key {
            config::update(&app, |store| {
                let device = store.device_mut(&device_id)?;
                let DeviceSettings::Tv(settings) = &mut device.settings else { return Err("Selected device is not a TV".into()) };
                settings.webos_client_key = key.to_owned();
                Ok(())
            })?;
        }
    }
    Ok(())
}

#[cfg(target_os = "android")]
#[tauri::command]
pub(crate) async fn remote_button(app: AppHandle, device_id: String, button: String) -> Result<(), String> {
    if !BUTTONS.contains(&button.as_str()) { return Err("Unsupported remote button".into()); }
    let config = config::tv(&app, &device_id)?;
    let request = json!({"id":1,"action":"button","button":button,"tv_ip":config.tv_ip,
        "client_key":config.webos_client_key});
    let response = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        move || mobile_request(&app, request)
    }).await.map_err(|e| e.to_string())??;
    if response["ok"] != true { return Err(response["error"].as_str().unwrap_or("Remote command failed").into()); }
    if let Some(key) = response["client_key"].as_str() {
        if key != config.webos_client_key {
            config::update(&app, |store| {
                let device = store.device_mut(&device_id)?;
                let DeviceSettings::Tv(settings) = &mut device.settings else { return Err("Selected device is not a TV".into()) };
                settings.webos_client_key = key.to_owned();
                Ok(())
            })?;
        }
    }
    Ok(())
}

