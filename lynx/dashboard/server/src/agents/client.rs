use crate::config::Config;

/// Build a reqwest client for calling agent HTTP endpoints.
///
/// If X.509 mTLS certs are configured, the client presents the dashboard
/// client cert and trusts only the internal CA. Falls back to plain HTTP
/// client if certs are not yet available (dev mode / pre-bootstrap).
pub fn build_agent_client(config: &Config) -> reqwest::Client {
    let timeout = std::time::Duration::from_secs(15);

    // Attempt to build mTLS client.
    if let Some(client) = try_build_mtls_client(config, timeout) {
        return client;
    }

    // Fall back to plain client (WireGuard still provides transport security).
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .expect("build plain agent HTTP client")
}

fn try_build_mtls_client(config: &Config, timeout: std::time::Duration) -> Option<reqwest::Client> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

    let cert_der = config.x509_client_cert_der.as_slice();
    let key_der = config.x509_client_key_der.as_slice();
    let ca_cert_der = config.x509_ca_cert_der.as_slice();

    // Trust root store with the internal CA.
    let mut root_store = rustls::RootCertStore::empty();
    root_store
        .add(CertificateDer::from(ca_cert_der.to_vec()))
        .ok()?;

    // Dashboard client cert + key.
    let cert_chain = vec![CertificateDer::from(cert_der.to_vec())];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der.to_vec()));

    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(cert_chain, key)
        .ok()?;

    reqwest::Client::builder()
        .timeout(timeout)
        .use_preconfigured_tls(tls_config)
        .build()
        .ok()
}

/// Build a reqwest client for calling agent endpoints with a custom timeout.
pub fn build_agent_client_with_timeout(
    config: &Config,
    timeout: std::time::Duration,
) -> reqwest::Client {
    if let Some(client) = try_build_mtls_client(config, timeout) {
        return client;
    }
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .expect("build plain agent HTTP client")
}
