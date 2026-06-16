use std::time::Duration;

use deskcustom_proto::{Message, encode};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::keyboard::ClipboardTrigger;
use crate::tcp;

pub type PeerSender = mpsc::Sender<Message>;

pub fn peer_channel() -> (PeerSender, mpsc::Receiver<Message>) {
    mpsc::channel(32)
}

pub async fn apply_clipboard(text: &str) {
    if let Err(err) = deskcustom_platform::write_text(text) {
        warn!(?err, "failed to set local clipboard");
    } else {
        debug!(chars = text.len(), "clipboard updated locally");
    }
}

pub fn spawn_clipboard_push(sender: PeerSender, trigger: ClipboardTrigger) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(120)).await;
        match deskcustom_platform::read_text() {
            Ok(Some(text)) => {
                debug!(?trigger, chars = text.len(), "pushing clipboard to peer");
                if sender
                    .send(Message::ClipboardSet { text })
                    .await
                    .is_err()
                {
                    warn!("failed to send clipboard — peer disconnected");
                }
            }
            Ok(None) => debug!(?trigger, "clipboard empty after copy"),
            Err(err) => warn!(?err, "failed to read clipboard"),
        }
    });
}

pub async fn handle_incoming(msg: Message) {
    if let Message::ClipboardSet { text } = msg {
        apply_clipboard(&text).await;
    }
}

pub fn spawn_tcp_writer(
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    mut rx: mpsc::Receiver<Message>,
) {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let payload = encode(&msg);
            if tcp::write_frame(&mut writer, &payload).await.is_err() {
                break;
            }
        }
    });
}
