//! Device discovery and connections.
//!
//! * iPhone / iPad (and Xcode-paired legacy Apple TVs) come from macOS's
//!   built-in `usbmuxd` at /var/run/usbmuxd — USB or Wi-Fi sync.
//! * Apple TV 4K on tvOS 17+ has no data port; it is found over Bonjour
//!   (`_remotepairing._tcp` / `_remotepairing-manual-pairing._tcp`), paired
//!   with the PIN shown on the TV, and reached through an encrypted
//!   CoreDevice tunnel (TLS-PSK + CDTunnel + userspace TCP), exactly the way
//!   Xcode does it.

use std::{
    collections::HashMap,
    net::IpAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, anyhow, bail};
use idevice::{
    IdeviceService,
    lockdown::LockdownClient,
    provider::{IdeviceProvider, UsbmuxdProvider},
    remote_pairing::{RemotePairingClient, RpPairingFile, RpPairingSocket, connect_tls_psk_tunnel_native},
    rsd::RsdHandshake,
    tcp::{adapter::Adapter, handle::AdapterHandle},
    usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection},
};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

use crate::store::{Platform, pairings_dir, sanitize};

pub const HOST_LABEL: &str = "Lodestar";
const SVC_TUNNEL: &str = "_remotepairing._tcp.local.";
const SVC_MANUAL: &str = "_remotepairing-manual-pairing._tcp.local.";

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceKind {
    Usbmux { device_id: u32, udid: String, wifi: bool },
    Network { host: String, ip: IpAddr, tunnel_port: Option<u16>, pairing_port: Option<u16>, paired: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub key: String,
    pub name: String,
    pub model: String,
    pub platform: Platform,
    pub kind: DeviceKind,
}

impl Device {
    pub fn needs_pairing(&self) -> bool {
        matches!(self.kind, DeviceKind::Network { paired: false, .. })
    }
}

// ───────────────────────── usbmuxd ─────────────────────────

#[derive(Clone)]
struct CachedInfo {
    name: String,
    model: String,
    class: String,
}

pub struct UsbScanner {
    cache: HashMap<String, CachedInfo>,
}

impl UsbScanner {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    pub async fn scan(&mut self) -> Vec<Device> {
        let Ok(mut mux) = UsbmuxdConnection::default().await else {
            return vec![];
        };
        let Ok(list) = mux.get_devices().await else {
            return vec![];
        };
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        // Prefer the USB entry when a device is reachable both ways.
        let mut list = list;
        list.sort_by_key(|d| !matches!(d.connection_type, Connection::Usb));
        for d in list {
            if !seen.insert(d.udid.clone()) {
                continue;
            }
            let wifi = matches!(d.connection_type, Connection::Network(_));
            let info = match self.cache.get(&d.udid) {
                Some(i) => i.clone(),
                None => {
                    let provider = d.to_provider(UsbmuxdAddr::default(), HOST_LABEL);
                    match lockdown_info(&provider).await {
                        Ok(i) => {
                            self.cache.insert(d.udid.clone(), i.clone());
                            i
                        }
                        Err(e) => {
                            debug!("lockdown query for {} failed: {e}", d.udid);
                            CachedInfo {
                                name: format!("Device {}", &d.udid[..8.min(d.udid.len())]),
                                model: "Unpaired — tap Trust on the device".into(),
                                class: String::new(),
                            }
                        }
                    }
                }
            };
            out.push(Device {
                key: format!("usb:{}", d.udid),
                name: info.name,
                platform: if info.class == "AppleTV" { Platform::Tvos } else { Platform::Ios },
                model: info.model,
                kind: DeviceKind::Usbmux { device_id: d.device_id, udid: d.udid, wifi },
            });
        }
        out
    }
}

async fn lockdown_info(provider: &impl IdeviceProvider) -> Result<CachedInfo> {
    let mut lockdown = LockdownClient::connect(provider).await?;
    let pairing = provider.get_pairing_file().await?;
    lockdown.start_session(&pairing).await?;
    let mut get = async |k: &str| -> String {
        lockdown.get_value(Some(k), None).await.ok().and_then(|v| v.as_string().map(str::to_string)).unwrap_or_default()
    };
    let name = get("DeviceName").await;
    let product = get("ProductType").await;
    let version = get("ProductVersion").await;
    let class = get("DeviceClass").await;
    Ok(CachedInfo {
        name,
        model: format!("{} · {} {}", model_name(&product), if class == "AppleTV" { "tvOS" } else { "iOS" }, version),
        class,
    })
}

/// Marketing names for Apple TVs; other hardware keeps its identifier.
fn model_name(product: &str) -> String {
    match product {
        "AppleTV14,1" => "Apple TV 4K (3rd gen)",
        "AppleTV11,1" => "Apple TV 4K (2nd gen)",
        "AppleTV6,2" => "Apple TV 4K",
        "AppleTV5,3" => "Apple TV HD",
        other => other,
    }
    .to_string()
}

pub fn usb_provider(device_id: u32, udid: &str) -> UsbmuxdProvider {
    UsbmuxdProvider {
        addr: UsbmuxdAddr::default(),
        tag: device_id,
        udid: udid.to_string(),
        device_id,
        label: HOST_LABEL.to_string(),
    }
}

// ───────────────────────── Bonjour (Apple TV) ─────────────────────────

#[derive(Debug, Clone, Default)]
struct NetEntry {
    ip: Option<IpAddr>,
    tunnel_port: Option<u16>,
    pairing_port: Option<u16>,
    /// Full Bonjour instance names (`<instance>._remotepairing._tcp.local.`) that
    /// produced the ports above, so a removal clears exactly the right one.
    tunnel_instance: Option<String>,
    pairing_instance: Option<String>,
}

/// Background Bonjour browser. Cheap to clone.
///
/// Created idle: the first multicast query is what makes macOS ask for Local
/// Network access, so browsing starts only when `begin` is called (at launch
/// for people who finished setup, or when they press Allow during setup).
#[derive(Clone, Default)]
pub struct NetScanner {
    entries: Arc<Mutex<HashMap<String, NetEntry>>>,
    started: Arc<std::sync::atomic::AtomicBool>,
}

impl NetScanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn started(&self) -> bool {
        self.started.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Starts browsing. Later calls do nothing.
    pub fn begin(&self, on_change: impl Fn() + Send + 'static) {
        if self.started.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let e2 = self.entries.clone();
        std::thread::Builder::new()
            .name("bonjour".into())
            .spawn(move || {
                let daemon = match mdns_sd::ServiceDaemon::new() {
                    Ok(d) => d,
                    Err(e) => {
                        warn!("Bonjour unavailable: {e}");
                        return;
                    }
                };
                let (Ok(rx_t), Ok(rx_m)) = (daemon.browse(SVC_TUNNEL), daemon.browse(SVC_MANUAL)) else {
                    warn!("Bonjour browse failed");
                    return;
                };
                let sel = [rx_t, rx_m];
                loop {
                    for rx in &sel {
                        while let Ok(ev) = rx.try_recv() {
                            if apply_event(&e2, ev) {
                                on_change();
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            })
            .expect("spawn bonjour thread");
    }

    pub fn devices(&self) -> Vec<Device> {
        let map = self.entries.lock().unwrap();
        map.iter()
            .filter_map(|(host, e)| {
                let ip = e.ip?;
                if e.tunnel_port.is_none() && e.pairing_port.is_none() {
                    return None;
                }
                let paired = pairing_path(host).exists();
                Some(Device {
                    key: format!("net:{host}"),
                    name: friendly_host(host),
                    model: "Apple TV · Wi-Fi".into(),
                    platform: Platform::Tvos,
                    kind: DeviceKind::Network {
                        host: host.clone(),
                        ip,
                        tunnel_port: e.tunnel_port,
                        pairing_port: e.pairing_port,
                        paired,
                    },
                })
            })
            .collect()
    }
}

fn apply_event(map: &Arc<Mutex<HashMap<String, NetEntry>>>, ev: mdns_sd::ServiceEvent) -> bool {
    use mdns_sd::ServiceEvent::*;
    match ev {
        ServiceResolved(svc) => {
            let host = svc.get_hostname().trim_end_matches('.').to_string();
            let ip = svc.get_addresses_v4().into_iter().next().map(IpAddr::V4);
            note_resolved(&mut map.lock().unwrap(), host, ip, &svc.ty_domain, svc.get_port(), svc.get_fullname());
            info!("Bonjour: {} {} port {}", svc.get_hostname(), svc.ty_domain, svc.get_port());
            true
        }
        ServiceRemoved(ty, fullname) => {
            note_removed(&mut map.lock().unwrap(), &ty, &fullname);
            true
        }
        _ => false,
    }
}

/// Records a resolved service for `host`, remembering which instance announced it.
fn note_resolved(
    map: &mut HashMap<String, NetEntry>,
    host: String,
    ip: Option<IpAddr>,
    ty: &str,
    port: u16,
    fullname: &str,
) {
    let e = map.entry(host).or_default();
    if ip.is_some() {
        e.ip = ip;
    }
    if ty == SVC_TUNNEL {
        e.tunnel_port = Some(port);
        e.tunnel_instance = Some(fullname.to_string());
    } else {
        e.pairing_port = Some(port);
        e.pairing_instance = Some(fullname.to_string());
    }
}

/// Handles a service going away: clears only the entry whose instance name matches
/// exactly (DNS names are case-insensitive). Returns whether anything changed.
fn note_removed(map: &mut HashMap<String, NetEntry>, ty: &str, fullname: &str) -> bool {
    let mut changed = false;
    for e in map.values_mut() {
        if ty == SVC_TUNNEL {
            if e.tunnel_instance.as_deref().is_some_and(|i| i.eq_ignore_ascii_case(fullname)) {
                e.tunnel_port = None;
                e.tunnel_instance = None;
                changed = true;
            }
        } else if e.pairing_instance.as_deref().is_some_and(|i| i.eq_ignore_ascii_case(fullname)) {
            e.pairing_port = None;
            e.pairing_instance = None;
            changed = true;
        }
    }
    changed
}

fn friendly_host(host: &str) -> String {
    host.trim_end_matches(".local").replace('-', " ").trim().to_string()
}

pub fn pairing_path(host: &str) -> PathBuf {
    pairings_dir().join(pairing_file_name(host))
}

/// One file per device, named from the (sanitised) Bonjour host name so a
/// hostile name can never point outside the pairings folder.
fn pairing_file_name(host: &str) -> String {
    format!("{}.plist", sanitize(host))
}

// ───────────────────────── Remote pairing + tunnel ─────────────────────────

/// First-time pairing. The TV must be on Settings → Remotes and Devices →
/// Remote App and Devices; it will display a PIN that `pin` must return.
pub async fn pair_network<F, Fut>(host: &str, ip: IpAddr, port: u16, pin: F) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = String>,
{
    let stream = TcpStream::connect((ip, port)).await.with_context(|| format!("Can't reach {ip}:{port}"))?;
    let mut rpf = RpPairingFile::generate(HOST_LABEL);
    let mut rpc = RemotePairingClient::new(RpPairingSocket::new(stream), HOST_LABEL);
    rpc.connect(&mut rpf, pin).await.map_err(|e| anyhow!("Pairing failed: {e}"))?;
    rpf.write_to_file(pairing_path(host)).await.map_err(|e| anyhow!("Couldn't save pairing: {e}"))?;
    info!("Paired with {host}");
    Ok(())
}

/// A live CoreDevice tunnel to a network device, with RSD services resolved.
pub struct Tunnel {
    pub handle: AdapterHandle,
    pub handshake: RsdHandshake,
    // The pairing control channel must outlive the tunnel.
    _control: RemotePairingClient<RpPairingSocket<TcpStream>>,
}

impl Tunnel {
    pub fn prop(&self, key: &str) -> Option<String> {
        self.handshake.properties.get(key).and_then(|v| v.as_string()).map(str::to_string)
    }

    pub fn platform(&self) -> Platform {
        let class = self.prop("DeviceClass").unwrap_or_default();
        let product = self.prop("ProductType").unwrap_or_default();
        if class == "AppleTV" || product.starts_with("AppleTV") { Platform::Tvos } else { Platform::Ios }
    }
}

pub async fn open_tunnel(host: &str, ip: IpAddr, port: u16) -> Result<Tunnel> {
    let path = pairing_path(host);
    if !path.exists() {
        bail!("This device isn't paired yet");
    }
    let mut rpf = RpPairingFile::read_from_file(&path).await.map_err(|e| anyhow!("Corrupt pairing file: {e}"))?;

    let stream =
        TcpStream::connect((ip, port)).await.with_context(|| format!("Can't reach {ip}:{port} — is the TV awake?"))?;
    let mut rpc = RemotePairingClient::new(RpPairingSocket::new(stream), HOST_LABEL);
    rpc.connect(&mut rpf, async || String::new())
        .await
        .map_err(|e| anyhow!("The TV rejected our pairing ({e}). Remove it and pair again."))?;

    let listener = rpc.create_tcp_listener().await.map_err(|e| anyhow!("Tunnel request refused: {e}"))?;
    debug!("tunnel listener on port {listener}");
    let tstream = TcpStream::connect((ip, listener)).await?;
    let tunnel = connect_tls_psk_tunnel_native(tstream, rpc.encryption_key())
        .await
        .map_err(|e| anyhow!("Tunnel handshake failed: {e}"))?;
    let info = tunnel.info.clone();
    info!("Tunnel up: {} → {} (RSD {})", info.client_address, info.server_address, info.server_rsd_port);

    let mut adapter =
        Adapter::new(Box::new(tunnel.into_inner()), info.client_address.parse()?, info.server_address.parse()?);
    adapter.set_mss((info.mtu as usize).saturating_sub(60));
    let mut handle = adapter.to_async_handle();
    let rsd = handle.connect(info.server_rsd_port).await?;
    let handshake = RsdHandshake::new(rsd).await.map_err(|e| anyhow!("RSD handshake failed: {e}"))?;
    debug!("RSD properties: {:?}", handshake.properties.keys().collect::<Vec<_>>());

    Ok(Tunnel { handle, handshake, _control: rpc })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_tv_models_get_marketing_names() {
        assert_eq!(model_name("AppleTV14,1"), "Apple TV 4K (3rd gen)");
        assert_eq!(model_name("AppleTV11,1"), "Apple TV 4K (2nd gen)");
        assert_eq!(model_name("iPhone16,1"), "iPhone16,1");
    }

    #[test]
    fn pairing_file_names_cannot_escape_the_pairings_folder() {
        assert_eq!(pairing_file_name("GWABtv.local"), "GWABtv_local.plist");
        let hostile = pairing_file_name("../../evil host");
        assert_eq!(hostile, "______evil_host.plist");
        assert!(!hostile.contains('/') && !hostile.contains(".."));
    }

    #[test]
    fn only_unpaired_network_devices_need_pairing() {
        let mk = |kind| Device { key: "k".into(), name: "n".into(), model: "m".into(), platform: Platform::Tvos, kind };
        let net = |paired| DeviceKind::Network {
            host: "h".into(),
            ip: "10.0.0.2".parse().unwrap(),
            tunnel_port: Some(1),
            pairing_port: Some(2),
            paired,
        };
        assert!(mk(net(false)).needs_pairing());
        assert!(!mk(net(true)).needs_pairing());
        assert!(!mk(DeviceKind::Usbmux { device_id: 1, udid: "u".into(), wifi: false }).needs_pairing());
    }

    fn tv(map: &mut HashMap<String, NetEntry>, host: &str, instance: &str, ty: &str, port: u16) {
        note_resolved(map, host.into(), Some("10.0.0.2".parse().unwrap()), ty, port, &format!("{instance}.{ty}"));
    }

    #[test]
    fn removing_one_tv_leaves_another_with_a_similar_name_alone() {
        let mut m = HashMap::new();
        tv(&mut m, "tv.local", "AA:11", SVC_TUNNEL, 1000);
        tv(&mut m, "tv-2.local", "AA:22", SVC_TUNNEL, 2000);
        assert!(note_removed(&mut m, SVC_TUNNEL, &format!("AA:11.{SVC_TUNNEL}")));
        assert_eq!(m["tv.local"].tunnel_port, None);
        assert_eq!(m["tv-2.local"].tunnel_port, Some(2000), "a different TV must be untouched");
    }

    #[test]
    fn removal_works_even_when_the_instance_name_is_not_the_host_name() {
        let mut m = HashMap::new();
        tv(&mut m, "livingroom.local", "3C:4D:5E:6F", SVC_TUNNEL, 49152);
        assert!(note_removed(&mut m, SVC_TUNNEL, &format!("3c:4d:5e:6f.{SVC_TUNNEL}")), "case-insensitive");
        assert_eq!(m["livingroom.local"].tunnel_port, None);
    }

    #[test]
    fn removal_clears_only_the_service_that_went_away() {
        let mut m = HashMap::new();
        tv(&mut m, "tv.local", "AA:11", SVC_TUNNEL, 1000);
        tv(&mut m, "tv.local", "AA:11", SVC_MANUAL, 2000);
        note_removed(&mut m, SVC_MANUAL, &format!("AA:11.{SVC_MANUAL}"));
        assert_eq!(m["tv.local"].pairing_port, None);
        assert_eq!(m["tv.local"].tunnel_port, Some(1000));
    }

    #[test]
    fn removing_something_never_seen_changes_nothing() {
        let mut m = HashMap::new();
        tv(&mut m, "tv.local", "AA:11", SVC_TUNNEL, 1000);
        assert!(!note_removed(&mut m, SVC_TUNNEL, &format!("ZZ:99.{SVC_TUNNEL}")));
        assert!(!note_removed(&mut m, SVC_TUNNEL, ""), "an empty name must not clear everything");
        assert_eq!(m["tv.local"].tunnel_port, Some(1000));
    }
}
