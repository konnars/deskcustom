use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_role")]
    pub role: Role,

    #[serde(default = "default_bind")]
    pub bind: String,

    #[serde(default = "default_tcp_port")]
    pub tcp_port: u16,

    #[serde(default = "default_udp_port")]
    pub udp_port: u16,

    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub client: ClientConfig,

    #[serde(default)]
    pub screens: Vec<Screen>,

    #[serde(default)]
    pub keyboard: KeyboardConfig,

    #[serde(default)]
    pub clipboard: ClipboardConfig,

    #[serde(default)]
    pub debug: DebugConfig,

    #[serde(default)]
    pub update: UpdateConfig,
}

fn default_role() -> Role {
    Role::Server
}

fn default_bind() -> String {
    "0.0.0.0".into()
}

fn default_tcp_port() -> u16 {
    24800
}

fn default_udp_port() -> u16 {
    24801
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Server,
    Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_true")]
    pub capture_input: bool,

    #[serde(default = "default_edge_px")]
    pub edge_threshold_px: u32,
}

fn default_true() -> bool {
    true
}

fn default_edge_px() -> u32 {
    2
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            capture_input: true,
            edge_threshold_px: default_edge_px(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default)]
    pub server_addr: String,

    #[serde(default = "default_mouse_profile")]
    pub mouse: MouseProfile,
}

fn default_mouse_profile() -> MouseProfile {
    MouseProfile::default()
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_addr: String::new(),
            mouse: MouseProfile::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseProfile {
    #[serde(default = "default_one_f")]
    pub dpi_scale: f32,

    #[serde(default = "default_poll_cap")]
    pub poll_rate_cap_hz: u32,

    #[serde(default = "default_smoothing")]
    pub smoothing: String,

    #[serde(default = "default_alpha")]
    pub ewma_alpha: f32,

    #[serde(default = "default_coalesce_us")]
    pub coalesce_us: u32,

    #[serde(default = "default_drop_us")]
    pub drop_stale_us: u32,
}

fn default_one_f() -> f32 {
    1.0
}
fn default_poll_cap() -> u32 {
    500
}
fn default_smoothing() -> String {
    "ewma".into()
}
fn default_alpha() -> f32 {
    0.45
}
fn default_coalesce_us() -> u32 {
    800
}
fn default_drop_us() -> u32 {
    8000
}

impl Default for MouseProfile {
    fn default() -> Self {
        Self {
            dpi_scale: default_one_f(),
            poll_rate_cap_hz: default_poll_cap(),
            smoothing: default_smoothing(),
            ewma_alpha: default_alpha(),
            coalesce_us: default_coalesce_us(),
            drop_stale_us: default_drop_us(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screen {
    pub name: String,
    #[serde(default)]
    pub alias: String,

    #[serde(default)]
    pub switch_edge: Option<String>,

    #[serde(default)]
    pub mouse: Option<MouseProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardConfig {
    #[serde(default)]
    pub sync_layout_from_server: bool,

    #[serde(default = "default_modifier_policy")]
    pub alt_shift_policy: String,

    /// Ctrl+C/V on Windows keyboard → Cmd+C/V on Mac client.
    #[serde(default = "default_true")]
    pub translate_ctrl_to_cmd_on_mac: bool,
}

fn default_modifier_policy() -> String {
    "local_only".into()
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            sync_layout_from_server: false,
            alt_shift_policy: default_modifier_policy(),
            translate_ctrl_to_cmd_on_mac: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// After Ctrl+C / Cmd+C, send clipboard text to the other PC.
    #[serde(default = "default_true")]
    pub sync_on_copy: bool,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sync_on_copy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugConfig {
    #[serde(default)]
    pub log_events: bool,

    #[serde(default = "default_metrics_interval")]
    pub metrics_interval_secs: u64,

    #[serde(default)]
    pub json_log: bool,
}

fn default_metrics_interval() -> u64 {
    5
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            log_events: false,
            metrics_interval_secs: default_metrics_interval(),
            json_log: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// URL to latest.json — LAN or GitHub Releases.
    #[serde(default)]
    pub manifest_url: String,

    /// Allow unsigned updates from private/local IPs (home LAN).
    #[serde(default = "default_true")]
    pub trust_local_network: bool,

    #[serde(default = "default_update_interval")]
    pub check_interval_secs: u64,
}

fn default_update_interval() -> u64 {
    1800
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            manifest_url: String::new(),
            trust_local_network: true,
            check_interval_secs: default_update_interval(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            role: default_role(),
            bind: default_bind(),
            tcp_port: default_tcp_port(),
            udp_port: default_udp_port(),
            server: ServerConfig::default(),
            client: ClientConfig::default(),
            screens: Vec::new(),
            keyboard: KeyboardConfig::default(),
            clipboard: ClipboardConfig::default(),
            debug: DebugConfig::default(),
            update: UpdateConfig::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("deskcustom")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        let mut cfg = Self::load_path(&path).unwrap_or_else(|_| Self::default_with_screens());

        #[cfg(target_os = "macos")]
        {
            if !path.exists() {
                cfg.role = Role::Client;
            }
        }
        #[cfg(windows)]
        {
            if !path.exists() {
                cfg.role = Role::Server;
            }
        }

        if !path.exists() {
            let _ = cfg.save_path(&path);
        }
        cfg
    }

    pub fn default_with_screens() -> Self {
        let mut cfg = Self::default();
        if cfg.screens.is_empty() {
            cfg.screens = vec![
                Screen {
                    name: "windows-pc".into(),
                    alias: "win".into(),
                    switch_edge: None,
                    mouse: None,
                },
                Screen {
                    name: "macbook".into(),
                    alias: "mac".into(),
                    switch_edge: Some("right".into()),
                    mouse: None,
                },
            ];
        }
        cfg
    }

    pub fn load_path(path: &PathBuf) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn load(path: &str) -> Result<Self, ConfigError> {
        Self::load_path(&PathBuf::from(path))
    }

    pub fn save_path(&self, path: &PathBuf) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_path(&Self::config_path())
    }

    pub fn mouse_profile_for(&self, screen: &str) -> MouseProfile {
        self.screens
            .iter()
            .find(|s| s.name == screen || s.alias == screen)
            .and_then(|s| s.mouse.clone())
            .unwrap_or_else(|| self.client.mouse.clone())
    }
}
