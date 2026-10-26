use crate::{
    cloud::{CloudSession, LoginStep},
    config::{self, DeviceEntry, DeviceSettings, DeviceSummary},
    devices::{
        bedside_lamp::{self, LampConfig},
        tv::TvConfig,
    },
};
use std::{
    collections::HashMap,
    net::Ipv4Addr,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, State};
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct SetupState {
    cloud: Arc<Mutex<Option<CloudSession>>>,
    paired_tvs: Arc<Mutex<HashMap<String, String>>>,
}

fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err("Device name must be 1 to 80 characters".into());
    }
    Ok(name.to_owned())
}

fn validate_ip(ip: &str) -> Result<(), String> {
    ip.parse::<Ipv4Addr>()
        .map_err(|_| "Enter a valid IPv4 address")?;
    Ok(())
}

fn save_lamp(
    app: &AppHandle,
    name: &str,
    ip: &str,
    device_id: u32,
    token: &str,
    expected_model: Option<&str>,
) -> Result<DeviceSummary, String> {
    let name = validate_name(name)?;
    validate_ip(ip)?;
    let lamp = LampConfig {
        ip: ip.into(),
        device_id,
        token: token.into(),
    };
    lamp.validate()?;
    if lamp.token.is_empty() {
        return Err("A device token is required".into());
    }
    bedside_lamp::read_state(&lamp)
        .map_err(|e| format!("Could not verify local lamp control: {e}"))?;
    let model = bedside_lamp::device_model(&lamp)
        .map_err(|e| format!("Could not identify lamp model: {e}"))?;
    if let Some(expected) = expected_model {
        if model != expected {
            return Err(format!(
                "The device reports model {model}, but Xiaomi listed {expected}"
            ));
        }
    }
    let settings = match model.as_str() {
        "yeelink.light.bslamp1" => DeviceSettings::BedsideLamp(lamp),
        "yeelink.light.color4" => DeviceSettings::ColorBulb(lamp),
        _ => return Err(format!("Frog does not yet have controls for {model}")),
    };
    config::update(app, |store| {
        if store.devices.iter().any(|item| match &item.settings {
            DeviceSettings::BedsideLamp(existing) | DeviceSettings::ColorBulb(existing) => {
                existing.device_id == device_id || existing.ip == ip
            }
            _ => false,
        }) {
            return Err("This lamp is already connected".into());
        }
        let entry = DeviceEntry {
            id: Uuid::new_v4().to_string(),
            name,
            settings,
        };
        let summary = DeviceSummary::from(&entry);
        store.devices.push(entry);
        Ok(summary)
    })
}

#[tauri::command]
pub(crate) async fn finish_manual_lamp(
    app: AppHandle,
    name: String,
    ip: String,
    device_id: u32,
    token: String,
) -> Result<DeviceSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        save_lamp(&app, &name, &ip, device_id, &token, None)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn begin_xiaomi_setup(
    app: AppHandle,
    state: State<'_, SetupState>,
    username: String,
    password: String,
    region: String,
) -> Result<LoginStep, String> {
    let sessions = state.cloud.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut session = CloudSession::new(username, password, region)?;
        let step = session.begin()?;
        if let Some(account) = session.saved_account() {
            config::save_xiaomi_account(&app, account)?;
        }
        *sessions.lock().map_err(|e| e.to_string())? = Some(session);
        Ok(step)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn submit_xiaomi_captcha(
    app: AppHandle,
    state: State<'_, SetupState>,
    code: String,
) -> Result<LoginStep, String> {
    let sessions = state.cloud.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = sessions.lock().map_err(|e| e.to_string())?;
        let step = guard
            .as_mut()
            .ok_or("Start Xiaomi sign-in first")?
            .captcha(&code)?;
        if let Some(account) = guard.as_ref().and_then(CloudSession::saved_account) {
            config::save_xiaomi_account(&app, account)?;
        }
        Ok(step)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn submit_xiaomi_email(
    app: AppHandle,
    state: State<'_, SetupState>,
    code: String,
) -> Result<LoginStep, String> {
    let sessions = state.cloud.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = sessions.lock().map_err(|e| e.to_string())?;
        let step = guard
            .as_mut()
            .ok_or("Start Xiaomi sign-in first")?
            .email_code(&code)?;
        if let Some(account) = guard.as_ref().and_then(CloudSession::saved_account) {
            config::save_xiaomi_account(&app, account)?;
        }
        Ok(step)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn finish_cloud_lamp(
    app: AppHandle,
    state: State<'_, SetupState>,
    name: String,
    ip: String,
    device_id: u32,
) -> Result<DeviceSummary, String> {
    let sessions = state.cloud.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = sessions.lock().map_err(|e| e.to_string())?;
        let session = guard.as_ref().ok_or("Sign in to Xiaomi first")?;
        let device = session
            .devices
            .iter()
            .find(|device| device.did == device_id.to_string())
            .ok_or("This local device was not found in your Xiaomi account")?;
        if !matches!(
            device.model.as_str(),
            "yeelink.light.bslamp1" | "yeelink.light.color4"
        ) {
            return Err(format!(
                "This model ({}) is not a supported lamp",
                device.model
            ));
        }
        save_lamp(
            &app,
            &name,
            &ip,
            device_id,
            &device.token,
            Some(&device.model),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn use_xiaomi_account(
    app: AppHandle,
    state: State<'_, SetupState>,
    user_id: String,
) -> Result<LoginStep, String> {
    let sessions = state.cloud.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let saved = config::read(&app)?
            .xiaomi_accounts
            .into_iter()
            .find(|item| item.user_id == user_id)
            .ok_or("Saved Xiaomi account not found")?;
        let mut session = CloudSession::from_saved(&saved)?;
        let step = session.refresh().map_err(|e| {
            format!("Saved Xiaomi session could not be used: {e}. Sign in again to renew it.")
        })?;
        *sessions.lock().map_err(|e| e.to_string())? = Some(session);
        Ok(step)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub(crate) async fn pair_new_tv(
    app: AppHandle,
    state: State<'_, SetupState>,
    ip: String,
) -> Result<(), String> {
    validate_ip(&ip)?;
    let key = crate::devices::tv::pair_ip(&app, &ip).await?;
    state
        .paired_tvs
        .lock()
        .map_err(|e| e.to_string())?
        .insert(ip, key);
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub(crate) fn finish_new_tv(
    app: AppHandle,
    state: State<'_, SetupState>,
    name: String,
    ip: String,
    root_private_key: String,
) -> Result<DeviceSummary, String> {
    let name = validate_name(&name)?;
    validate_ip(&ip)?;
    let pairing_key = state
        .paired_tvs
        .lock()
        .map_err(|e| e.to_string())?
        .get(&ip)
        .cloned()
        .ok_or("Pair this TV before adding it")?;
    let mut tv = TvConfig::default();
    tv.tv_ip = ip.clone();
    tv.webos_client_key = pairing_key;
    tv.root_private_key = root_private_key.replace("\r\n", "\n");
    let summary = config::update(&app, |store| {
        if store.devices.iter().any(|device| matches!(&device.settings, DeviceSettings::Tv(existing) if existing.tv_ip == ip)) {
            return Err("This TV is already connected".into());
        }
        let entry = DeviceEntry {
            id: Uuid::new_v4().to_string(),
            name,
            settings: DeviceSettings::Tv(tv),
        };
        let summary = DeviceSummary::from(&entry);
        store.devices.push(entry);
        Ok(summary)
    })?;
    state
        .paired_tvs
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&ip);
    Ok(summary)
}

#[tauri::command]
pub(crate) fn cancel_device_setup(state: State<'_, SetupState>) -> Result<(), String> {
    *state.cloud.lock().map_err(|e| e.to_string())? = None;
    state.paired_tvs.lock().map_err(|e| e.to_string())?.clear();
    Ok(())
}
