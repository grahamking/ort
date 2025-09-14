//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! --- Minimal TLS 1.3 client (AES-128-GCM + ECDHE) ---

use std::{io::Write as _, net::TcpStream};

const TLS_HANDSHAKE_TYPE: u8 = 0x16;
const TLS_APPLICATION_DATA_TYPE: u8 = 0x17;
const TLS_VERSION_1_3: [u8; 2] = [0x03, 0x04];
const CYPHER_SUITES: [u8; 2] = [0x13, 0x01]; // AES_128_GCM_SHA256
const GROUPS: [u8; 2] = [0x00, 0x17]; // secp256r1
const SIG_ALGS: [u8; 8] = [0x00, 0x04, 0x04, 0x01, 0x04, 0x03, 0x02, 0x01]; // rsa_pkcs1_sha256, ecdsa_sha256, rsa_sha256, rsa_pkcs1_sha1 (minimal for PoC)

use ring::{
    agreement::{ECDH_P256, EphemeralPrivateKey},
    rand::{SecureRandom, SystemRandom},
};

pub struct TlsStream {
    tcp: TcpStream,
    rng: SystemRandom,
}

impl TlsStream {
    pub fn handshake(tcp: TcpStream) -> anyhow::Result<Self> {
        let mut tls = TlsStream {
            tcp,
            rng: SystemRandom::new(),
        };

        tls.client_hello()?;

        Ok(tls)
    }

    /// Send the opening ClientHello message
    fn client_hello(&mut self) -> anyhow::Result<()> {
        // Generate client random
        let mut client_random = [0u8; 32];
        self.rng
            .fill(&mut client_random)
            .map_err(|_| anyhow::anyhow!("rand error"))?;

        // ECDHE key share
        let private_key = EphemeralPrivateKey::generate(&ECDH_P256, &self.rng)
            .map_err(|_| anyhow::anyhow!("key gen error"))?;
        let public_key = private_key
            .compute_public_key()
            .map_err(|_| anyhow::anyhow!("pub key error"))?;
        let pub_key_bytes = public_key.as_ref();

        let mut ch = Vec::with_capacity(128);
        ch.extend_from_slice(&[
            TLS_HANDSHAKE_TYPE,
            TLS_VERSION_1_3[0],
            TLS_VERSION_1_3[1],
            0x00,
            0x00,
            0x00,
            0x00,
            0x00, // Header placeholders
        ]);
        ch.extend_from_slice(b"\x01\x00\x00"); // ClientHello type and length placeholders
        let client_hello_payload_start = handshake.len();
        ch.extend_from_slice(&TLS_VERSION_1_3);
        ch.extend_from_slice(&client_random);
        ch.push(0); // Session ID length
        ch.extend_from_slice(&[0x02, 0x01]); // GK: or 0x01, 0x02 // Cipher suites length (2), then suite
        ch.extend_from_slice(&CYPHER_SUITES);
        ch.push(0); // Compression methods length (0)
        let extensions_start = ch.len();
        ch.push(0x8D);
        ch.push(0); // Extensions length (calculated later)
        // Supported groups (extension 10)
        ch.extend_from_slice(&[0x00, 0x0A, 0x00, 0x02]); // Ext type, length
        ch.extend_from_slice(&GROUPS);
        // Signature algorithmsextension 13)
        ch.extend_from_slice(&[0x00, 0x0D, 0x00, 0x04]); // Ext type, length
        ch.extend_from_slice(&SIG_ALGS);
        // Key share (extension 51)
        ch.extend_from_slice(&[0x00, 0x33, 0x00, 0x24]); // Ext type, length (36)
        ch.extend_from_slice(&GROUPS); // Group
        ch.extend_from_slice(&[32, 0]); //0x0020 as u16); // Key length 32
        ch.extend_from_slice(pub_key_bytes);
        // ALPN (extension 16, "http/1.1")
        ch.extend_from_slice(&[
            0x00, 0x10, 0x00, 0x0B, 0x08, 0x68, 0x74, 0x74, 0x70, 0x2f, 0x31, 0x2e, 0x31,
        ]); // Big endian length
        // Backfill lengths
        let client_hello_payload_len = ch.len() - client_hello_payload_start;
        let extensions_len = ch.len() - extensions_start;
        ch[client_hello_payload_start - 3..client_hello_payload_start]
            .copy_from_slice(&(client_hello_payload_len as u24));
        ch[extensions_start - 2..extensions_start].copy_from_slice(&(extensions_len as u16));

        let record_len = (ch.len() - 5) as u16;
        ch[3..5].copy_from_slice(&record_len.to_be_bytes());

        // Send ClientHello
        self.tcp.write_all(&ch)?;

        Ok(())
    }
}
