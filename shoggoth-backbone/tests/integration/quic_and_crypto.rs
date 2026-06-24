// SPDX-License-Identifier: Apache-2.0
/// Integration tests for QUIC transport, compression, and encryption.
///
/// Verifies certificate generation, WorkUnit/WorkResult serialization,
/// QAT compression round-trips, and AES-256-GCM encryption.

use shoggoth_core::qat_compress::{self, CompressionAlgo};
use shoggoth_sdk::quic_transport::{self, WorkUnit, WorkResult};

// ── Certificate Generation ─────────────────────────────────────────────────────

#[test]
fn test_certificate_generation() {
    let result = quic_transport::generate_self_signed_cert("test.shoggoth.local");
    assert!(result.is_ok());
    let (cert, _key) = result.unwrap();
    assert!(!cert.is_empty());
}

#[test]
fn test_certificate_different_names_produce_different_certs() {
    let (cert1, _) = quic_transport::generate_self_signed_cert("node-01.shoggoth.local").unwrap();
    let (cert2, _) = quic_transport::generate_self_signed_cert("node-02.shoggoth.local").unwrap();
    assert_ne!(cert1, cert2);
}

// ── WorkUnit Serialization ─────────────────────────────────────────────────────

#[test]
fn test_work_unit_compute_dispatch_round_trip() {
    let wu = WorkUnit::ComputeDispatch {
        work_id: 42,
        spirv_blob: vec![0x03, 0x02, 0x23, 0x07],
        push_constants: vec![0u8; 24],
        grid_x: 256,
        grid_y: 1,
        grid_z: 1,
    };

    let bytes = bincode::serialize(&wu).unwrap();
    let decoded: WorkUnit = bincode::deserialize(&bytes).unwrap();

    match decoded {
        WorkUnit::ComputeDispatch {
            work_id,
            grid_x,
            spirv_blob,
            ..
        } => {
            assert_eq!(work_id, 42);
            assert_eq!(grid_x, 256);
            assert_eq!(spirv_blob.len(), 4);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_work_unit_render_tile_round_trip() {
    let wu = WorkUnit::RenderTile {
        work_id: 7,
        tile_id: 3,
        viewport_width: 7680,
        viewport_height: 4320,
        tile_x: 2560,
        tile_y: 1440,
        tile_width: 2560,
        tile_height: 1440,
        scene_descriptor: vec![0xAB; 128],
        frame_id: 100,
        vertex_matrix_hash: 0xDEAD_BEEF_CAFE_BABE,
    };

    let bytes = bincode::serialize(&wu).unwrap();
    let decoded: WorkUnit = bincode::deserialize(&bytes).unwrap();

    match decoded {
        WorkUnit::RenderTile {
            tile_id,
            viewport_width,
            tile_width,
            ..
        } => {
            assert_eq!(tile_id, 3);
            assert_eq!(viewport_width, 7680);
            assert_eq!(tile_width, 2560);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_work_result_round_trip() {
    let wr = WorkResult {
        work_id: 99,
        success: true,
        output_data: vec![0x42; 1024],
        elapsed_us: 1200,
        error_message: None,
    };

    let bytes = bincode::serialize(&wr).unwrap();
    let decoded: WorkResult = bincode::deserialize(&bytes).unwrap();

    assert_eq!(decoded.work_id, 99);
    assert!(decoded.success);
    assert_eq!(decoded.output_data.len(), 1024);
    assert_eq!(decoded.elapsed_us, 1200);
    assert!(decoded.error_message.is_none());
}

// ── QAT Compression ────────────────────────────────────────────────────────────

#[test]
fn test_zstd_compress_decompress() {
    let data = b"Shoggoth inter-node traffic compression benchmark payload. ".repeat(200);
    let compressed = qat_compress::compress(&data, CompressionAlgo::Zstd).unwrap();
    // Compression should reduce size for repeated data.
    assert!(compressed.len() < data.len());
    let decompressed = qat_compress::decompress(&compressed, CompressionAlgo::Zstd).unwrap();
    assert_eq!(data.to_vec(), decompressed);
}

#[test]
fn test_lz4_compress_decompress() {
    let data = vec![0xABu8; 8192];
    let compressed = qat_compress::compress(&data, CompressionAlgo::Lz4).unwrap();
    let decompressed = qat_compress::decompress(&compressed, CompressionAlgo::Lz4).unwrap();
    assert_eq!(data, decompressed);
}

#[test]
fn test_deflate_compress_decompress() {
    let data = b"DEFLATE compression test for legacy compatibility.".repeat(50);
    let compressed = qat_compress::compress(&data, CompressionAlgo::Deflate).unwrap();
    let decompressed = qat_compress::decompress(&compressed, CompressionAlgo::Deflate).unwrap();
    assert_eq!(data.to_vec(), decompressed);
}

#[test]
fn test_compress_none_is_passthrough() {
    let data = b"passthrough".to_vec();
    let compressed = qat_compress::compress(&data, CompressionAlgo::None).unwrap();
    assert_eq!(data, compressed);
}

#[test]
fn test_empty_compress() {
    assert!(qat_compress::compress(&[], CompressionAlgo::Zstd).unwrap().is_empty());
}

// ── AES-256-GCM Encryption ─────────────────────────────────────────────────────

#[test]
fn test_aes_encrypt_decrypt_round_trip() {
    let key = [0x42u8; 32];
    let plaintext = b"SHOGGOTH TOP SECRET: node-to-node control message payload.";

    let (nonce, ciphertext, tag) =
        qat_compress::encrypt_aes256gcm(plaintext, &key).unwrap();
    assert_eq!(nonce.len(), 12);
    assert_eq!(tag.len(), 16);
    assert_ne!(ciphertext, plaintext);

    let mut decrypted = ciphertext.clone();
    let nonce_arr: [u8; 12] = nonce.try_into().unwrap();
    let tag_arr: [u8; 16] = tag.try_into().unwrap();
    qat_compress::decrypt_aes256gcm(&mut decrypted, &key, &nonce_arr, &tag_arr).unwrap();

    assert_eq!(plaintext.to_vec(), decrypted);
}

#[test]
fn test_aes_wrong_key_fails() {
    let key1 = [0x11u8; 32];
    let key2 = [0x22u8; 32];
    let plaintext = b"sensitive data";

    let (nonce, mut ciphertext, tag) =
        qat_compress::encrypt_aes256gcm(plaintext, &key1).unwrap();
    let nonce_arr: [u8; 12] = nonce.try_into().unwrap();
    let tag_arr: [u8; 16] = tag.try_into().unwrap();

    let result = qat_compress::decrypt_aes256gcm(&mut ciphertext, &key2, &nonce_arr, &tag_arr);
    assert!(result.is_err());
}

#[test]
fn test_qat_hardware_detection() {
    // Should not panic; may or may not be available.
    let _ = qat_compress::is_qat_available();
    let info = qat_compress::compression_hardware_info();
    assert!(!info.is_empty());
}
