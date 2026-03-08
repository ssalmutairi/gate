use crate::models::Upstream;
use std::time::Duration;

/// Parse PEM-encoded certificates into DER bytes.
pub fn pem_to_der_certs(pem: &str) -> Vec<Vec<u8>> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    let mut certs = Vec::new();
    for item in rustls_pemfile::read_all(&mut reader).flatten() {
        if let rustls_pemfile::Item::X509Certificate(der) = item {
            certs.push(der.to_vec());
        }
    }
    certs
}

/// Parse PEM-encoded private key into DER bytes.
pub fn pem_to_der_key(pem: &str) -> Option<Vec<u8>> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    for item in rustls_pemfile::read_all(&mut reader).flatten() {
        match item {
            rustls_pemfile::Item::Pkcs1Key(der) => return Some(der.secret_pkcs1_der().to_vec()),
            rustls_pemfile::Item::Pkcs8Key(der) => return Some(der.secret_pkcs8_der().to_vec()),
            rustls_pemfile::Item::Sec1Key(der) => return Some(der.secret_sec1_der().to_vec()),
            _ => {}
        }
    }
    None
}

/// Build a per-upstream reqwest client with TLS settings, or return None to use the default.
pub fn build_upstream_client(upstream: &Upstream) -> Option<reqwest::Client> {
    let has_tls = upstream.tls_skip_verify
        || upstream.tls_ca_cert.is_some()
        || upstream.tls_client_cert.is_some();

    if !has_tls {
        return None;
    }

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy();

    if upstream.tls_skip_verify {
        builder = builder.danger_accept_invalid_certs(true);
    }

    if let Some(ref ca_pem) = upstream.tls_ca_cert {
        match reqwest::Certificate::from_pem(ca_pem.as_bytes()) {
            Ok(cert) => builder = builder.add_root_certificate(cert),
            Err(e) => tracing::warn!(upstream = %upstream.name, "Failed to parse CA cert PEM: {e}"),
        }
    }

    if let (Some(ref cert_pem), Some(ref key_pem)) = (&upstream.tls_client_cert, &upstream.tls_client_key) {
        let combined = format!("{}\n{}", cert_pem, key_pem);
        match reqwest::Identity::from_pem(combined.as_bytes()) {
            Ok(identity) => builder = builder.identity(identity),
            Err(e) => tracing::warn!(upstream = %upstream.name, "Failed to parse client cert/key PEM: {e}"),
        }
    }

    match builder.build() {
        Ok(client) => Some(client),
        Err(e) => {
            tracing::warn!(upstream = %upstream.name, "Failed to build TLS client: {e}");
            None
        }
    }
}
