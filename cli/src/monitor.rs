use crate::clock::unix_now;
use crate::config::{Monitor, MonitorsConfig};
use crate::history::MonitorId;
use crate::net;
use crate::status::Status;
use rustls::pki_types::{ServerName, UnixTime};
use rustls::time_provider::TimeProvider;
use rustls::{CertificateError, ClientConfig, ClientConnection, Error};
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ureq::Agent;

/// An http or tcp monitor the prober probes.
pub struct Probed {
    pub id: MonitorId,
    pub monitor: Monitor,
}

/// One probe's result; `ms` when a response arrived.
pub struct Seen {
    pub status: Status,
    pub message: String,
    pub ms: Option<u16>,
}

/// The http and tcp monitors of `config`, in config order, when `host` is its prober.
pub fn probed(config: MonitorsConfig, host: &str) -> Vec<Probed> {
    let ours = config.prober.as_ref().is_some_and(|p| p.as_str() == host);
    let monitors = config.monitors.into_iter();
    let probed = monitors.filter(|m| ours && !matches!(m, Monitor::Heartbeat(_)));
    let id = |m: &Monitor| MonitorId::of(m.names().0, m.names().1);
    probed
        .map(|monitor| Probed {
            id: id(&monitor),
            monitor,
        })
        .collect()
}

fn timeout(m: &Monitor) -> Duration {
    let s = match m {
        Monitor::Http(h) => h.timeout_s,
        Monitor::Tcp(t) => t.timeout_s,
        Monitor::Heartbeat(_) => 0,
    };
    Duration::from_secs(s.into())
}

fn seen(message: String, ms: Option<u16>) -> Seen {
    let status = if message.is_empty() {
        Status::Up
    } else {
        Status::Down
    };
    Seen {
        status,
        message,
        ms,
    }
}

/// One probe on a new connection; `cert_expires` is the certificate's notAfter inside its warning window.
pub fn probe(agent: &Agent, m: &Monitor, cert_expires: Option<u64>) -> Seen {
    let (start, timeout) = (Instant::now(), timeout(m));
    let took = || u16::try_from(start.elapsed().as_millis()).ok();
    let failed = |e: String, timed_out: bool| {
        let after = format!("timeout after {}s", timeout.as_secs());
        seen(if timed_out { after } else { e }, None)
    };
    match m {
        Monitor::Http(h) => match agent
            .get(&h.url.0)
            .config()
            .timeout_global(Some(timeout))
            .build()
            .call()
        {
            Err(e) => failed(e.to_string(), matches!(e, ureq::Error::Timeout(_))),
            Ok(mut r) => {
                let (code, ms) = (r.status().as_u16(), took());
                let body = r.body_mut().with_config().limit(256 << 10);
                let message = if !h
                    .expect_status
                    .map_or((200..300).contains(&code), |s| s == code)
                {
                    format!("HTTP {code}")
                } else if let Some(k) = &h.keyword
                    && !String::from_utf8_lossy(&body.read_to_vec().unwrap_or_default())
                        .contains(k.as_str())
                {
                    format!("keyword missing: {k}")
                } else {
                    let days = |at: u64| at.saturating_sub(unix_now()) / 86_400;
                    cert_expires
                        .map_or(String::new(), |at| format!("cert expires in {}d", days(at)))
                };
                seen(message, ms)
            }
        },
        _ => match (|| {
            let Monitor::Tcp(t) = m else {
                return Err(ErrorKind::Unsupported.into());
            };
            let addr = (t.host.as_str(), t.port).to_socket_addrs()?.next();
            TcpStream::connect_timeout(&addr.ok_or(ErrorKind::NotFound)?, timeout)
        })() {
            Ok(_) => seen(String::new(), took()),
            Err(e) => failed(e.to_string(), e.kind() == ErrorKind::TimedOut),
        },
    }
}

#[derive(Debug)]
struct Ahead(u64);

impl TimeProvider for Ahead {
    fn current_time(&self) -> Option<UnixTime> {
        Some(UnixTime::since_unix_epoch(Duration::from_secs(self.0)))
    }
}

/// One TLS handshake with the clock `cert_warn_days` ahead: the notAfter it fails on; `None` when it could not tell.
pub fn cert_expiry(m: &Monitor) -> Option<Option<u64>> {
    let Monitor::Http(h) = m else {
        return Some(None);
    };
    if h.cert_warn_days == 0 {
        return Some(None);
    }
    let uri: ureq::http::Uri = h.url.0.parse().ok()?;
    let host = uri.host()?.trim_start_matches('[').trim_end_matches(']');
    let ahead = Arc::new(Ahead(unix_now() + u64::from(h.cert_warn_days) * 86_400));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_details(provider, ahead)
        .with_safe_default_protocol_versions()
        .ok()?
        .with_root_certificates(net::roots())
        .with_no_client_auth();
    let name = ServerName::try_from(host.to_string()).ok()?;
    let mut conn = ClientConnection::new(Arc::new(config), name).ok()?;
    let addr = (host, uri.port_u16().unwrap_or(443))
        .to_socket_addrs()
        .ok()?
        .next()?;
    let mut tcp = TcpStream::connect_timeout(&addr, timeout(m)).ok()?;
    tcp.set_read_timeout(Some(timeout(m))).ok()?;
    while conn.is_handshaking() {
        if let Err(e) = conn.complete_io(&mut tcp) {
            return match e.get_ref().and_then(|e| e.downcast_ref::<Error>()) {
                Some(Error::InvalidCertificate(CertificateError::ExpiredContext {
                    not_after,
                    ..
                })) => Some(Some(not_after.as_secs())),
                _ => None,
            };
        }
    }
    Some(None)
}
