mod devices;

use devices::tv;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(tv::AppState::default())
        .invoke_handler(tauri::generate_handler![
            tv::get_config, tv::save_config, tv::get_state, tv::set_volume,
            tv::set_output, tv::get_paired_devices, tv::connect_headphones, tv::remote_button,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frog");
}
