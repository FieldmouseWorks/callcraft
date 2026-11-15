#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Desktop shell: start the loopback server in-process, then point the webview
//! at it. One API, one origin, one test surface. The webview never gets direct
//! filesystem access; native dialogs are added through Tauri commands.

/// Diagnostics endpoint for the Windows spike: proves that a page served from
/// the loopback origin can reach Tauri IPC.
#[tauri::command]
fn app_info() -> String {
    format!(
        "Callcraft {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    )
}

fn main() {
    let token = uuid::Uuid::new_v4().to_string();
    let (port, _server) = callcraft_server::start(0, token.clone()).expect("start loopback server");
    let url = format!("http://127.0.0.1:{port}/?token={token}");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![app_info])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External(url.parse().expect("valid loopback URL")),
            )
            .title("Callcraft")
            .inner_size(1100.0, 760.0)
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri run");
}
