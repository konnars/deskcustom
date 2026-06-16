use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use deskcustom_config::Config;
use deskcustom_platform::InputCapture;
use deskcustom_proto::{encode, Message, Role};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::clipboard::{self, PeerSender};
use crate::debug::{DebugLog, Metrics, log_startup};
use crate::runtime::RuntimeStatus;
use crate::netbind::{bind_tcp, bind_udp};
use crate::netutil::tune_tcp;
use crate::tcp;

#[cfg(windows)]
use deskcustom_platform::WinInputCapture;

#[cfg(not(windows))]
use deskcustom_platform::MacInputCapture;

pub async fn run_server(
    config: Config,
    cancel: CancellationToken,
    metrics: Arc<Mutex<Metrics>>,
    status: Arc<Mutex<RuntimeStatus>>,
) -> Result<()> {
    log_startup(&config);

    let udp_addr = format!("{}:{}", config.bind, config.udp_port);
    let tcp_addr = format!("{}:{}", config.bind, config.tcp_port);

    let udp = bind_udp(&udp_addr).await?;
    let tcp = bind_tcp(&tcp_addr).await?;

    info!(%udp_addr, %tcp_addr, "server listening");

    let active_client: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));
    let active_screen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let peer_tx: Arc<Mutex<Option<PeerSender>>> = Arc::new(Mutex::new(None));

    let udp_task = {
        let debug = DebugLog::new(&config);
        let active_client = active_client.clone();
        let status = status.clone();
        let cancel = cancel.clone();
        async move {
            let mut buf = vec![0u8; 2048];
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    incoming = udp.recv_from(&mut buf) => {
                        let Ok((len, peer)) = incoming else { continue };
                        if let Ok(msg) = deskcustom_proto::decode(&buf[..len]) {
                            match &msg {
                                Message::Ping { sent_us } => {
                                    let pong = encode(&Message::Pong {
                                        sent_us: *sent_us,
                                        recv_us: now_us(),
                                    });
                                    let _ = udp.send_to(&pong, peer).await;
                                }
                                Message::Hello { hostname, .. } => {
                                    info!(%peer, hostname, "client connected");
                                    *active_client.lock().await = Some(peer);
                                    let mut st = status.lock().await;
                                    st.connected_peer = Some(format!("{hostname} ({peer})"));
                                    st.message = "Client connected".into();
                                }
                                _ => {
                                    debug.event("in", "udp", serde_json::json!({ "peer": peer.to_string() }));
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    let capture_task = {
        let config = config.clone();
        let metrics = metrics.clone();
        let debug = DebugLog::new(&config);
        let active_client = active_client.clone();
        let active_screen = active_screen.clone();
        let peer_tx = peer_tx.clone();
        let cancel = cancel.clone();
        async move {
            if let Err(err) = capture_loop(
                config,
                metrics,
                debug,
                active_client,
                active_screen,
                peer_tx,
                cancel,
            )
            .await
            {
                warn!(?err, "capture loop ended");
            }
        }
    };

    let tcp_task = {
        let cancel = cancel.clone();
        let peer_tx = peer_tx.clone();
        async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    accepted = tcp.accept() => {
                        let Ok((stream, peer)) = accepted else { continue };
                        info!(%peer, "client tcp connected");
                        let cancel = cancel.clone();
                        let peer_tx = peer_tx.clone();
                        tokio::spawn(async move {
                            if let Err(err) = handle_tcp_client(stream, peer, cancel, peer_tx).await {
                                warn!(?peer, ?err, "tcp client error");
                            }
                        });
                    }
                }
            }
        }
    };

    tokio::select! {
        _ = udp_task => {},
        _ = capture_task => {},
        _ = tcp_task => {},
        _ = cancel.cancelled() => {},
    }

    Ok(())
}

async fn capture_loop(
    config: Config,
    metrics: Arc<Mutex<Metrics>>,
    debug: DebugLog,
    active_client: Arc<Mutex<Option<SocketAddr>>>,
    active_screen: Arc<Mutex<Option<String>>>,
    peer_tx: Arc<Mutex<Option<PeerSender>>>,
    cancel: CancellationToken,
) -> Result<()> {
    let udp = UdpSocket::bind("0.0.0.0:0").await?;
    udp.connect(format!("127.0.0.1:{}", config.udp_port)).await.ok();

    #[cfg(windows)]
    WinInputCapture::configure_edge_switch(config.server.edge_threshold_px as i32);

    let mut keyboard =
        crate::keyboard::KeyboardPolicy::new(config.keyboard.clone(), config.clipboard.clone());

    #[cfg(windows)]
    let mut capture = WinInputCapture::new()?;
    #[cfg(not(windows))]
    let mut capture = MacInputCapture::new()?;

    let mut remote_active = false;
    let mut seq: u32 = 0;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        let on_remote = active_screen.lock().await.is_some();
        if on_remote != remote_active {
            remote_active = on_remote;
            #[cfg(windows)]
            WinInputCapture::set_remote_forwarding(remote_active);
        }

        #[cfg(windows)]
        if !on_remote && WinInputCapture::take_edge_hit() {
            if let Some(screen) = config
                .screens
                .iter()
                .find(|s| s.switch_edge.as_deref() == Some("right"))
            {
                *active_screen.lock().await = Some(screen.name.clone());
                remote_active = true;
                WinInputCapture::set_remote_forwarding(true);
                info!(screen = %screen.name, "switched to remote screen");
                let enter = encode(&Message::ScreenEnter { screen: screen.name.clone() });
                if let Some(peer) = *active_client.lock().await {
                    let _ = udp.send_to(&enter, peer).await;
                }
            }
        }

        let events = capture.poll()?;
        for event in events {
            let event = match keyboard.filter_capture(event) {
                Some(ev) => ev,
                None => continue,
            };

            let on_remote = active_screen.lock().await.is_some();

            match &event {
                deskcustom_platform::InputEvent::MouseMove(delta) => {
                    if !on_remote {
                        continue;
                    }

                    let dx = delta.dx.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    let dy = delta.dy.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    if dx == 0 && dy == 0 {
                        continue;
                    }

                    seq = seq.wrapping_add(1);
                    let packet = encode(&Message::MouseMove { dx, dy, seq });
                    if let Some(peer) = *active_client.lock().await {
                        let _ = udp.send_to(&packet, peer).await;
                        metrics.lock().await.mouse_sent += 1;
                        debug.event(
                            "out",
                            "mouse",
                            serde_json::json!({ "dx": dx, "dy": dy, "seq": seq }),
                        );
                    }
                }
                deskcustom_platform::InputEvent::MouseButton { button, pressed } => {
                    if !on_remote {
                        continue;
                    }
                    let packet = encode(&Message::MouseButton {
                        button: button.clone(),
                        pressed: *pressed,
                    });
                    if let Some(peer) = *active_client.lock().await {
                        let _ = udp.send_to(&packet, peer).await;
                    }
                }
                deskcustom_platform::InputEvent::Key {
                    scancode,
                    action,
                    modifiers,
                } => {
                    if !on_remote {
                        if let Some(trigger) = keyboard.clipboard_after_local_key(&event) {
                            if let Some(tx) = peer_tx.lock().await.clone() {
                                clipboard::spawn_clipboard_push(tx, trigger);
                            }
                        }
                        continue;
                    }
                    seq = seq.wrapping_add(1);
                    let packet = encode(&Message::Key {
                        scancode: *scancode,
                        action: action.clone(),
                        modifiers: *modifiers,
                        seq,
                    });
                    if let Some(peer) = *active_client.lock().await {
                        let _ = udp.send_to(&packet, peer).await;
                        metrics.lock().await.keys_sent += 1;
                    }
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }

    Ok(())
}

async fn handle_tcp_client(
    stream: TcpStream,
    peer: SocketAddr,
    cancel: CancellationToken,
    peer_slot: Arc<Mutex<Option<PeerSender>>>,
) -> Result<()> {
    tune_tcp(&stream).ok();

    let (peer_tx, peer_rx) = clipboard::peer_channel();
    *peer_slot.lock().await = Some(peer_tx.clone());

    let (mut reader, writer) = stream.into_split();
    clipboard::spawn_tcp_writer(writer, peer_rx);

    let _ = peer_tx
        .send(Message::Hello {
            hostname: local_hostname(),
            role: Role::Server,
        })
        .await;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            frame = tcp::read_frame(&mut reader) => {
                match frame {
                    Ok(payload) => {
                        let msg = deskcustom_proto::decode(&payload)?;
                        match msg {
                            Message::Hello { hostname, role } => {
                                info!(%peer, hostname, ?role, "hello over tcp");
                            }
                            Message::Key { seq, .. } => {
                                let _ = peer_tx.send(Message::Ack { seq }).await;
                            }
                            msg @ Message::ClipboardSet { .. } => {
                                clipboard::handle_incoming(msg).await;
                            }
                            _ => {}
                        }
                    }
                    Err(err) => {
                        warn!(%peer, ?err, "tcp client disconnected");
                        break;
                    }
                }
            }
        }
    }

    *peer_slot.lock().await = None;
    Ok(())
}

fn local_hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "deskcustom-server".into())
}

fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
