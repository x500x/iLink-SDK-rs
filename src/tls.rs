//! TLS configuration for the SDK's HTTP client.
//!
//! Server certificates are validated against the operating system trust store
//! through `rustls-platform-verifier`, so the SDK honours the same roots as
//! browsers and respects corporate MITM proxies. The `ring`-based provider
//! bundled with `reqwest` supplies the cipher suites.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::ring::default_provider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as RustlsError, SignatureScheme};

/// A verifier that accepts every certificate. **Tests only.**
#[derive(Debug)]
pub struct DangerIgnoreVerifier;

impl ServerCertVerifier for DangerIgnoreVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// Return a [`ClientConfig`] that validates server certificates against the
/// operating system trust store.
pub fn default_tls_config() -> Result<ClientConfig, RustlsError> {
    install_default_provider();

    let verifier = rustls_platform_verifier::Verifier::new(Arc::new(default_provider()))
        .map_err(|_| RustlsError::General("failed to build platform verifier".into()))?;

    Ok(ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth())
}

/// Return a [`ClientConfig`] that skips certificate validation.
///
/// Only for local tests; never use against real endpoints.
#[cfg(test)]
pub(crate) fn test_tls_config() -> ClientConfig {
    install_default_provider();
    ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(DangerIgnoreVerifier))
        .with_no_client_auth()
}

fn install_default_provider() {
    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INSTALLED.get_or_init(|| {
        let _ = rustls::crypto::CryptoProvider::install_default(default_provider());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_config() {
        let cfg = default_tls_config().expect("tls config");
        assert!(cfg.alpn_protocols.is_empty());
    }

    #[test]
    fn builds_test_config() {
        let _ = test_tls_config();
    }
}
