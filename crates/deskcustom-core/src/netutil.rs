use std::net::{IpAddr, UdpSocket as StdUdpSocket};
use std::time::Duration;

use anyhow::Result;
use tokio::net::TcpStream;

pub fn local_ipv4_addresses() -> Vec<String> {
    let mut ips = Vec::new();

    if let Ok(socket) = StdUdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                if let IpAddr::V4(ip) = addr.ip() {
                    if !ip.is_loopback() {
                        ips.push(ip.to_string());
                    }
                }
            }
        }
    }

    if ips.is_empty() {
        ips.push("127.0.0.1".into());
    }

    ips
}

pub fn server_display_addr(port: u16) -> String {
    local_ipv4_addresses()
        .into_iter()
        .next()
        .map(|ip| format!("{ip}:{port}"))
        .unwrap_or_else(|| format!("127.0.0.1:{port}"))
}

pub fn tune_tcp(stream: &TcpStream) -> Result<()> {
    stream.set_nodelay(true)?;
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(30))
        .with_interval(Duration::from_secs(10));
    socket2::SockRef::from(stream).set_tcp_keepalive(&keepalive)?;
    Ok(())
}
