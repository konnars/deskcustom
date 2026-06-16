use std::net::{IpAddr, UdpSocket as StdUdpSocket};

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
