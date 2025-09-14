//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use ring::{aead, agreement, digest, hkdf, rand, rand::SecureRandom as _};
use std::io::{self, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

const HOST: &str = "api.openai.com";
const USER_AGENT: &str = "MyApp/1.0";
const TLS_HANDSHAKE_TYPE: u8 = 0x16;
const TLS_APPLICATION_DATA_TYPE: u8 = 0x17;
const TLS_VERSION_1_3: [u8; 2] = [0x03, 0x04];
const CYPHER_SUITES: [u8; 2] = [0x13, 0x01]; // AES_128_GCM_SHA256
const GROUPS: [u8; 2] = [0x00, 0x17]; // secp256r1
const SIG_ALGS: [u8; 8] = [0x00, 0x04, 0x04, 0x01, 0x04, 0x03, 0x02, 0x01]; // rsa_pkcs1_sha256, ecdsa_sha256, rsa_sha256, rsa_pkcs1_sha1 (minimal for PoC)

pub struct SimpleTls<S: Read + Write> {
    inner: S,
    sealing_key: aead::SealingKey<aead::AES_128_GCM>,
    opening_key: aead::OpeningKey<aead::AES_128_GCM>,
    server_write_iv: [u8; 12],
    client_write_iv: [u8; 12],
    tx_seq: u64,
    rx_seq: u64,
}

impl SimpleTls<TcpStream> {
    pub fn handshake_and_seal(mut tcp: TcpStream) -> io::Result<Self> {
        let rng = rand::SystemRandom::new();

        // Generate client random
        let mut client_random = [0u8; 32];
        rng.fill(&mut client_random)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "rand error"))?;

        // ECDHE key share
        let private_key = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "key gen error"))?;
        let public_key = private_key
            .compute_public_key()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "pub key error"))?;
        let pub_key_bytes = public_key.as_ref();

        // Build ClientHello (simplified, only essential extensions)
        let mut handshake = Vec::new();
        handshake.extend_from_slice(&[
            TLS_HANDSHAKE_TYPE,
            TLS_VERSION_1_3[0],
            TLS_VERSION_1_3[1],
            0x00,
            0x00,
            0x00,
            0x00,
            0x00, // Header placeholders
        ]);
        handshake.extend_from_slice(b"\x01\x00\x00"); // ClientHello type and length placeholders
        let client_hello_payload_start = handshake.len();
        handshake.extend_from_slice(&TLS_VERSION_1_3);
        handshake.extend_from_slice(&client_random);
        handshake.push(0); // Session ID length
        handshake.extend_from_slice(&[0x02, 0x01]); // GK: or 0x01, 0x02 // Cipher suites length (2), then suite
        handshake.extend_from_slice(&CYPHER_SUITES);
        handshake.push(0); // Compression methods length (0)
        let extensions_start = handshake.len();
        handshake.push(0x8D);
        handshake.push(0); // Extensions length (calculated later)
        // Supported groups (extension 10)
        handshake.extend_from_slice(&[0x00, 0x0A, 0x00, 0x02]); // Ext type, length
        handshake.extend_from_slice(&GROUPS);
        // Signature algorithmsextension 13)
        handshake.extend_from_slice(&[0x00, 0x0D, 0x00, 0x04]); // Ext type, length
        handshake.extend_from_slice(&SIG_ALGS);
        // Key share (extension 51)
        handshake.extend_from_slice(&[0x00, 0x33, 0x00, 0x24]); // Ext type, length (36)
        handshake.extend_from_slice(&GROUPS); // Group
        handshake.extend_from_slice(&[32, 0]); //0x0020 as u16); // Key length 32
        handshake.extend_from_slice(pub_key_bytes);
        // ALPN (extension 16, "http/1.1")
        handshake.extend_from_slice(&[
            0x00, 0x10, 0x00, 0x0B, 0x08, 0x68, 0x74, 0x74, 0x70, 0x2f, 0x31, 0x2e, 0x31,
        ]); // Big endian length
        // Backfill lengths
        let client_hello_payload_len = handshake.len() - client_hello_payload_start;
        let extensions_len = handshake.len() - extensions_start;
        handshake[client_hello_payload_start - 3..client_hello_payload_start]
            .copy_from_slice(&(client_hello_payload_len as u24));
        handshake[extensions_start - 2..extensions_start].copy_from_slice(&(extensions_len as u16));

        let record_len = (handshake.len() - 5) as u16;
        handshake[3..5].copy_from_slice(&record_len.to_be_bytes());

        // Send ClientHello
        tcp.write_all(&handshake)?;

        // Read ServerHello (simplified, assume sequential response)
        let mut server_hello_buffer = [0u8; 1024];
        tcp.read(&mut server_hello_buffer)?;
        let (_version, _random, server_pub_key_bytes) = parse_server_hello(&server_hello_buffer)?;

        // Compute shared secret
        let server_pub_key =
            agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, server_pub_key_bytes.clone());
        let shared_secret = agreement::agree_ephemeral(private_key, &server_pub_key, |secret| {
            Ok(Vec::from(secret))
        })
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "agreement error"))??;

        // Derive keys using HKDF for TLS 1.3 handshake
        let hash_alg = hkdf::HKDF_SHA256;
        let prk = hkdf::derive_secret_kdf(&hash_alg, &shared_secret, &[], &[]);
        let master_secret = hkdf::derive_secret_kdf(&hash_alg, &prk, b"master", &[]);
        let client_write_key =
            hkdf::derive(&hash_alg, &master_secret, b"c a traffic", &[]).unwrap();
        let server_write_key =
            hkdf::derive(&hash_alg, &master_secret, b"s a traffic", &[]).unwrap();
        let client_write_iv = hkdf::derive(&hash_alg, &master_secret, b"c iv", &[]).unwrap();
        let server_write_iv = hkdf::derive(&hash_alg, &master_secret, b"s iv", &[]).unwrap();

        // Create AEAD keys
        let sealing_key = aead::SealingKey::new(&aead::AES_128_GCM, &client_write_key).unwrap();
        let opening_key = aead::OpeningKey::new(&aead::AES_128_GCM, &server_write_key).unwrap();

        // Create TlsStream
        Ok(SimpleTls {
            inner: tcp,
            sealing_key,
            opening_key,
            server_write_iv,
            client_write_iv,
            tx_seq: 0,
            rx_seq: 0,
        })
    }

    fn parse_server_hello(_buffer: &[u8; 1024]) -> io::Result<(u16, [u8; 32], Vec<u8>)> {
        // Simplified parsing: extract version, random, server pub key from KeyShare extension
        // Skip to server pub key bytes (hardcoded offset for PoC)
        let server_pub_key_bytes = vec![0xde, 0xad, 0xbe, 0xef]; // Placeholder, in real PoC parse properly
        Ok((0x0303, [0u8; 32], server_pub_key_bytes))
    }
}

impl<S: Read + Write> Write for SimpleTls<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut nonce = [0u8; 12];
        for i in 0..12 {
            nonce[i] = self.client_write_iv[i] ^ ((self.tx_seq >> ((11 - i) * 8)) & 0xFF) as u8;
        }
        let mut ciphertext = buf.to_vec();
        let tag = aead::seal(&self.sealing_key, &nonce, &[], &mut ciphertext).unwrap();
        ciphertext.extend_from_slice(&tag);

        let record = [
            TLS_APPLICATION_DATA_TYPE,
            TLS_VERSION_1_3[0],
            TLS_VERSION_1_3[1],
            ((ciphertext.len() >> 8) as u8),
            (ciphertext.len() as u8),
        ];
        self.inner.write_all(&record)?;
        self.inner.write_all(&ciphertext)?;
        self.tx_seq += 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<S: Read + Write> Read for SimpleTls<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut record_buf = [0u8; 5];
        self.inner.read_exact(&mut record_buf)?;
        if record_buf[0] != TLS_APPLICATION_DATA_TYPE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected record type",
            ));
        }
        let record_len = ((record_buf[3] as usize) << 8) | (record_buf[4] as usize);
        let mut ciphertext = vec![0u8; record_len];
        self.inner.read_exact(&mut ciphertext)?;

        let mut nonce = [0u8; 12];
        for i in 0..12 {
            nonce[i] = self.server_write_iv[i] ^ ((self.rx_seq >> ((11 - i) * 8)) & 0xFF) as u8;
        }
        let plaintext = aead::open(&self.opening_key, &nonce, &[], &ciphertext).unwrap();
        let len = plaintext.len();
        buf[..len].copy_from_slice(&plaintext);
        self.rx_seq += 1;
        Ok(len)
    }
}
