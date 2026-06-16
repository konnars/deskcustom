use std::collections::HashMap;
use std::path::PathBuf;

use deskcustom_config::Config;
use reqwest::Url;
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub available: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub notes: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateCheckInternal {
    pub result: UpdateCheckResult,
    pub pending: Option<PendingUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUpdate {
    pub version: String,
    pub url: String,
    pub local_installer: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateManifest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    platforms: HashMap<String, PlatformArtifact>,
}

#[derive(Debug, Deserialize)]
struct PlatformArtifact {
    url: String,
    #[serde(default)]
    signature: Option<String>,
}

pub async fn check_for_update(app: &AppHandle, cfg: &Config) -> Result<UpdateCheckInternal, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();

    if !cfg.update.enabled {
        return Ok(UpdateCheckInternal {
            result: UpdateCheckResult {
                available: false,
                current_version: current,
                latest_version: None,
                notes: None,
                error: None,
            },
            pending: None,
        });
    }

    let manifest_url = cfg.update.manifest_url.trim();
    if manifest_url.is_empty() {
        return try_tauri_updater(app, &current).await;
    }

    match fetch_manifest(manifest_url).await {
        Ok(manifest) => check_manifest(cfg, &current, manifest_url, manifest).await,
        Err(err) => {
            warn!(?err, "manifest fetch failed, trying signed updater");
            try_tauri_updater(app, &current).await
        }
    }
}

async fn try_tauri_updater(app: &AppHandle, current: &str) -> Result<UpdateCheckInternal, String> {
    use tauri_plugin_updater::UpdaterExt;

    match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => {
                let version = update.version.clone();
                Ok(UpdateCheckInternal {
                    result: UpdateCheckResult {
                        available: true,
                        current_version: current.to_string(),
                        latest_version: Some(version.clone()),
                        notes: update.body.clone(),
                        error: None,
                    },
                    pending: Some(PendingUpdate {
                        version,
                        url: String::new(),
                        local_installer: false,
                    }),
                })
            }
            Ok(None) => Ok(UpdateCheckInternal {
                result: UpdateCheckResult {
                    available: false,
                    current_version: current.to_string(),
                    latest_version: None,
                    notes: None,
                    error: None,
                },
                pending: None,
            }),
            Err(err) => Ok(UpdateCheckInternal {
                result: UpdateCheckResult {
                    available: false,
                    current_version: current.to_string(),
                    latest_version: None,
                    notes: None,
                    error: Some(err.to_string()),
                },
                pending: None,
            }),
        },
        Err(err) => Ok(UpdateCheckInternal {
            result: UpdateCheckResult {
                available: false,
                current_version: current.to_string(),
                latest_version: None,
                notes: None,
                error: Some(err.to_string()),
            },
            pending: None,
        }),
    }
}

async fn fetch_manifest(url: &str) -> Result<UpdateManifest, String> {
    reqwest::get(url)
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<UpdateManifest>()
        .await
        .map_err(|e| e.to_string())
}

async fn check_manifest(
    cfg: &Config,
    current: &str,
    manifest_url: &str,
    manifest: UpdateManifest,
) -> Result<UpdateCheckInternal, String> {
    let latest = manifest.version.clone();
    let current_v = Version::parse(current).map_err(|e| e.to_string())?;
    let latest_v = Version::parse(&latest).map_err(|e| e.to_string())?;

    if latest_v <= current_v {
        return Ok(UpdateCheckInternal {
            result: UpdateCheckResult {
                available: false,
                current_version: current.to_string(),
                latest_version: Some(latest),
                notes: None,
                error: None,
            },
            pending: None,
        });
    }

    let download_url = resolve_download_url(&manifest)?;
    if download_url.is_empty() {
        return Err("manifest has no download url for this platform".into());
    }

    if !cfg.update.trust_local_network && is_local_url(manifest_url) {
        return Err("local updates disabled — enable trust_local_network".into());
    }

    Ok(UpdateCheckInternal {
        result: UpdateCheckResult {
            available: true,
            current_version: current.to_string(),
            latest_version: Some(latest.clone()),
            notes: Some(manifest.notes),
            error: None,
        },
        pending: Some(PendingUpdate {
            version: latest,
            url: download_url,
            local_installer: true,
        }),
    })
}

fn resolve_download_url(manifest: &UpdateManifest) -> Result<String, String> {
    if let Some(url) = &manifest.url {
        if !url.is_empty() {
            return Ok(url.clone());
        }
    }

    let key = platform_key();
    manifest
        .platforms
        .get(&key)
        .map(|p| p.url.clone())
        .ok_or_else(|| format!("no artifact for platform {key}"))
}

fn platform_key() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "windows" => format!("windows-{arch}"),
        "macos" => format!("darwin-{arch}"),
        "linux" => format!("linux-{arch}"),
        other => format!("{other}-{arch}"),
    }
}

fn is_local_url(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .is_some_and(|host| {
            host == "localhost"
                || host.starts_with("127.")
                || host.starts_with("192.168.")
                || host.starts_with("10.")
                || host.starts_with("172.16.")
                || host.ends_with(".local")
        })
}

pub async fn install_update(app: &AppHandle, pending: PendingUpdate) -> Result<(), String> {
    if pending.local_installer {
        let path = download_installer(&pending.url).await?;
        launch_installer(app, &path)?;
        return Ok(());
    }

    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "update no longer available".to_string())?;

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;

    relaunch(app)
}

async fn download_installer(url: &str) -> Result<PathBuf, String> {
    info!(%url, "downloading update");
    let response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;

    let filename = url
        .rsplit('/')
        .next()
        .unwrap_or("deskcustom-update.exe");
    let path = std::env::temp_dir().join(filename);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

fn launch_installer(app: &AppHandle, path: &PathBuf) -> Result<(), String> {
    info!(?path, "launching installer");

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let status = if ext == "msi" {
            std::process::Command::new("msiexec")
                .args(["/i"])
                .arg(path)
                .arg("/passive")
                .arg("/norestart")
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(())
        } else {
            std::process::Command::new(path)
                .arg("/S")
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        status?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    app.exit(0);
    Ok(())
}

pub fn relaunch(app: &AppHandle) -> Result<(), String> {
    app.restart();
    Ok(())
}
