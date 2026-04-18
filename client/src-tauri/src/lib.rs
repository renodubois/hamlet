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

            // WKWebView hides `navigator.mediaDevices` in processes that don't have
            // `NSMicrophoneUsageDescription` in their bundle Info.plist. `tauri dev` runs
            // the raw binary outside a `.app`, so the Info.plist entry never applies and
            // the API is unreachable. Flip the private WKPreferences flag at runtime so
            // the voice settings work in dev. Release builds skip this (debug_assertions
            // is off) and rely on the merged Info.plist entries in the signed bundle —
            // that keeps the release binary free of private-selector references.
            #[cfg(all(target_os = "macos", debug_assertions))]
            {
                use objc2::runtime::AnyObject;
                use objc2::{msg_send, sel};
                use objc2_web_kit::WKWebView;

                window.with_webview(|webview| {
                    let wk_ptr = webview.inner().cast::<WKWebView>();
                    #[allow(unsafe_code)]
                    // SAFETY: Tauri guarantees `inner()` returns a valid, retained
                    // WKWebView pointer owned by the window. The closure runs on the
                    // main thread per Tauri's docs, which is where WKWebView APIs
                    // must be called. We only read through the pointer.
                    unsafe {
                        let wk: &WKWebView = &*wk_ptr;
                        let config = wk.configuration();
                        let prefs = config.preferences();
                        let prefs_obj: &AnyObject = prefs.as_ref();

                        if msg_send![prefs_obj, respondsToSelector: sel!(_setMediaDevicesEnabled:)]
                        {
                            let _: () = msg_send![prefs_obj, _setMediaDevicesEnabled: true];
                        }
                        if msg_send![prefs_obj, respondsToSelector: sel!(_setPeerConnectionEnabled:)]
                        {
                            let _: () = msg_send![prefs_obj, _setPeerConnectionEnabled: true];
                        }
                        if msg_send![prefs_obj, respondsToSelector: sel!(_setMediaStreamEnabled:)]
                        {
                            let _: () = msg_send![prefs_obj, _setMediaStreamEnabled: true];
                        }
                    }
                })?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
