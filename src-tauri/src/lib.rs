//! Tauri shell. Thin: it wires logging, state and commands; all logic is in
//! the core.

mod commands;
mod state;

use shim_core::paths::AppPaths;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn run() {
    let paths = match AppPaths::discover().and_then(|p| p.ensure().map(|()| p)) {
        Ok(p) => p,
        Err(e) => fatal(&e),
    };
    init_logging(&paths);
    let state = state::AppState::boot(paths);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::get_library,
            commands::scan_library,
            commands::plan_preview,
            commands::get_install,
            commands::get_components,
            commands::fetch_component,
            commands::inspect_file,
            commands::install_game,
            commands::remove_game,
            commands::rescan_game,
            commands::open_folder,
            commands::fetch_covers,
            commands::check_update,
            commands::relaunch_elevated,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Log to stderr (visible in `tauri dev`) and to `logs/shim.log`, which is
/// the only place a release build on Windows can be read from.
fn init_logging(paths: &AppPaths) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let stderr = fmt::layer().with_writer(std::io::stderr);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.logs_dir().join("shim.log"))
        .ok()
        .map(|f| {
            fmt::layer()
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(f))
        });
    tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(file)
        .init();
}

/// Nothing can run without a data directory. Release builds on Windows have
/// no console, so show a native message box before exiting.
fn fatal(e: &shim_core::Error) -> ! {
    let text = format!(
        "shim could not start.\n\n{}\n\n{}",
        e.user_message(),
        e.detail()
    );
    eprintln!("{text}");
    #[cfg(windows)]
    message_box(&text);
    std::process::exit(1)
}

#[cfg(windows)]
fn message_box(text: &str) {
    use std::os::windows::ffi::OsStrExt;
    let wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let (body, title) = (wide(text), wide("shim"));
    extern "system" {
        fn MessageBoxW(
            hwnd: *mut core::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            kind: u32,
        ) -> i32;
    }
    const MB_ICONERROR: u32 = 0x10;
    // SAFETY: both buffers are NUL-terminated UTF-16 and outlive the call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR,
        );
    }
}
