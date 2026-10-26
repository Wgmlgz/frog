use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_shell::{process::{CommandChild, CommandEvent}, ShellExt};
use tokio::sync::{mpsc, Mutex};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TvConfig {
    tv_ip: String,
    tv_mac: String,
    broadcast_ip: String,
    preferred_bluetooth_address: String,
    root_key_path: String,
    webos_client_key: String,
}

impl Default for TvConfig {
    fn default() -> Self {
        let dev_key = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tv/.tv_root_key");
        Self {
            tv_ip: "192.168.31.89".into(),
            tv_mac: "20-28-BC-73-B2-7A".into(),
            broadcast_ip: "192.168.31.255".into(),
            preferred_bluetooth_address: "80:99:e7:ff:d0:01".into(),
            root_key_path: if cfg!(debug_assertions) && dev_key.exists() {
                dev_key.to_string_lossy().into_owned()
            } else {
                String::new()
            },
            webos_client_key: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ConfigView {
    tv_ip: String,
    tv_mac: String,
    broadcast_ip: String,
    preferred_bluetooth_address: String,
    root_key_path: String,
    has_webos_client_key: bool,
}

impl From<&TvConfig> for ConfigView {
    fn from(config: &TvConfig) -> Self {
        Self {
            tv_ip: config.tv_ip.clone(),
            tv_mac: config.tv_mac.clone(),
            broadcast_ip: config.broadcast_ip.clone(),
            preferred_bluetooth_address: config.preferred_bluetooth_address.clone(),
            root_key_path: config.root_key_path.clone(),
            has_webos_client_key: !config.webos_client_key.is_empty(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct AudioState {
    volume: u8,
    sound_output: String,
}

struct RemoteProcess {
    child: CommandChild,
    responses: mpsc::Receiver<String>,
}

#[derive(Default)]
pub(crate) struct AppState {
    remote: Arc<Mutex<Option<RemoteProcess>>>,
}

fn app_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tv/settings.toml"))
    } else {
        Ok(app_dir(app)?.join("settings.toml"))
    }
}

fn load_config(app: &AppHandle) -> Result<TvConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        let config = TvConfig::default();
        save_config_file(app, &config)?;
        return Ok(config);
    }
    let data = fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str(&data).map_err(|e| e.to_string())
}

fn save_config_file(app: &AppHandle, config: &TvConfig) -> Result<(), String> {
    let data = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    let path = config_path(app)?;
    fs::write(&path, data).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn validate_config(config: &TvConfig) -> Result<(), String> {
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

fn valid_address(address: &str) -> bool {
    let parts: Vec<_> = address.split(':').collect();
    parts.len() == 6 && parts.iter().all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn luna_call(app: &AppHandle, config: &TvConfig, service: &str, method: &str, payload: Value) -> Result<Value, String> {
    let key = Path::new(&config.root_key_path);
    if config.root_key_path.is_empty() || !key.is_file() {
        return Err("Set the root SSH key path in Settings".into());
    }
    if !service.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
        || !method.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'/') {
        return Err("Invalid TV service".into());
    }
    let known_hosts = app_dir(app)?.join("known_hosts");
    let mut command = Command::new("ssh");
    command.args([
        "-i", &config.root_key_path,
        "-o", "BatchMode=yes",
        "-o", "ConnectTimeout=5",
        "-o", "StrictHostKeyChecking=accept-new",
        "-o", &format!("UserKnownHostsFile={}", known_hosts.display()),
        &format!("root@{}", config.tv_ip), "sh", "-s",
    ]);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(|e| format!("Could not start SSH: {e}"))?;
    let data = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    // Calls only receive allowlisted, validated values, so the shell quoting stays fixed.
    let script = format!("luna-send -w 5000 -n 1 -f luna://{service}/{method} '{data}' </dev/null\n");
    child.stdin.take().ok_or("SSH input unavailable")?
        .write_all(script.as_bytes()).map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() || output.stdout.is_empty() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if error.is_empty() { "TV did not answer the SSH command".into() } else { error });
    }
    let response: Value = serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid TV response: {e}"))?;
    if response["returnValue"] != true {
        return Err(response["errorText"].as_str().unwrap_or("TV rejected the command").into());
    }
    Ok(response)
}

fn audio_state(app: &AppHandle, config: &TvConfig) -> Result<AudioState, String> {
    let result = luna_call(app, config, "com.webos.service.audio", "master/getVolume", json!({}))?;
    let status = &result["volumeStatus"];
    Ok(AudioState {
        volume: status["volume"].as_u64().ok_or("TV did not report volume")? as u8,
        sound_output: status["soundOutput"].as_str().ok_or("TV did not report sound output")?.into(),
    })
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
pub(crate) fn get_config(app: AppHandle) -> Result<ConfigView, String> {
    Ok(ConfigView::from(&load_config(&app)?))
}

#[tauri::command]
pub(crate) fn save_config(app: AppHandle, config: ConfigView) -> Result<ConfigView, String> {
    let mut stored = load_config(&app)?;
    stored.tv_ip = config.tv_ip;
    stored.tv_mac = config.tv_mac;
    stored.broadcast_ip = config.broadcast_ip;
    stored.preferred_bluetooth_address = config.preferred_bluetooth_address;
    stored.root_key_path = config.root_key_path;
    validate_config(&stored)?;
    save_config_file(&app, &stored)?;
    Ok(ConfigView::from(&stored))
}

#[tauri::command]
pub(crate) async fn get_state(app: AppHandle) -> Result<AudioState, String> {
    tauri::async_runtime::spawn_blocking(move || audio_state(&app, &load_config(&app)?))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn set_volume(app: AppHandle, volume: u8) -> Result<AudioState, String> {
    if volume > 100 { return Err("Volume must be 0 to 100".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        let config = load_config(&app)?;
        let output = audio_state(&app, &config)?.sound_output;
        luna_call(&app, &config, "com.webos.service.audio", "master/setVolume", json!({"soundOutput":output,"volume":volume}))?;
        audio_state(&app, &config)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn set_output(app: AppHandle, output: String) -> Result<AudioState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = load_config(&app)?;
        change_output(&app, &config, &output)?;
        audio_state(&app, &config)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn get_paired_devices(app: AppHandle) -> Result<Vec<Value>, String> {
    tauri::async_runtime::spawn_blocking(move || paired_devices(&app, &load_config(&app)?))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn connect_headphones(app: AppHandle) -> Result<AudioState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = load_config(&app)?;
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
    }).await.map_err(|e| e.to_string())?
}

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

#[tauri::command]
pub(crate) async fn remote_button(app: AppHandle, state: State<'_, AppState>, button: String) -> Result<(), String> {
    const BUTTONS: &[&str] = &["UP","DOWN","LEFT","RIGHT","ENTER","BACK","EXIT","HOME","MENU","POWER","INPUT_HUB",
        "MUTE","VOLUMEUP","VOLUMEDOWN","CHANNELUP","CHANNELDOWN","PLAY","PAUSE","STOP",
        "REWIND","FASTFORWARD","INFO","GUIDE","RED","GREEN","YELLOW","BLUE",
        "0","1","2","3","4","5","6","7","8","9"];
    if !BUTTONS.contains(&button.as_str()) { return Err("Unsupported remote button".into()); }
    let remote = state.remote.clone();
    let mut guard = remote.lock().await;
    if guard.is_none() { *guard = Some(start_remote(&app).await?); }
    let process = guard.as_mut().ok_or("Remote process unavailable")?;
    let config = load_config(&app)?;
    let request = json!({"id":1,"action":"button","button":button,"tv_ip":config.tv_ip,
        "client_key":config.webos_client_key});
    process.child.write(format!("{}\n", request).as_bytes()).map_err(|e| e.to_string())?;
    let line = tokio::time::timeout(Duration::from_secs(20), process.responses.recv())
        .await.map_err(|_| "Remote request timed out".to_string())?.ok_or("Remote process ended")?;
    let response: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if response["ok"] != true { return Err(response["error"].as_str().unwrap_or("Remote command failed").into()); }
    if let Some(key) = response["client_key"].as_str() {
        if key != config.webos_client_key {
            let mut updated = config;
            updated.webos_client_key = key.to_owned();
            save_config_file(&app, &updated)?;
        }
    }
    Ok(())
}

