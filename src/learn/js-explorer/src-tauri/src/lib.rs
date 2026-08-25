// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[cfg(desktop)]
use tauri::Manager;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// Stronghold needs a password -> 32-byte-key hash function. This is the pattern
// from Tauri's own docs (argon2id) — fine for this learn/demo app, but a real
// app should use a unique salt per vault rather than one hard-coded string.
fn stronghold_hash_key(password: &str) -> Vec<u8> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(10_000, 10, 4, Some(32)).expect("valid argon2 params");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), b"js-explorer-demo-salt", &mut key)
        .expect("failed to hash password");
    key.to_vec()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_stronghold::Builder::new(stronghold_hash_key).build())
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_websocket::init())
        .plugin(tauri_plugin_upload::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(tauri_plugin_log::log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // fs must come before persisted-scope — it hooks into fs's scope, so it
        // needs fs already registered to attach to.
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_persisted_scope::init())
        .plugin(tauri_plugin_opener::init());

    // Desktop-only plugins — matches the target-gated deps in Cargo.toml, so
    // none of these are even compiled in on a mobile build.
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_autostart::Builder::new().build())
            .plugin(tauri_plugin_cli::init())
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_global_shortcut::Builder::new().build())
            // Serves the frontend over real http://localhost — see that card's own
            // "considerable security risk" caveat; a fixed dev-only port is fine here.
            .plugin(tauri_plugin_localhost::Builder::new(9527).build())
            .plugin(tauri_plugin_positioner::init())
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                // a second launch happened — focus the existing window instead of opening a new one
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_focus();
                }
            }))
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_window_state::Builder::new().build());
    }
    // Mobile-only plugins — matches the target-gated deps in Cargo.toml.
    #[cfg(mobile)]
    {
        builder = builder
            .plugin(tauri_plugin_barcode_scanner::init())
            .plugin(tauri_plugin_biometric::init())
            .plugin(tauri_plugin_geolocation::init())
            .plugin(tauri_plugin_haptics::init())
            .plugin(tauri_plugin_nfc::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
