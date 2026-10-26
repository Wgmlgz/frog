use crate::{
    cloud::{SavedXiaomiAccount, XiaomiAccountView},
    devices::{bedside_lamp::LampConfig, tv::TvConfig},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;

const VERSION: u32 = 1;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AppConfig {
    version: u32,
    #[serde(default)]
    pub(crate) xiaomi_accounts: Vec<SavedXiaomiAccount>,
    pub(crate) devices: Vec<DeviceEntry>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: VERSION,
            xiaomi_accounts: Vec::new(),
            devices: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct DeviceEntry {
    pub id: String,
    pub name: String,
    pub settings: DeviceSettings,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum DeviceSettings {
    Tv(TvConfig),
    BedsideLamp(LampConfig),
    ColorBulb(LampConfig),
}

#[derive(Serialize)]
pub(crate) struct DeviceSummary {
    id: String,
    name: String,
    #[serde(rename = "type")]
    device_type: &'static str,
    address: String,
}

impl From<&DeviceEntry> for DeviceSummary {
    fn from(device: &DeviceEntry) -> Self {
        let (device_type, address) = match &device.settings {
            DeviceSettings::Tv(settings) => ("tv", settings.tv_ip.clone()),
            DeviceSettings::BedsideLamp(settings) => ("bedside_lamp", settings.ip.clone()),
            DeviceSettings::ColorBulb(settings) => ("color_bulb", settings.ip.clone()),
        };
        Self {
            id: device.id.clone(),
            name: device.name.clone(),
            device_type,
            address,
        }
    }
}

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("frog.toml"))
}

fn read_unlocked(app: &AppHandle) -> Result<AppConfig, String> {
    let path = path(app)?;
    if !path.exists() {
        let config = migrate(app)?;
        write_unlocked(app, &config)?;
        return Ok(config);
    }
    parse(&fs::read_to_string(path).map_err(|e| e.to_string())?)
}

pub(crate) fn parse(source: &str) -> Result<AppConfig, String> {
    let config: AppConfig =
        toml::from_str(source).map_err(|e| format!("Invalid Frog config: {e}"))?;
    validate(&config)?;
    Ok(config)
}

fn validate(config: &AppConfig) -> Result<(), String> {
    if config.version != VERSION {
        return Err(format!(
            "Unsupported Frog config version {}",
            config.version
        ));
    }
    let mut ids = std::collections::HashSet::new();
    let mut accounts = std::collections::HashSet::new();
    for account in &config.xiaomi_accounts {
        if account.user_id.is_empty()
            || !accounts.insert(&account.user_id)
            || account.ssecurity.is_empty()
            || account.service_token.is_empty()
        {
            return Err("Xiaomi account sessions must be complete and unique".into());
        }
    }
    for device in &config.devices {
        if Uuid::parse_str(&device.id).is_err() || !ids.insert(&device.id) {
            return Err("Device IDs must be unique UUIDs".into());
        }
        if device.name.trim().is_empty() || device.name.len() > 80 {
            return Err("Device name must be 1 to 80 characters".into());
        }
        match &device.settings {
            DeviceSettings::Tv(settings) => settings.validate()?,
            DeviceSettings::BedsideLamp(settings) => settings.validate()?,
            DeviceSettings::ColorBulb(settings) => settings.validate()?,
        }
    }
    Ok(())
}

fn write_unlocked(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    validate(config)?;
    let path = path(app)?;
    fs::write(
        &path,
        toml::to_string_pretty(config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_tv_python::TvPythonExt;
        let _ = app.tv_python().refresh_widgets();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn read(app: &AppHandle) -> Result<AppConfig, String> {
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    read_unlocked(app)
}

pub(crate) fn update<T>(
    app: &AppHandle,
    edit: impl FnOnce(&mut AppConfig) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    let mut config = read_unlocked(app)?;
    let result = edit(&mut config)?;
    write_unlocked(app, &config)?;
    Ok(result)
}

impl AppConfig {
    pub fn device(&self, id: &str) -> Result<&DeviceEntry, String> {
        self.devices
            .iter()
            .find(|device| device.id == id)
            .ok_or_else(|| "Device not found".into())
    }
    pub fn device_mut(&mut self, id: &str) -> Result<&mut DeviceEntry, String> {
        self.devices
            .iter_mut()
            .find(|device| device.id == id)
            .ok_or_else(|| "Device not found".into())
    }
}

pub(crate) fn tv(app: &AppHandle, id: &str) -> Result<TvConfig, String> {
    match &read(app)?.device(id)?.settings {
        DeviceSettings::Tv(settings) => Ok(settings.clone()),
        _ => Err("Selected device is not a TV".into()),
    }
}

pub(crate) fn lamp(app: &AppHandle, id: &str) -> Result<LampConfig, String> {
    match &read(app)?.device(id)?.settings {
        DeviceSettings::BedsideLamp(settings) | DeviceSettings::ColorBulb(settings) => {
            Ok(settings.clone())
        }
        _ => Err("Selected device is not a bedside lamp".into()),
    }
}

#[tauri::command]
pub(crate) fn list_xiaomi_accounts(app: AppHandle) -> Result<Vec<XiaomiAccountView>, String> {
    Ok(read(&app)?
        .xiaomi_accounts
        .iter()
        .map(XiaomiAccountView::from)
        .collect())
}

pub(crate) fn save_xiaomi_account(
    app: &AppHandle,
    account: SavedXiaomiAccount,
) -> Result<(), String> {
    update(app, |store| {
        if let Some(existing) = store
            .xiaomi_accounts
            .iter_mut()
            .find(|item| item.user_id == account.user_id)
        {
            *existing = account;
        } else {
            store.xiaomi_accounts.push(account);
        }
        Ok(())
    })
}

#[tauri::command]
pub(crate) fn list_devices(app: AppHandle) -> Result<Vec<DeviceSummary>, String> {
    Ok(read(&app)?
        .devices
        .iter()
        .map(DeviceSummary::from)
        .collect())
}

#[tauri::command]
pub(crate) fn remove_device(app: AppHandle, device_id: String) -> Result<(), String> {
    update(&app, |config| {
        let old_len = config.devices.len();
        config.devices.retain(|device| device.id != device_id);
        if config.devices.len() == old_len {
            return Err("Device not found".into());
        }
        Ok(())
    })
}

#[tauri::command]
pub(crate) fn export_config_to_clipboard(app: AppHandle) -> Result<(), String> {
    let content = toml::to_string_pretty(&read(&app)?).map_err(|e| e.to_string())?;
    app.clipboard()
        .write_text(content)
        .map_err(|e| format!("Clipboard export failed: {e}"))
}

#[tauri::command]
pub(crate) fn import_config_from_clipboard(app: AppHandle) -> Result<Vec<DeviceSummary>, String> {
    #[cfg(not(target_os = "android"))]
    let content = app
        .clipboard()
        .read_text()
        .map_err(|e| format!("Clipboard import failed: {e}"))?;
    #[cfg(target_os = "android")]
    let content = {
        use tauri_plugin_tv_python::TvPythonExt;
        app.tv_python()
            .read_clipboard_text()
            .map_err(|e| format!("Clipboard import failed: {e}"))?
    };
    let imported = parse(&content)?;
    let summaries = imported.devices.iter().map(DeviceSummary::from).collect();
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    write_unlocked(&app, &imported)?;
    Ok(summaries)
}

fn migrate(app: &AppHandle) -> Result<AppConfig, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dev_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tv");
    let old_tv = if cfg!(debug_assertions) {
        dev_dir.join("settings.toml")
    } else {
        app_dir.join("settings.toml")
    };
    let fallback_key = if cfg!(debug_assertions) {
        Some(dev_dir.join(".tv_root_key"))
    } else {
        None
    };
    migrate_files(
        &old_tv,
        fallback_key.as_deref(),
        &app_dir.join("bedside_lamp.toml"),
    )
}

fn migrate_files(
    old_tv: &Path,
    fallback_key: Option<&Path>,
    old_lamp: &Path,
) -> Result<AppConfig, String> {
    let mut config = AppConfig::default();
    if old_tv.exists() {
        let mut tv: TvConfig =
            toml::from_str(&fs::read_to_string(&old_tv).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Invalid old TV config: {e}"))?;
        if !tv.root_key_path.is_empty() {
            tv.root_private_key = fs::read_to_string(&tv.root_key_path)
                .map_err(|e| format!("Could not import TV SSH key: {e}"))?;
        } else if let Some(key_path) = fallback_key.filter(|path| path.exists()) {
            tv.root_private_key = fs::read_to_string(key_path)
                .map_err(|e| format!("Could not import TV SSH key: {e}"))?;
        }
        tv.root_key_path.clear();
        config.devices.push(DeviceEntry {
            id: Uuid::new_v4().to_string(),
            name: "LG TV".into(),
            settings: DeviceSettings::Tv(tv),
        });
    }
    let lamp = if old_lamp.exists() {
        Some(
            toml::from_str::<LampConfig>(&fs::read_to_string(old_lamp).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Invalid old lamp config: {e}"))?,
        )
    } else {
        LampConfig::import_existing_token()
    };
    if let Some(lamp) = lamp {
        config.devices.push(DeviceEntry {
            id: Uuid::new_v4().to_string(),
            name: "Mi Bedside Lamp".into(),
            settings: DeviceSettings::BedsideLamp(lamp),
        });
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_round_trip() {
        let mut tv = TvConfig::default();
        tv.root_private_key =
            "-----BEGIN OPENSSH PRIVATE KEY-----\nexample\n-----END OPENSSH PRIVATE KEY-----\n"
                .into();
        tv.webos_client_key = "pairing-secret".into();
        let mut lamp = LampConfig::default();
        lamp.token = "0123456789abcdef0123456789abcdef".into();
        let config = AppConfig {
            version: VERSION,
            xiaomi_accounts: vec![SavedXiaomiAccount {
                username: "user@example.com".into(),
                user_id: "12345".into(),
                ssecurity: "session-security".into(),
                service_token: "session-token".into(),
                region: "ru".into(),
            }],
            devices: vec![
                DeviceEntry {
                    id: Uuid::new_v4().to_string(),
                    name: "TV 1".into(),
                    settings: DeviceSettings::Tv(tv),
                },
                DeviceEntry {
                    id: Uuid::new_v4().to_string(),
                    name: "Lamp 1".into(),
                    settings: DeviceSettings::BedsideLamp(lamp),
                },
            ],
        };
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed = parse(&text).unwrap();
        assert_eq!(parsed.devices.len(), 2);
        assert_eq!(parsed.xiaomi_accounts[0].service_token, "session-token");
        let DeviceSettings::Tv(tv) = &parsed.devices[0].settings else {
            panic!("TV changed type")
        };
        assert_eq!(tv.webos_client_key, "pairing-secret");
        assert!(tv.root_private_key.contains("BEGIN OPENSSH"));
        let DeviceSettings::BedsideLamp(lamp) = &parsed.devices[1].settings else {
            panic!("lamp changed type")
        };
        assert_eq!(lamp.token, "0123456789abcdef0123456789abcdef");
        assert!(!text.contains("root_key_path"));
    }

    #[test]
    fn rejects_duplicate_device_ids() {
        let id = Uuid::new_v4().to_string();
        let config = AppConfig {
            version: VERSION,
            xiaomi_accounts: Vec::new(),
            devices: vec![
                DeviceEntry {
                    id: id.clone(),
                    name: "TV 1".into(),
                    settings: DeviceSettings::Tv(TvConfig::default()),
                },
                DeviceEntry {
                    id,
                    name: "TV 2".into(),
                    settings: DeviceSettings::Tv(TvConfig::default()),
                },
            ],
        };
        assert!(parse(&toml::to_string_pretty(&config).unwrap()).is_err());
    }

    #[test]
    fn migrates_existing_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let key = temp.path().join("old_key");
        fs::write(&key, "private-key-sample").unwrap();
        let old_tv = temp.path().join("settings.toml");
        let old_lamp = temp.path().join("bedside_lamp.toml");
        let tv = format!(
            "{}\nroot_key_path = {}\n",
            toml::to_string(&TvConfig::default()).unwrap(),
            toml::Value::String(key.to_string_lossy().into_owned())
        );
        fs::write(&old_tv, tv).unwrap();
        let mut lamp = LampConfig::default();
        lamp.token = "0123456789abcdef0123456789abcdef".into();
        fs::write(&old_lamp, toml::to_string(&lamp).unwrap()).unwrap();
        let migrated = migrate_files(&old_tv, None, &old_lamp).unwrap();
        assert_eq!(migrated.devices.len(), 2);
        let DeviceSettings::Tv(tv) = &migrated.devices[0].settings else {
            panic!("TV changed type")
        };
        assert_eq!(tv.root_private_key, "private-key-sample");
        let DeviceSettings::BedsideLamp(lamp) = &migrated.devices[1].settings else {
            panic!("lamp changed type")
        };
        assert_eq!(lamp.token, "0123456789abcdef0123456789abcdef");
    }
}
