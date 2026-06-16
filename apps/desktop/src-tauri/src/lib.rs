mod update;

use deskcustom_config::{Config, Role};
use deskcustom_core::{ServiceHandle, local_ipv4_addresses, server_display_addr, snapshot};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use update::{UpdateCheckResult, check_for_update, install_update, relaunch};

struct AppState {
    service: Mutex<Option<ServiceHandle>>,
    config: Mutex<Config>,
    pending_update: Mutex<Option<update::PendingUpdate>>,
}

#[derive(Serialize)]
struct UiConfig {
    role: String,
    server_addr: String,
    dpi_scale: f32,
    ewma_alpha: f32,
    poll_rate_cap_hz: u32,
    sync_layout: bool,
    alt_shift_local: bool,
    clipboard_sync: bool,
    ctrl_to_cmd: bool,
    update_enabled: bool,
    update_url: String,
    tcp_port: u16,
    udp_port: u16,
    app_version: String,
    local_ips: Vec<String>,
    server_display: String,
}

#[derive(Serialize)]
struct UiStatus {
    running: bool,
    phase: String,
    message: String,
    connected_peer: Option<String>,
    active_screen: Option<String>,
    rtt_ms: f64,
    jitter_ms: f64,
    mouse_sent: u64,
    mouse_recv: u64,
    suggest_update: bool,
}

#[tauri::command]
async fn get_ui_config(state: State<'_, AppState>) -> Result<UiConfig, String> {
    let cfg = state.config.lock().await;
    Ok(to_ui_config(&cfg))
}

#[tauri::command]
async fn save_ui_config(
    state: State<'_, AppState>,
    role: String,
    server_addr: String,
    dpi_scale: f32,
    ewma_alpha: f32,
    poll_rate_cap_hz: u32,
    alt_shift_local: bool,
    clipboard_sync: bool,
    ctrl_to_cmd: bool,
    update_enabled: bool,
    update_url: String,
) -> Result<(), String> {
    let mut cfg = state.config.lock().await;
    cfg.role = if role == "client" {
        Role::Client
    } else {
        Role::Server
    };
    cfg.client.server_addr = server_addr;
    cfg.client.mouse.dpi_scale = dpi_scale;
    cfg.client.mouse.ewma_alpha = ewma_alpha.clamp(0.05, 1.0);
    cfg.client.mouse.poll_rate_cap_hz = poll_rate_cap_hz.clamp(125, 1000);
    cfg.keyboard.sync_layout_from_server = false;
    cfg.keyboard.alt_shift_policy = if alt_shift_local {
        "local_only".into()
    } else {
        "forward".into()
    };
    cfg.clipboard.enabled = clipboard_sync;
    cfg.clipboard.sync_on_copy = clipboard_sync;
    cfg.keyboard.translate_ctrl_to_cmd_on_mac = ctrl_to_cmd;
    cfg.update.enabled = update_enabled;
    cfg.update.manifest_url = update_url.trim().to_string();
    cfg.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_service(state: State<'_, AppState>) -> Result<(), String> {
    let mut service_guard = state.service.lock().await;
    if let Some(handle) = service_guard.take() {
        if handle.is_running() {
            *service_guard = Some(handle);
            return Err("Уже запущено".into());
        }
        handle.stop().await;
    }

    let cfg = state.config.lock().await.clone();
    if cfg.role == Role::Client && cfg.client.server_addr.trim().is_empty() {
        return Err("Укажи IP Windows PC".into());
    }

    let handle = ServiceHandle::start(cfg).map_err(|e| e.to_string())?;
    *service_guard = Some(handle);
    Ok(())
}

#[tauri::command]
async fn stop_service(state: State<'_, AppState>) -> Result<(), String> {
    let mut service_guard = state.service.lock().await;
    if let Some(handle) = service_guard.take() {
        handle.stop().await;
    }
    Ok(())
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> Result<UiStatus, String> {
    let service_guard = state.service.lock().await;
    if let Some(handle) = service_guard.as_ref() {
        let (status, metrics) = snapshot(handle).await;
        let suggest_update = status.phase == deskcustom_core::ServicePhase::Error
            && !status.message.contains("уже занят");
        return Ok(UiStatus {
            running: handle.is_running(),
            phase: format!("{:?}", status.phase).to_lowercase(),
            message: status.message,
            connected_peer: status.connected_peer,
            active_screen: status.active_screen,
            rtt_ms: metrics.rtt_ms,
            jitter_ms: metrics.jitter_ms,
            mouse_sent: metrics.mouse_sent,
            mouse_recv: metrics.mouse_recv,
            suggest_update,
        });
    }

    Ok(UiStatus {
        running: false,
        phase: "stopped".into(),
        message: "Нажми «Запустить»".into(),
        connected_peer: None,
        active_screen: None,
        rtt_ms: 0.0,
        jitter_ms: 0.0,
        mouse_sent: 0,
        mouse_recv: 0,
        suggest_update: false,
    })
}

#[tauri::command]
async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdateCheckResult, String> {
    let cfg = state.config.lock().await.clone();
    let internal = check_for_update(&app, &cfg).await?;
    if let Some(pending) = internal.pending {
        *state.pending_update.lock().await = Some(pending);
    }
    Ok(internal.result)
}

#[tauri::command]
async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pending = state.pending_update.lock().await.clone();
    let Some(pending) = pending else {
        return Err("Нет загруженного обновления — сначала проверь".into());
    };
    install_update(&app, pending).await
}

#[tauri::command]
async fn relaunch_app(app: AppHandle) -> Result<(), String> {
    relaunch(&app)
}

fn to_ui_config(cfg: &Config) -> UiConfig {
    UiConfig {
        role: match cfg.role {
            Role::Server => "server".into(),
            Role::Client => "client".into(),
        },
        server_addr: cfg.client.server_addr.clone(),
        dpi_scale: cfg.client.mouse.dpi_scale,
        ewma_alpha: cfg.client.mouse.ewma_alpha,
        poll_rate_cap_hz: cfg.client.mouse.poll_rate_cap_hz,
        sync_layout: cfg.keyboard.sync_layout_from_server,
        alt_shift_local: cfg.keyboard.alt_shift_policy == "local_only",
        clipboard_sync: cfg.clipboard.enabled && cfg.clipboard.sync_on_copy,
        ctrl_to_cmd: cfg.keyboard.translate_ctrl_to_cmd_on_mac,
        update_enabled: cfg.update.enabled,
        update_url: cfg.update.manifest_url.clone(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        local_ips: local_ipv4_addresses(),
        server_display: server_display_addr(cfg.tcp_port),
        tcp_port: cfg.tcp_port,
        udp_port: cfg.udp_port,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .init();

    let config = Config::load_or_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            service: Mutex::new(None),
            config: Mutex::new(config),
            pending_update: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_ui_config,
            save_ui_config,
            start_service,
            stop_service,
            get_status,
            check_app_update,
            install_app_update,
            relaunch_app,
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("Deskcustom");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running Deskcustom");
}
