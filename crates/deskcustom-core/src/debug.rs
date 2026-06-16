use std::sync::Arc;
use std::time::Duration;

use deskcustom_config::Config;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Default)]
pub struct Metrics {
    pub mouse_sent: u64,
    pub mouse_recv: u64,
    pub keys_sent: u64,
    pub keys_recv: u64,
    pub rtt_ms: f64,
    pub jitter_ms: f64,
}

pub struct DebugLog {
    enabled: bool,
    json: bool,
}

impl Clone for DebugLog {
    fn clone(&self) -> Self {
        Self {
            enabled: self.enabled,
            json: self.json,
        }
    }
}

impl DebugLog {
    pub fn new(config: &Config) -> Self {
        Self {
            enabled: config.debug.log_events,
            json: config.debug.json_log,
        }
    }

    pub fn event(&self, direction: &str, kind: &str, detail: serde_json::Value) {
        if !self.enabled {
            return;
        }
        if self.json {
            let line = serde_json::json!({
                "ts": chrono_now_ms(),
                "dir": direction,
                "kind": kind,
                "detail": detail,
            });
            println!("{}", line);
        } else {
            info!(direction, kind, ?detail, "event");
        }
    }
}

pub struct MetricsReporter {
    interval: Duration,
    metrics: Arc<Mutex<Metrics>>,
}

impl MetricsReporter {
    pub fn new(config: &Config, metrics: Arc<Mutex<Metrics>>) -> Self {
        Self {
            interval: Duration::from_secs(config.debug.metrics_interval_secs.max(1)),
            metrics,
        }
    }

    pub async fn run(self) {
        let mut tick = tokio::time::interval(self.interval);
        loop {
            tick.tick().await;
            let m = self.metrics.lock().await;
            info!(
                mouse_sent = m.mouse_sent,
                mouse_recv = m.mouse_recv,
                keys_sent = m.keys_sent,
                keys_recv = m.keys_recv,
                rtt_ms = format!("{:.2}", m.rtt_ms),
                jitter_ms = format!("{:.2}", m.jitter_ms),
                "metrics"
            );
        }
    }
}

pub fn update_rtt(metrics: &mut Metrics, sample_ms: f64) {
    if metrics.rtt_ms == 0.0 {
        metrics.rtt_ms = sample_ms;
        metrics.jitter_ms = 0.0;
        return;
    }
    let diff = (sample_ms - metrics.rtt_ms).abs();
    metrics.jitter_ms = metrics.jitter_ms * 0.8 + diff * 0.2;
    metrics.rtt_ms = metrics.rtt_ms * 0.8 + sample_ms * 0.2;
}

pub fn chrono_now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn log_startup(config: &Config) {
    info!(
        role = ?config.role,
        bind = %config.bind,
        alt_shift_policy = %config.keyboard.alt_shift_policy,
        sync_layout = config.keyboard.sync_layout_from_server,
        "deskcustom starting"
    );
    if config.keyboard.sync_layout_from_server {
        warn!("sync_layout_from_server=true breaks Alt+Shift layout switching on clients");
    }
}
