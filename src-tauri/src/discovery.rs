use crate::config::{self, DeviceSettings};
use if_addrs::{get_if_addrs, IfAddr};
use serde::Serialize;
use std::{
    collections::HashSet,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    time::{Duration, Instant},
};
use tauri::AppHandle;

#[derive(Clone, Serialize)]
pub(crate) struct Candidate {
    pub kind: &'static str,
    pub ip: String,
    pub device_id: Option<u32>,
    pub name: String,
}

fn hello() -> [u8; 32] {
    let mut packet = [0xff; 32];
    packet[..4].copy_from_slice(&[0x21, 0x31, 0x00, 0x20]);
    packet
}

fn parse_miio(packet: &[u8]) -> Option<u32> {
    if packet.len() != 32 || packet[..4] != [0x21, 0x31, 0x00, 0x20] {
        return None;
    }
    let id = u32::from_be_bytes(packet[8..12].try_into().ok()?);
    (id != 0 && id != u32::MAX).then_some(id)
}

fn is_lg_ssdp(packet: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(packet) else {
        return false;
    };
    let lower = text.to_ascii_lowercase();
    lower.starts_with("http/1.1 200")
        && (lower.contains("lg electronics")
            || lower.contains("lg smart tv")
            || lower.contains("webos")
            || lower.contains("lge-com"))
}

fn targets() -> Vec<Ipv4Addr> {
    let mut out = HashSet::new();
    if let Ok(interfaces) = get_if_addrs() {
        for interface in interfaces {
            let IfAddr::V4(v4) = interface.addr else {
                continue;
            };
            if v4.ip.is_loopback() {
                continue;
            }
            let broadcast = v4
                .broadcast
                .unwrap_or_else(|| Ipv4Addr::from(u32::from(v4.ip) | !u32::from(v4.netmask)));
            out.insert(broadcast);
            // Some access points suppress broadcast replies. Sweep common /24 LANs as a fallback.
            if v4.netmask.octets() == [255, 255, 255, 0] {
                let base = u32::from(v4.ip) & u32::from(v4.netmask);
                for host in 1..=254 {
                    out.insert(Ipv4Addr::from(base + host));
                }
            }
        }
    }
    out.into_iter().collect()
}

fn scan() -> Result<Vec<Candidate>, String> {
    let miio = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    miio.set_broadcast(true).map_err(|e| e.to_string())?;
    miio.set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    let ssdp = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    ssdp.set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    let ips = targets();
    for ip in &ips {
        let _ = miio.send_to(&hello(), SocketAddrV4::new(*ip, 54321));
    }
    let search = b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: ssdp:all\r\n\r\n";
    let _ = ssdp.send_to(search, "239.255.255.250:1900");
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut buffer = [0u8; 4096];
    while Instant::now() < deadline {
        if let Ok((length, source)) = miio.recv_from(&mut buffer) {
            if let Some(id) = parse_miio(&buffer[..length]) {
                let ip = source.ip().to_string();
                if seen.insert(("xiaomi_miio", ip.clone())) {
                    found.push(Candidate {
                        kind: "xiaomi_miio",
                        ip,
                        device_id: Some(id),
                        name: format!("Xiaomi device {id}"),
                    });
                }
            }
        }
        if let Ok((length, source)) = ssdp.recv_from(&mut buffer) {
            if is_lg_ssdp(&buffer[..length]) {
                let ip = source.ip().to_string();
                if seen.insert(("tv", ip.clone())) {
                    found.push(Candidate {
                        kind: "tv",
                        ip,
                        device_id: None,
                        name: "LG webOS TV".into(),
                    });
                }
            }
        }
    }
    // webOS may have SSDP disabled. Its HTTPS remote endpoint answers this probe.
    let webos_probe = reqwest::blocking::Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(500))
        .build()
        .map_err(|e| e.to_string())?;
    for chunk in ips.chunks(64) {
        let responses = std::thread::scope(|scope| {
            chunk
                .iter()
                .map(|ip| {
                    let webos_probe = webos_probe.clone();
                    scope.spawn(move || {
                        let url = format!("https://{ip}:3001/");
                        webos_probe
                            .get(url)
                            .send()
                            .ok()
                            .filter(|reply| reply.status().is_success())
                            .and_then(|reply| reply.text().ok())
                            .filter(|body| body.trim() == "Hello world")
                            .map(|_| *ip)
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|handle| handle.join().ok().flatten())
                .collect::<Vec<_>>()
        });
        for ip in responses {
            let ip = ip.to_string();
            if seen.insert(("tv", ip.clone())) {
                found.push(Candidate {
                    kind: "tv",
                    ip,
                    device_id: None,
                    name: "Possible LG webOS TV".into(),
                });
            }
        }
    }
    found.sort_by(|a, b| a.ip.cmp(&b.ip).then(a.kind.cmp(b.kind)));
    Ok(found)
}

fn probe(ip: Ipv4Addr) -> Result<Candidate, String> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(800)))
        .map_err(|e| e.to_string())?;
    socket
        .send_to(&hello(), SocketAddrV4::new(ip, 54321))
        .map_err(|e| e.to_string())?;
    let mut packet = [0u8; 4096];
    if let Ok((length, source)) = socket.recv_from(&mut packet) {
        if source.ip() == ip {
            if let Some(id) = parse_miio(&packet[..length]) {
                return Ok(Candidate {
                    kind: "xiaomi_miio",
                    ip: ip.to_string(),
                    device_id: Some(id),
                    name: format!("Xiaomi device {id}"),
                });
            }
        }
    }
    let webos = reqwest::blocking::Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;
    if webos
        .get(format!("https://{ip}:3001/"))
        .send()
        .ok()
        .and_then(|response| response.text().ok())
        .is_some_and(|text| text.trim() == "Hello world")
    {
        return Ok(Candidate {
            kind: "tv",
            ip: ip.to_string(),
            device_id: None,
            name: "LG webOS TV".into(),
        });
    }
    Err("No supported device answered at this address".into())
}

#[tauri::command]
pub(crate) async fn probe_device(ip: String) -> Result<Candidate, String> {
    let ip = ip
        .parse::<Ipv4Addr>()
        .map_err(|_| "Enter a valid IPv4 address")?;
    tauri::async_runtime::spawn_blocking(move || probe(ip))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn discover_devices(app: AppHandle) -> Result<Vec<Candidate>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let existing = config::read(&app)?;
        let connected: HashSet<_> = existing
            .devices
            .iter()
            .map(|device| match &device.settings {
                DeviceSettings::Tv(settings) => ("tv", settings.tv_ip.clone()),
                DeviceSettings::BedsideLamp(settings) | DeviceSettings::ColorBulb(settings) => {
                    ("xiaomi_miio", settings.ip.clone())
                }
            })
            .collect();
        let lamp_ids: HashSet<_> = existing
            .devices
            .iter()
            .filter_map(|device| match &device.settings {
                DeviceSettings::BedsideLamp(settings) => Some(settings.device_id),
                _ => None,
            })
            .collect();
        Ok(scan()?
            .into_iter()
            .filter(|candidate| {
                !connected.contains(&(candidate.kind, candidate.ip.clone()))
                    && !candidate.device_id.is_some_and(|id| lamp_ids.contains(&id))
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_miio_hello() {
        let mut packet = hello();
        packet[8..12].copy_from_slice(&75416834u32.to_be_bytes());
        assert_eq!(parse_miio(&packet), Some(75416834));
        packet[0] = 0;
        assert_eq!(parse_miio(&packet), None);
    }

    #[test]
    #[ignore = "requires local devices"]
    fn live_scan() {
        for device in scan().unwrap() {
            println!("{} {} {:?}", device.kind, device.ip, device.device_id);
        }
    }
}
