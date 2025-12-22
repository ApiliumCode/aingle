//! TLS utils for kitsune

use crate::config::*;
use crate::*;
use lair_keystore_api::actor::*;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use std::sync::Arc;

/// Tls Configuration.
#[derive(Clone)]
pub struct TlsConfig {
    /// Cert
    pub cert: Cert,

    /// Cert Priv Key
    pub cert_priv_key: CertPrivKey,

    /// Cert Digest
    pub cert_digest: CertDigest,
}

impl TlsConfig {
    /// Create a new ephemeral tls certificate that will not be persisted.
    pub async fn new_ephemeral() -> KitsuneResult<Self> {
        let mut options = lair_keystore_api::actor::TlsCertOptions::default();
        options.alg = lair_keystore_api::actor::TlsCertAlg::PkcsEcdsaP256Sha256;
        let cert = lair_keystore_api::internal::tls::tls_cert_self_signed_new_from_entropy(options)
            .await
            .map_err(KitsuneError::other)?;
        Ok(Self {
            cert: cert.cert_der,
            cert_priv_key: cert.priv_key_der,
            cert_digest: cert.cert_digest,
        })
    }
}

/// Helper to generate rustls configs given a TlsConfig reference.
#[allow(dead_code)]
pub fn gen_tls_configs(
    alpn: &[u8],
    tls: &TlsConfig,
    tuning_params: KitsuneP2pTuningParams,
) -> KitsuneResult<(Arc<rustls::ServerConfig>, Arc<rustls::ClientConfig>)> {
    let cert = CertificateDer::from(tls.cert.0.to_vec());
    let cert_priv_key = PrivatePkcs8KeyDer::from(tls.cert_priv_key.0.to_vec());

    // Build server config with custom client verifier that accepts any client
    let client_cert_verifier = Arc::new(TlsClientVerifier);

    let mut tls_server_config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_cert_verifier)
        .with_single_cert(vec![cert.clone()], cert_priv_key.clone_key().into())
        .map_err(KitsuneError::other)?;

    // Session caching
    tls_server_config.session_storage = rustls::server::ServerSessionMemoryCache::new(
        tuning_params.tls_in_mem_session_storage as usize,
    );
    tls_server_config.ticketer =
        rustls::crypto::aws_lc_rs::Ticketer::new().map_err(KitsuneError::other)?;
    tls_server_config.alpn_protocols = vec![alpn.to_vec()];
    let tls_server_config = Arc::new(tls_server_config);

    // Build client config with custom server verifier
    let server_cert_verifier = Arc::new(TlsServerVerifier);

    let mut tls_client_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(server_cert_verifier)
        .with_client_auth_cert(vec![cert], cert_priv_key.into())
        .map_err(KitsuneError::other)?;

    tls_client_config.resumption = rustls::client::Resumption::default()
        .tls12_resumption(rustls::client::Tls12Resumption::SessionIdOnly);
    tls_client_config.alpn_protocols = vec![alpn.to_vec()];
    let tls_client_config = Arc::new(tls_client_config);

    Ok((tls_server_config, tls_client_config))
}

/// Custom client certificate verifier that accepts any authenticated client.
#[derive(Debug)]
struct TlsClientVerifier;

impl rustls::server::danger::ClientCertVerifier for TlsClientVerifier {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        // TODO - check acceptable cert digest
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }

    fn client_auth_mandatory(&self) -> bool {
        false
    }
}

/// Custom server certificate verifier that accepts any server certificate.
/// This is used because kitsune uses self-signed certificates.
#[derive(Debug)]
struct TlsServerVerifier;

impl rustls::client::danger::ServerCertVerifier for TlsServerVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // TODO - check acceptable cert digest
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}
