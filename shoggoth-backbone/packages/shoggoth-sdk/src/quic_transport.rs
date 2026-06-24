// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/quic_transport.rs — QUIC transport layer for Shoggoth.
//
// Shared protocol definitions used by both the orchestrator (client side)
// and node agents (server side). Provides:
//   • Certificate generation (self-signed, rcgen)
//   • Protocol message types (WorkUnit, WorkResult)
//   • Connection management
//   • Bidirectional stream helpers

use std::sync::Arc;
use std::time::Duration;

use quinn::{Connection, Endpoint, RecvStream, SendStream, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};

// ── Certificate Generation ─────────────────────────────────────────────────────

/// Generates a self-signed TLS certificate for QUIC mTLS.
///
/// Both the orchestrator and node agents call this at startup.
/// In production, certificates are issued by a central Shoggoth CA
/// and pinned by fingerprint.
pub fn generate_self_signed_cert(
    common_name: &str,
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), rcgen::Error> {
    let mut params = rcgen::CertificateParams::new(vec![common_name.into()]);
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyEncipherment,
    ];
    let key_pair = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    Ok((
        CertificateDer::from(cert.der().to_vec()),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der())),
    ))
}

/// Builds a Quinn `ServerConfig` for a node agent.
pub fn build_server_config(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
) -> Result<quinn::ServerConfig, Box<dyn std::error::Error + Send + Sync>> {
    let mut server_config =
        quinn::ServerConfig::with_single_cert(vec![cert], key)?;

    // Enable keepalive to detect dead connections quickly.
    let transport_config = Arc::get_mut(&mut server_config.transport)
        .expect("Fresh transport config");
    transport_config.max_idle_timeout(Some(
        Duration::from_secs(30).try_into()?,
    ));
    transport_config.keep_alive_interval(Some(Duration::from_secs(5)));

    Ok(server_config)
}

/// Builds a Quinn client `Endpoint` for the orchestrator.
pub fn build_client_endpoint(
    bind_addr: &str,
) -> Result<Endpoint, Box<dyn std::error::Error + Send + Sync>> {
    let mut roots = rustls::RootCertStore::empty();

    // In production: load the Shoggoth CA certificate into roots.
    // For now, we accept self-signed certs by adding a custom verifier.
    let mut client_crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    // Disable certificate verification for self-signed dev certs.
    // In production: use proper CA-issued certs with fingerprint pinning.
    client_crypto
        .dangerous()
        .set_certificate_verifier(Arc::new(SkipServerVerification::new()));

    let mut endpoint = Endpoint::client(bind_addr.parse()?)?;
    endpoint.set_default_client_config(client_crypto);

    Ok(endpoint)
}

// ── Protocol Messages ──────────────────────────────────────────────────────────

/// A work unit dispatched from the orchestrator to a node agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkUnit {
    /// Execute a compute shader (SPIR-V binary) with push constants.
    ComputeDispatch {
        work_id: u64,
        spirv_blob: Vec<u8>,
        push_constants: Vec<u8>,
        grid_x: u32,
        grid_y: u32,
        grid_z: u32,
    },
    /// Render a tile of the viewport.
    RenderTile {
        work_id: u64,
        tile_id: u32,
        viewport_width: u32,
        viewport_height: u32,
        tile_x: u32,
        tile_y: u32,
        tile_width: u32,
        tile_height: u32,
        scene_descriptor: Vec<u8>,
        frame_id: u64,
        vertex_matrix_hash: u64,
    },
    /// Preload model weights into VRAM cache.
    PreloadWeights {
        model_name: String,
        layer_start: u32,
        layer_end: u32,
        weights_blob: Vec<u8>,
    },
    /// Graceful shutdown.
    Shutdown,
}

/// Result of executing a work unit on a node agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkResult {
    pub work_id: u64,
    pub success: bool,
    pub output_data: Vec<u8>,
    pub elapsed_us: u64,
    pub error_message: Option<String>,
}

// ── Stream Helpers ─────────────────────────────────────────────────────────────

/// Sends a bincode-encoded message over a QUIC send stream.
pub async fn send_message<T: Serialize>(
    send: &mut SendStream,
    msg: &T,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bytes = bincode::serialize(msg)?;
    let len = (bytes.len() as u32).to_le_bytes();
    send.write_all(&len).await?;
    send.write_all(&bytes).await?;
    send.finish()?;
    Ok(())
}

/// Receives a bincode-encoded message from a QUIC recv stream.
pub async fn recv_message<T: for<'de> Deserialize<'de>>(
    recv: &mut RecvStream,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;

    let msg = bincode::deserialize(&buf)?;
    Ok(msg)
}

// ── TLS Helper ─────────────────────────────────────────────────────────────────

/// A TLS certificate verifier that accepts all certificates.
///
/// **WARNING**: Only for development. In production, use proper CA-issued
/// certificates with fingerprint pinning.
#[derive(Debug)]
struct SkipServerVerification {
    inner: Arc<rustls::crypto::CryptoProvider>,
}

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(rustls::crypto::aws_lc_rs::default_provider()),
        })
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.inner.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.inner.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.signature_verification_algorithms.supported_schemes()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cert_generation() {
        let result = generate_self_signed_cert("test-node.shoggoth.local");
        assert!(result.is_ok());
        let (cert, _key) = result.unwrap();
        assert!(!cert.is_empty());
    }

    #[test]
    fn test_work_unit_serialization() {
        let wu = WorkUnit::ComputeDispatch {
            work_id: 42,
            spirv_blob: vec![0x03, 0x02, 0x23, 0x07],
            push_constants: vec![0; 64],
            grid_x: 256,
            grid_y: 1,
            grid_z: 1,
        };
        let bytes = bincode::serialize(&wu).unwrap();
        let decoded: WorkUnit = bincode::deserialize(&bytes).unwrap();

        match decoded {
            WorkUnit::ComputeDispatch { work_id, grid_x, .. } => {
                assert_eq!(work_id, 42);
                assert_eq!(grid_x, 256);
            }
            _ => panic!("Wrong variant"),
        }
    }
}
