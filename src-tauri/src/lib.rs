mod config;
mod devices;

use devices::bedside_lamp;
#[cfg(not(target_os = "android"))]
use devices::tv;

pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_clipboard_manager::init());
    #[cfg(not(target_os = "android"))]
    let builder = builder
        .plugin(tauri_plugin_shell::init())
        .manage(tv::AppState::default());
    #[cfg(not(target_os = "android"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
            config::list_devices,
            config::add_device,
            config::remove_device,
            config::export_config_to_clipboard,
            config::import_config_from_clipboard,
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
        ]);
    #[cfg(target_os = "android")]
    let builder = builder.invoke_handler(tauri::generate_handler![
            config::list_devices,
            config::add_device,
            config::remove_device,
            config::export_config_to_clipboard,
            config::import_config_from_clipboard,
            bedside_lamp::get_lamp_config,
            bedside_lamp::save_lamp_config,
            bedside_lamp::get_lamp_state,
            bedside_lamp::set_lamp_power,
            bedside_lamp::set_lamp_brightness,
            bedside_lamp::set_lamp_temperature,
        ]);
    builder
        .run(tauri::generate_context!())
        .expect("error while running Frog");
}
