use crate::{
    config::{self, DeviceSettings},
    devices::{
        bedside_lamp::{self, LampConfig},
        tv::{self, TvConfig},
    },
};
use jni::{
    objects::{JObject, JString},
    sys::{jboolean, jstring},
    JNIEnv,
};
use serde_json::{json, Value};
use std::{
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    thread,
};

fn run(config_path: &str, toggle: bool) -> Result<&'static str, String> {
    let source = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let store = config::parse(&source)?;
    let lamps: Vec<&LampConfig> = store
        .devices
        .iter()
        .filter_map(|device| match &device.settings {
            DeviceSettings::BedsideLamp(config) | DeviceSettings::ColorBulb(config) => Some(config),
            DeviceSettings::Tv(_) => None,
        })
        .collect();
    if lamps.is_empty() {
        return Ok("empty");
    }

    let states: Vec<_> = thread::scope(|scope| {
        let jobs: Vec<_> = lamps
            .iter()
            .map(|lamp| scope.spawn(move || bedside_lamp::read_state(lamp)))
            .collect();
        jobs.into_iter()
            .map(|job| {
                job.join()
                    .unwrap_or_else(|_| Err("Lamp status worker failed".into()))
            })
            .collect()
    });
    if !toggle {
        if states.iter().any(Result::is_err) {
            return Ok("unavailable");
        }
        return Ok(
            if states
                .iter()
                .all(|state| state.as_ref().is_ok_and(|state| state.power))
            {
                "on"
            } else if states
                .iter()
                .all(|state| state.as_ref().is_ok_and(|state| !state.power))
            {
                "off"
            } else {
                "mixed"
            },
        );
    }

    // Match the app's All lights button: a partial or unavailable group is turned on.
    let all_on = states
        .iter()
        .all(|state| state.as_ref().is_ok_and(|state| state.power));
    let target_on = !all_on;
    let failures = thread::scope(|scope| {
        let jobs: Vec<_> = lamps
            .iter()
            .map(|lamp| scope.spawn(move || bedside_lamp::set_power_direct(lamp, target_on)))
            .collect();
        jobs.into_iter()
            .enumerate()
            .filter_map(|(index, job)| match job.join() {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(format!("Lamp {}: {error}", index + 1)),
                Err(_) => Some(format!("Lamp {}: command worker failed", index + 1)),
            })
            .collect::<Vec<_>>()
    });
    if !failures.is_empty() {
        return Err(failures.join("; "));
    }
    Ok(if target_on { "on" } else { "off" })
}

fn store(config_path: &str) -> Result<config::AppConfig, String> {
    config::parse(&fs::read_to_string(config_path).map_err(|e| e.to_string())?)
}

fn tv_config<'a>(store: &'a config::AppConfig, id: &str) -> Result<&'a TvConfig, String> {
    store
        .devices
        .iter()
        .find_map(|device| {
            if !id.is_empty() && device.id != id {
                return None;
            }
            match &device.settings {
                DeviceSettings::Tv(config) => Some(config),
                _ => None,
            }
        })
        .ok_or_else(|| "No TV is configured for this widget".into())
}

fn lamp_config<'a>(store: &'a config::AppConfig, id: &str) -> Result<&'a LampConfig, String> {
    store
        .devices
        .iter()
        .find_map(|device| {
            if !id.is_empty() && device.id != id {
                return None;
            }
            match &device.settings {
                DeviceSettings::BedsideLamp(config) | DeviceSettings::ColorBulb(config) => {
                    Some(config)
                }
                _ => None,
            }
        })
        .ok_or_else(|| "No lamp is configured for this widget".into())
}

fn jstring(env: &mut JNIEnv, value: JString) -> Result<String, String> {
    env.get_string(&value)
        .map(|value| value.into())
        .map_err(|error| error.to_string())
}

fn response(env: &mut JNIEnv, result: Result<Value, String>) -> jstring {
    let value = match result {
        Ok(value) => value,
        Err(error) => json!({"error":error}),
    };
    env.new_string(value.to_string())
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_wgmlgz_frog_tvpython_LampWidgetNative_devices(
    mut env: JNIEnv,
    _receiver: JObject,
    config_path: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        let store = store(&jstring(&mut env, config_path)?)?;
        Ok(json!({"devices":store.devices.iter().map(|device| json!({
            "id":device.id,"name":device.name,"type":match &device.settings {
                DeviceSettings::Tv(_) => "tv", _ => "lamp",
            }
        })).collect::<Vec<_>>() }))
    }))
    .unwrap_or_else(|_| Err("Could not list widget devices".into()));
    response(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_wgmlgz_frog_tvpython_LampWidgetNative_tvConfig(
    mut env: JNIEnv,
    _receiver: JObject,
    config_path: JString,
    device_id: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        let path = jstring(&mut env, config_path)?;
        let id = jstring(&mut env, device_id)?;
        let store = store(&path)?;
        let config = tv_config(&store, &id)?;
        Ok(json!({"ip":config.tv_ip,"mac":config.tv_mac,
            "broadcast":config.broadcast_ip,"clientKey":config.webos_client_key}))
    }))
    .unwrap_or_else(|_| Err("Could not read TV widget settings".into()));
    response(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_wgmlgz_frog_tvpython_LampWidgetNative_tvAudio(
    mut env: JNIEnv,
    _receiver: JObject,
    config_path: JString,
    device_id: JString,
    action: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        let path = jstring(&mut env, config_path)?;
        let id = jstring(&mut env, device_id)?;
        let action = jstring(&mut env, action)?;
        let store = store(&path)?;
        let config = tv_config(&store, &id)?;
        let dir = Path::new(&path)
            .parent()
            .ok_or("Missing widget settings directory")?;
        let state = tv::widget_audio(config, dir, &action)?;
        Ok(json!({"volume":state.widget_volume(),"output":state.widget_output()}))
    }))
    .unwrap_or_else(|_| Err("TV audio widget failed".into()));
    response(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_wgmlgz_frog_tvpython_LampWidgetNative_lamp(
    mut env: JNIEnv,
    _receiver: JObject,
    config_path: JString,
    device_id: JString,
    toggle: jboolean,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        let path = jstring(&mut env, config_path)?;
        let id = jstring(&mut env, device_id)?;
        let store = store(&path)?;
        let lamp = lamp_config(&store, &id)?;
        let state = bedside_lamp::read_state(lamp)?;
        if toggle != 0 {
            bedside_lamp::set_power_direct(lamp, !state.power)?;
            Ok(json!({"state":if state.power { "off" } else { "on" }}))
        } else {
            Ok(json!({"state":if state.power { "on" } else { "off" }}))
        }
    }))
    .unwrap_or_else(|_| Err("Lamp widget failed".into()));
    response(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_wgmlgz_frog_tvpython_LampWidgetNative_run(
    mut env: JNIEnv,
    _receiver: JObject,
    config_path: JString,
    toggle: jboolean,
) -> jstring {
    let output = catch_unwind(AssertUnwindSafe(|| {
        let path: String = env
            .get_string(&config_path)
            .map_err(|e| e.to_string())?
            .into();
        run(&path, toggle != 0)
    }));
    let response = match output {
        Ok(Ok(state)) => json!({ "state": state }),
        Ok(Err(error)) => json!({ "state": "unavailable", "error": error }),
        Err(_) => json!({ "state": "unavailable", "error": "Lamp widget failed" }),
    };
    match env.new_string(response.to_string()) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
