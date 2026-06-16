use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use deskcustom_config::Config;
use deskcustom_platform::{InputEvent, InputInject};
use deskcustom_proto::{decode, encode, Message, Role, TCP_PORT, UDP_PORT};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::clipboard::{self, PeerSender};
use crate::debug::{DebugLog, Metrics, log_startup, update_rtt};
use crate::keyboard::KeyboardPolicy;
use crate::mouse::MousePipeline;
use crate::runtime::RuntimeStatus;
use crate::tcp;

#[cfg(windows)]
use deskcustom_platform::WinInputInject;

#[cfg(not(windows))]
use deskcustom_platform::MacInputInject;

pub async fn run_client(
    config: Config,
    cancel: CancellationToken,
    metrics: Arc<Mutex<Metrics>>,
    status: Arc<Mutex<RuntimeStatus>>,
) -> Result<()> {
    log_startup(&config);

    let server = config.client.server_addr.clone().trim().to_string();
    if server.is_empty() {
        anyhow::bail!("Укажи IP Windows PC в настройках");
    }

    let debug = DebugLog::new(&config);
    let server_udp = resolve_udp_addr(&server)?;
    let udp = UdpSocket::bind(format!("{}:0", config.bind))
        .await
        .context("bind client UDP")?;

    let hello = encode(&Message::Hello {
        hostname: hostname(),
        role: Role::Client,
    });
    udp.send_to(&hello, server_udp).await?;

    let tcp_host = server.split(':').next().unwrap_or(&server);
    let tcp = TcpStream::connect(format!("{tcp_host}:{TCP_PORT}"))
        .await
        .with_context(|| format!("connect TCP {tcp_host}:{TCP_PORT}"))?;

    {
        let mut st = status.lock().await;
        st.connected_peer = Some(server.clone());
        st.message = format!("Connected to {server}");
    }

    info!(%server_udp, "connected to server");

    let (peer_tx, peer_rx) = clipboard::peer_channel();
    let (reader, writer) = tcp.into_split();
    clipboard::spawn_tcp_writer(writer, peer_rx);
    let _ = peer_tx
        .send(Message::Hello {
            hostname: hostname(),
            role: Role::Client,
        })
        .await;

    let mut keyboard =
        KeyboardPolicy::new(config.keyboard.clone(), config.clipboard.clone());
    let mouse_profile = config.client.mouse.clone();
    let mut mouse_pipe = MousePipeline::new(mouse_profile.clone());

    #[cfg(windows)]
    let mut inject = WinInputInject::new();
    #[cfg(not(windows))]
    let mut inject = MacInputInject::new();

    let udp_recv = recv_udp_loop(
        udp,
        &mut keyboard,
        &mut mouse_pipe,
        &mut inject,
        metrics.clone(),
        debug,
        mouse_profile,
        status.clone(),
        peer_tx.clone(),
        cancel.clone(),
    );

    let tcp_recv = tcp_read_loop(reader, cancel.clone());

    tokio::select! {
        res = udp_recv => res?,
        res = tcp_recv => res?,
        _ = cancel.cancelled() => {},
    }

    Ok(())
}

async fn recv_udp_loop(
    udp: UdpSocket,
    keyboard: &mut KeyboardPolicy,
    mouse_pipe: &mut MousePipeline,
    inject: &mut impl InputInject,
    metrics: Arc<Mutex<Metrics>>,
    debug: DebugLog,
    mouse_profile: deskcustom_config::MouseProfile,
    status: Arc<Mutex<RuntimeStatus>>,
    peer_tx: PeerSender,
    cancel: CancellationToken,
) -> Result<()> {
    let mut buf = vec![0u8; 2048];
    let mut last_ping = Instant::now();

    loop {
        if cancel.is_cancelled() {
            break;
        }

        if last_ping.elapsed() > Duration::from_secs(2) {
            let ping = encode(&Message::Ping { sent_us: now_us() });
            let _ = udp.send(&ping).await;
            last_ping = Instant::now();
        }

        let len = match tokio::time::timeout(Duration::from_millis(50), udp.recv(&mut buf)).await {
            Ok(Ok(len)) => len,
            Ok(Err(err)) => return Err(err.into()),
            Err(_) => continue,
        };

        let msg = decode(&buf[..len])?;
        match msg {
            Message::MouseMove { dx, dy, seq } => {
                if let Some(smoothed) = mouse_pipe.ingest(dx as i32, dy as i32) {
                    let ev = InputEvent::MouseMove(deskcustom_platform::MouseDelta {
                        dx: smoothed.dx as i32,
                        dy: smoothed.dy as i32,
                    });
                    inject.inject(&ev)?;
                    metrics.lock().await.mouse_recv += 1;
                    debug.event(
                        "in",
                        "mouse",
                        serde_json::json!({ "dx": smoothed.dx, "dy": smoothed.dy, "seq": seq }),
                    );
                }
            }
            Message::MouseButton { button, pressed } => {
                inject.inject(&InputEvent::MouseButton { button, pressed })?;
            }
            Message::Key {
                scancode,
                action,
                modifiers,
                seq,
            } => {
                let ev = InputEvent::Key {
                    scancode,
                    action,
                    modifiers,
                };
                if let Some(processed) = keyboard.process_inject(ev) {
                    inject.inject(&processed.event)?;
                    metrics.lock().await.keys_recv += 1;
                    debug.event("in", "key", serde_json::json!({ "scancode": scancode, "seq": seq }));
                    if let Some(trigger) = processed.clipboard {
                        clipboard::spawn_clipboard_push(peer_tx.clone(), trigger);
                    }
                }
            }
            Message::Pong { sent_us, recv_us } => {
                let now = now_us();
                let rtt = (now.saturating_sub(sent_us)) as f64 / 1000.0;
                update_rtt(&mut *metrics.lock().await, rtt);
                debug.event(
                    "in",
                    "pong",
                    serde_json::json!({ "sent_us": sent_us, "recv_us": recv_us, "rtt_ms": rtt }),
                );
            }
            Message::ScreenEnter { screen } => {
                mouse_pipe.update_profile(mouse_profile.clone());
                let mut st = status.lock().await;
                st.active_screen = Some(screen.clone());
                st.message = format!("Controlling {screen}");
                info!(%screen, "controlling remote screen");
            }
            Message::ScreenLeave { screen } => {
                let mut st = status.lock().await;
                st.active_screen = None;
                st.message = "Connected — waiting for cursor".into();
                info!(%screen, "returned to local screen");
            }
            _ => {}
        }
    }

    Ok(())
}

async fn tcp_read_loop(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    cancel: CancellationToken,
) -> Result<()> {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            frame = tcp::read_frame(&mut reader) => {
                let payload = frame?;
                let msg = decode(&payload)?;
                match msg {
                    Message::Hello { hostname, role } => {
                        info!(hostname, ?role, "server hello");
                    }
                    msg @ Message::ClipboardSet { .. } => {
                        clipboard::handle_incoming(msg).await;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn resolve_udp_addr(server: &str) -> Result<SocketAddr> {
    let host = server.split(':').next().unwrap_or(server);
    Ok(format!("{host}:{UDP_PORT}").parse()?)
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "deskcustom-client".into())
}

fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
