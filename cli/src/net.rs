use rustls::RootCertStore;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use std::sync::Arc;
use ureq::Agent;
use ureq::tls::{Certificate, RootCerts, TlsConfig};

const BUFFER: usize = 16 << 10;

/// The HTTPS agent of the notifier and the prober: 16 KB buffers, no kept connections.
pub fn agent() -> Agent {
    let certs: Vec<_> = extra_roots()
        .iter()
        .map(|c| Certificate::from_der(c).to_owned())
        .collect();
    let roots = (!certs.is_empty()).then(|| RootCerts::new_with_certs(&certs));
    let tls = TlsConfig::builder().root_certs(roots.unwrap_or(RootCerts::WebPki));
    let config = Agent::config_builder()
        .http_status_as_error(false)
        .input_buffer_size(BUFFER)
        .output_buffer_size(BUFFER)
        .max_idle_connections(0);
    config.tls_config(tls.build()).build().into()
}

/// `$SSL_CERT_FILE`'s certificates, else webpki's roots.
pub fn roots() -> Arc<RootCertStore> {
    let mut store = RootCertStore::empty();
    let extra = extra_roots();
    if extra.is_empty() {
        store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }
    store.add_parsable_certificates(extra);
    Arc::new(store)
}

fn extra_roots() -> Vec<CertificateDer<'static>> {
    let pem = std::env::var_os("SSL_CERT_FILE").and_then(|p| std::fs::read(p).ok());
    CertificateDer::pem_slice_iter(pem.as_deref().unwrap_or_default())
        .flatten()
        .collect()
}
