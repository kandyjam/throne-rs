//! System proxy toggle (macOS via `/usr/sbin/networksetup`).
//!
//! Design notes (Firefox / CFNetwork):
//! - Only set **HTTP + HTTPS** system proxies for the mixed inbound port.
//!   Enabling SOCKS *and* HTTP to the same port makes Firefox prefer SOCKS and
//!   often fail DNS (`socks_remote_dns`).
//! - Always **force SOCKS off** when enabling or disabling.
//! - Bypass list must be **host/domain tokens only** — CIDR like `10.0.0.0/8`
//!   is invalid for `-setproxybypassdomains` and can break system proxy for
//!   browsers (pages never load).
//! - Only touch primary hardware services (Wi-Fi / Ethernet).
//! - Disable PAC / auto-proxy when turning manual proxy on.
//! - Client address is always loopback (`127.0.0.1`), never `0.0.0.0` / `::`.

use std::process::Command;

use tracing::{info, warn};

/// Enable or disable system HTTP(S) proxy pointing at the mixed inbound.
///
/// `listen_host` is the core inbound bind address (may be `0.0.0.0` / `::`).
/// The address written into macOS System Preferences is always a **loopback
/// client address** (`127.0.0.1`).
pub fn set_system_proxy(enable: bool, listen_host: &str, port: i32) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let client_host = proxy_client_host(listen_host);
        set_macos(enable, &client_host, port)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (enable, listen_host, port);
        Err("system proxy toggle not implemented on this platform yet".into())
    }
}

/// Map inbound listen address → address apps should dial.
pub fn proxy_client_host(listen_host: &str) -> String {
    let h = listen_host.trim();
    match h {
        "" | "0.0.0.0" | "*" | "localhost" | "::" | "[::]" | "::0" | "::1" | "[::1]" => {
            "127.0.0.1".into()
        }
        "127.0.0.1" => "127.0.0.1".into(),
        // Any other bind address (LAN IP etc.) — still advertise loopback to local apps.
        _ => "127.0.0.1".into(),
    }
}

#[cfg(target_os = "macos")]
fn set_macos(enable: bool, host: &str, port: i32) -> Result<(), String> {
    if enable {
        if host.is_empty() {
            return Err("proxy host is empty".into());
        }
        if !(1..=65535).contains(&port) {
            return Err(format!("invalid proxy port {port}"));
        }
    }

    let services = list_network_services()?;
    if services.is_empty() {
        return Err("no suitable network services found".into());
    }

    let mut errors = Vec::new();
    for svc in &services {
        let r = if enable {
            enable_service(svc, host, port)
        } else {
            disable_service(svc)
        };
        if let Err(e) = r {
            warn!(service = %svc, %e, "system proxy update failed for service");
            errors.push(format!("{svc}: {e}"));
        }
    }

    if errors.len() == services.len() {
        return Err(format!(
            "system proxy failed on all services: {}",
            errors.join("; ")
        ));
    }

    info!(
        enable,
        host,
        port,
        services = ?services,
        partial_errors = errors.len(),
        "system proxy updated"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn enable_service(svc: &str, host: &str, port: i32) -> Result<(), String> {
    let port_s = port.to_string();
    let ns = "/usr/sbin/networksetup";

    // PAC/auto off so they don't override manual proxy.
    let _ = run_ns(ns, &["-setautoproxystate", svc, "off"]);
    let _ = run_ns(ns, &["-setproxyautodiscovery", svc, "Off"]);
    // SOCKS must stay off — mixed port is HTTP; Firefox SOCKS+remote DNS breaks hard.
    let _ = run_ns(ns, &["-setsocksfirewallproxy", svc, "127.0.0.1", "0"]);
    let _ = run_ns(ns, &["-setsocksfirewallproxystate", svc, "off"]);

    // HTTP + HTTPS only (mixed inbound speaks both).
    run_ns(ns, &["-setwebproxy", svc, host, &port_s])?;
    run_ns(ns, &["-setsecurewebproxy", svc, host, &port_s])?;
    run_ns(ns, &["-setwebproxystate", svc, "on"])?;
    run_ns(ns, &["-setsecurewebproxystate", svc, "on"])?;

    // Host/domain tokens ONLY — no CIDR (breaks Firefox / Safari).
    let _ = run_ns(
        ns,
        &[
            "-setproxybypassdomains",
            svc,
            "127.0.0.1",
            "localhost",
            "*.local",
            "*.localhost",
            "<local>",
        ],
    );

    Ok(())
}

#[cfg(target_os = "macos")]
fn disable_service(svc: &str) -> Result<(), String> {
    let ns = "/usr/sbin/networksetup";
    // ONLY flip state off. Never rewrite host/port to :0 — if something later
    // turns Enabled back on, Port 0 makes every browser fail instantly.
    // Keep this minimal: each call has a 1s ceiling; extra flags made Stop feel stuck.
    run_ns(ns, &["-setwebproxystate", svc, "off"])?;
    run_ns(ns, &["-setsecurewebproxystate", svc, "off"])?;
    let _ = run_ns(ns, &["-setsocksfirewallproxystate", svc, "off"]);
    Ok(())
}

/// Best-effort clear of system proxy on primary hardware services (recovery path).
///
/// Only touches Wi-Fi / Ethernet-class services — sweeping *every* service
/// (bridges, Tailscale, …) made Stop block for 20–30s when networksetup stalled.
pub fn force_clear_system_proxy() {
    #[cfg(target_os = "macos")]
    {
        if let Ok(services) = list_network_services() {
            for svc in services {
                let _ = disable_service(&svc);
            }
        }
    }
}

/// Point primary NICs at Tun DNS (`172.19.0.2`) or clear them (`Empty`).
///
/// Complements Go `sys.SetSystemDNS` so Tun works even when ThroneCore is an
/// older build that failed to bind DNS on the physical interface after
/// `auto_route` flipped the default route to utun.
pub fn set_tun_system_dns(enable: bool, tun_ipv4_cidr: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if enable {
            let dns = tun_dns_address(tun_ipv4_cidr)
                .ok_or_else(|| format!("invalid tun_ipv4_cidr {tun_ipv4_cidr:?}"))?;
            set_macos_dns(Some(&dns))
        } else {
            set_macos_dns(None)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (enable, tun_ipv4_cidr);
        Ok(())
    }
}

/// Derive system DNS IP from Tun CIDR (upstream: tunIP + 1 → `172.19.0.2`).
pub fn tun_dns_address(tun_ipv4_cidr: &str) -> Option<String> {
    let (addr, _pfx) = tun_ipv4_cidr.trim().split_once('/')?;
    let parts: Vec<u8> = addr.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 4 {
        return None;
    }
    // Next IPv4 address (wrapping within last octet is fine for /24 defaults).
    let mut last = parts[3] as u16 + 1;
    if last > 255 {
        last = 1;
    }
    Some(format!(
        "{}.{}.{}.{}",
        parts[0], parts[1], parts[2], last as u8
    ))
}

#[cfg(target_os = "macos")]
fn set_macos_dns(dns: Option<&str>) -> Result<(), String> {
    let services = list_network_services()?;
    if services.is_empty() {
        return Err("no suitable network services found".into());
    }
    let ns = "/usr/sbin/networksetup";
    let mut errors = Vec::new();
    for svc in &services {
        let r = match dns {
            Some(ip) => run_ns(ns, &["-setdnsservers", svc, ip]),
            // "Empty" is the networksetup token that clears manual DNS.
            None => run_ns(ns, &["-setdnsservers", svc, "Empty"]),
        };
        if let Err(e) = r {
            warn!(service = %svc, %e, "system DNS update failed for service");
            errors.push(format!("{svc}: {e}"));
        }
    }
    if errors.len() == services.len() {
        return Err(format!(
            "system DNS failed on all services: {}",
            errors.join("; ")
        ));
    }
    info!(dns = ?dns, services = ?services, "system DNS updated for Tun");
    Ok(())
}

#[cfg(target_os = "macos")]
fn list_network_services() -> Result<Vec<String>, String> {
    let out = Command::new("/usr/sbin/networksetup")
        .arg("-listallnetworkservices")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut all = Vec::new();
    for line in text.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('*') {
            continue;
        }
        all.push(line.to_string());
    }

    let primary: Vec<String> = all
        .iter()
        .filter(|s| is_primary_service(s))
        .cloned()
        .collect();
    if !primary.is_empty() {
        return Ok(primary);
    }
    Ok(all)
}

#[cfg(target_os = "macos")]
fn is_primary_service(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.contains("wi-fi") || n.contains("wifi") || n == "ethernet" || n.starts_with("ethernet ") {
        return true;
    }
    if n.contains("usb 10/100") {
        return true;
    }
    if n.contains("bridge")
        || n.contains("thunderbolt")
        || n.contains("iphone")
        || n.contains("ipad")
        || n.contains("bluetooth")
        || n.contains("utun")
        || n.contains("vpn")
        || n.contains("tailscale")
        || n.contains("wireguard")
        || n.contains("parallels")
        || n.contains("vmware")
        || n.contains("virtual")
    {
        return false;
    }
    false
}

#[cfg(target_os = "macos")]
fn run_ns(bin: &str, args: &[&str]) -> Result<(), String> {
    // 1s is plenty for a healthy networksetup; 2s made a wedged service stall Start/Stop.
    run_ns_timeout(bin, args, std::time::Duration::from_secs(1))
}

#[cfg(target_os = "macos")]
fn run_ns_timeout(bin: &str, args: &[&str], timeout: std::time::Duration) -> Result<(), String> {
    use std::sync::mpsc;
    use std::thread;

    let bin = bin.to_string();
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let args_for_err = args.clone();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let out = Command::new(&bin).args(&args).output();
        let _ = tx.send(out);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(out)) => {
            if out.status.success() {
                return Ok(());
            }
            let err = String::from_utf8_lossy(&out.stderr);
            let err_trim = err.trim();
            if err_trim.is_empty()
                || err_trim.contains("not a recognized network service")
                || err_trim.contains("is not a recognized network service")
                || err_trim.contains("Unrecognized network service")
            {
                warn!(args = ?args_for_err, %err_trim, "networksetup skipped");
                return Ok(());
            }
            Err(format!("networksetup {args_for_err:?}: {err_trim}"))
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => {
            warn!(args = ?args_for_err, "networksetup timed out after {timeout:?}");
            Err(format!("networksetup timed out: {args_for_err:?}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{proxy_client_host, tun_dns_address};

    #[test]
    fn client_host_never_any_address() {
        assert_eq!(proxy_client_host(""), "127.0.0.1");
        assert_eq!(proxy_client_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(proxy_client_host("::"), "127.0.0.1");
        assert_eq!(proxy_client_host("[::]"), "127.0.0.1");
        assert_eq!(proxy_client_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(proxy_client_host("192.168.1.2"), "127.0.0.1");
    }

    #[test]
    fn tun_dns_is_next_address() {
        assert_eq!(
            tun_dns_address("172.19.0.1/24").as_deref(),
            Some("172.19.0.2")
        );
        assert_eq!(tun_dns_address("not-a-cidr"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn primary_service_filter() {
        assert!(super::is_primary_service("Wi-Fi"));
        assert!(super::is_primary_service("Ethernet"));
        assert!(!super::is_primary_service("Thunderbolt Bridge"));
        assert!(!super::is_primary_service("iPhone USB"));
        assert!(!super::is_primary_service("Tailscale"));
    }
}
