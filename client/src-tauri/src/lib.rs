use tauri::{WebviewUrl, WebviewWindowBuilder};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("client")
                .inner_size(800.0, 600.0);

            if let Ok(dir) = std::env::var("HAMLET_DATA_DIR") {
                builder = builder.data_directory(std::path::PathBuf::from(dir));
            }

            let window = builder.build()?;

            // WebKitGTK leaves media capture disabled and denies permission requests by default,
            // so `navigator.mediaDevices` is undefined and `getUserMedia` never resolves. Turn
            // the setting on and auto-grant user-media requests so the in-app voice settings
            // (mic level meter, test tone) behave the same as on Windows/macOS.
            #[cfg(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            ))]
            {
                use webkit2gtk::glib::Cast;
                use webkit2gtk::{
                    DeviceInfoPermissionRequest, PermissionRequestExt, SettingsExt,
                    UserMediaPermissionRequest, WebViewExt,
                };
                window.with_webview(|webview| {
                    let wv = webview.inner();
                    if let Some(settings) = WebViewExt::settings(&wv) {
                        settings.set_enable_media_stream(true);
                        settings.set_enable_mediasource(true);
                    }
                    wv.connect_permission_request(|_, req| {
                        // UserMediaPermissionRequest covers getUserMedia; DeviceInfoPermissionRequest
                        // covers enumerateDevices — without the latter allowed, WebKitGTK only
                        // reveals a single default device per kind with a generic label.
                        let is_media =
                            req.dynamic_cast_ref::<UserMediaPermissionRequest>().is_some();
                        let is_device_info =
                            req.dynamic_cast_ref::<DeviceInfoPermissionRequest>().is_some();
                        if is_media || is_device_info {
                            req.allow();
                            true
                        } else {
                            false
                        }
                    });
                })?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
