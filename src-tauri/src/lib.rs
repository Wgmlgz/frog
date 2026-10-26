mod devices;

use devices::{bedside_lamp, tv};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(tv::AppState::default())
        .invoke_handler(tauri::generate_handler![
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frog");
}
