use std::sync::Arc;

use anyhow::Result;
use deskcustom_config::{Config, Role};
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::client::run_client;
use crate::debug::Metrics;
use crate::server::run_server;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServicePhase {
    Stopped,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub phase: ServicePhase,
    pub message: String,
    pub connected_peer: Option<String>,
    pub active_screen: Option<String>,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            phase: ServicePhase::Stopped,
            message: String::new(),
            connected_peer: None,
            active_screen: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub rtt_ms: f64,
    pub jitter_ms: f64,
    pub mouse_sent: u64,
    pub mouse_recv: u64,
    pub keys_sent: u64,
    pub keys_recv: u64,
}

impl From<&Metrics> for MetricsSnapshot {
    fn from(m: &Metrics) -> Self {
        Self {
            rtt_ms: m.rtt_ms,
            jitter_ms: m.jitter_ms,
            mouse_sent: m.mouse_sent,
            mouse_recv: m.mouse_recv,
            keys_sent: m.keys_sent,
            keys_recv: m.keys_recv,
        }
    }
}

pub struct SharedRuntime {
    pub metrics: Arc<Mutex<Metrics>>,
    pub status: Arc<Mutex<RuntimeStatus>>,
}

impl Default for SharedRuntime {
    fn default() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(Metrics::default())),
            status: Arc::new(Mutex::new(RuntimeStatus::default())),
        }
    }
}

pub struct ServiceHandle {
    cancel: CancellationToken,
    task: JoinHandle<()>,
    pub shared: Arc<SharedRuntime>,
}

impl ServiceHandle {
    pub fn start(config: Config) -> Result<Self> {
        let cancel = CancellationToken::new();
        let shared = Arc::new(SharedRuntime::default());
        let token = cancel.clone();
        let shared_task = shared.clone();

        {
            let role = match config.role {
                Role::Server => "server",
                Role::Client => "client",
            };
            let shared = shared.clone();
            tokio::spawn(async move {
                let mut status = shared.status.lock().await;
                status.phase = ServicePhase::Running;
                status.message = format!("Running as {role}");
            });
        }

        let task = tokio::spawn(async move {
            let metrics = shared_task.metrics.clone();
            let status = shared_task.status.clone();
            let result = match config.role {
                Role::Server => run_server(config, token, metrics, status.clone()).await,
                Role::Client => run_client(config, token, metrics, status.clone()).await,
            };

            let mut st = shared_task.status.lock().await;
            match result {
                Ok(()) if st.phase == ServicePhase::Running => {
                    st.phase = ServicePhase::Stopped;
                    st.message = "Stopped".into();
                }
                Ok(()) => {}
                Err(err) => {
                    st.phase = ServicePhase::Error;
                    st.message = err.to_string();
                }
            }
        });

        Ok(Self {
            cancel,
            task,
            shared,
        })
    }

    pub async fn stop(self) {
        self.cancel.cancel();
        let _ = self.task.await;
        let mut status = self.shared.status.lock().await;
        if status.phase == ServicePhase::Running {
            status.phase = ServicePhase::Stopped;
            status.message = "Stopped".into();
        }
        status.connected_peer = None;
        status.active_screen = None;
    }

    pub fn is_running(&self) -> bool {
        !self.task.is_finished()
    }
}

pub async fn snapshot(handle: &ServiceHandle) -> (RuntimeStatus, MetricsSnapshot) {
    let status = handle.shared.status.lock().await.clone();
    let metrics = handle.shared.metrics.lock().await;
    (status, MetricsSnapshot::from(&*metrics))
}
