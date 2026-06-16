use std::io::ErrorKind;

use anyhow::{Context, Result};
use tokio::net::{TcpListener, UdpSocket};

fn port_in_use_message(addr: &str) -> String {
    format!(
        "Порт {addr} уже занят. Закрой Deskflow/Synergy или другой экземпляр Deskcustom, затем нажми «Запустить» снова."
    )
}

pub async fn bind_tcp(addr: &str) -> Result<TcpListener> {
    match TcpListener::bind(addr).await {
        Ok(listener) => Ok(listener),
        Err(err) if err.kind() == ErrorKind::AddrInUse => anyhow::bail!("{}", port_in_use_message(addr)),
        Err(err) => Err(err).with_context(|| format!("bind TCP {addr}")),
    }
}

pub async fn bind_udp(addr: &str) -> Result<UdpSocket> {
    match UdpSocket::bind(addr).await {
        Ok(socket) => Ok(socket),
        Err(err) if err.kind() == ErrorKind::AddrInUse => anyhow::bail!("{}", port_in_use_message(addr)),
        Err(err) => Err(err).with_context(|| format!("bind UDP {addr}")),
    }
}
