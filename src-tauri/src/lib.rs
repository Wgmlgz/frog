mod cloud;
mod config;
mod devices;
mod discovery;
mod setup;

use devices::bedside_lamp;
#[cfg(not(target_os = "android"))]
use devices::tv;

pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_clipboard_manager::init());
    #[cfg(not(target_os = "android"))]
    let builder = builder
        .plugin(tauri_plugin_shell::init())
        .manage(tv::AppState::default());
    let builder = builder.manage(setup::SetupState::default());
    #[cfg(not(target_os = "android"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        config::list_devices,
        config::list_xiaomi_accounts,
        discovery::discover_devices,
        discovery::probe_device,
        config::remove_device,
        config::export_config_to_clipboard,
        config::import_config_from_clipboard,
        setup::finish_manual_lamp,
        setup::begin_xiaomi_setup,
        setup::submit_xiaomi_captcha,
        setup::submit_xiaomi_email,
        setup::finish_cloud_lamp,
        setup::use_xiaomi_account,
        setup::pair_new_tv,
        setup::finish_new_tv,
        setup::cancel_device_setup,
        tv::get_config,
        tv::save_config,
        tv::get_state,
        tv::set_volume,
        tv::set_output,
        tv::get_paired_devices,
        tv::connect_headphones,
        tv::remote_button,
        bedside_lamp::get_lamp_config,
        bedside_lamp::save_lamp_config,
        bedside_lamp::get_lamp_state,
        bedside_lamp::set_lamp_power,
        bedside_lamp::set_lamp_brightness,
        bedside_lamp::set_lamp_temperature,
        bedside_lamp::set_lamp_rgb,
    ]);
    #[cfg(target_os = "android")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        config::list_devices,
        config::list_xiaomi_accounts,
        discovery::discover_devices,
        discovery::probe_device,
        config::remove_device,
        config::export_config_to_clipboard,
        config::import_config_from_clipboard,
        setup::finish_manual_lamp,
        setup::begin_xiaomi_setup,
        setup::submit_xiaomi_captcha,
        setup::submit_xiaomi_email,
        setup::finish_cloud_lamp,
        setup::use_xiaomi_account,
        setup::cancel_device_setup,
        bedside_lamp::get_lamp_config,
        bedside_lamp::save_lamp_config,
        bedside_lamp::get_lamp_state,
        bedside_lamp::set_lamp_power,
        bedside_lamp::set_lamp_brightness,
        bedside_lamp::set_lamp_temperature,
        bedside_lamp::set_lamp_rgb,
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("error while running Frog");
}
