use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use ring::agreement::{EphemeralPrivateKey, UnparsedPublicKey, X25519, agree_ephemeral};
use ring::digest::{Context, SHA256};
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use std::io::{self, Read, Write};
use std::net::TcpStream;

pub struct ManualTlsStream {
    tcp: TcpStream,
    write_key: LessSafeKey,
    write_seq: u64,
    read_key: LessSafeKey,
    read_seq: u64,
    write_buf: Vec<u8>,
}

impl ManualTlsStream {
    pub fn new(tcp: TcpStream) -> io::Result<Self> {
        Ok(Self {
            tcp,
            write_key: LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &[0; 32]).unwrap()),
            write_seq: 0,
            read_key: LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &[0; 32]).unwrap()),
            read_seq: 0,
            write_buf: Vec::with_capacity(16 * 1024),
        })
    }

    pub fn handshake(&mut self) -> anyhow::Result<()> {
        let rng = SystemRandom::new();
        let mut client_random = [0u8; 32];
        rng.fill(&mut client_random);

        // Generate ephemeral key pair
        let client_priv = EphemeralPrivateKey::generate(&X25519, &rng)
            .map_err(|_| anyhow::anyhow!("EphemeralPrivateKey.generate unspecified err"))?;
        let client_pub = client_priv.compute_public_key().map_err(|_| {
            anyhow::anyhow!("EphemeralPrivateKey.compute_public_key unspecified err")
        })?;

        // Construct ClientHello
        let mut ch = Vec::with_capacity(128);
        ch.extend_from_slice(&[0x03, 0x03]); // TLS 1.2 version (legacy)
        ch.extend_from_slice(&client_random);
        ch.push(0x00); // Session ID (empty)
        ch.extend_from_slice(&[0x00, 0x02, 0x13, 0x01]); // Cipher suites (TLS_AES_256_GCM_SHA384)
        ch.extend_from_slice(&[0x01, 0x00]); // Compression methods (none)
        // Extensions
        ch.extend_from_slice(&[0x00, 0x2E]); // Extensions length
        ch.extend_from_slice(&[0x00, 0x0D, 0x00, 0x26, 0x00, 0x24]); // KeyShare extension
        ch.extend_from_slice(&[0x00, 0x1D]); // Group: X25519
        ch.extend_from_slice(&[0x00, 0x20]); // Key exchange length
        ch.extend_from_slice(client_pub.as_ref());
        ch.extend_from_slice(&[0x00, 0x17, 0x00, 0x00]); // Supported versions (TLS 1.3)
        ch.extend_from_slice(&[
            0x00, 0x10, 0x00, 0x0E, 0x00, 0x0C, 0x08, 0x68, 0x74, 0x74, 0x70, 0x2F, 0x31, 0x2E,
            0x31,
        ]); // ALPN (http/1.1)

        // Send ClientHello
        self.write_record(0x16, &ch)?; // Handshake type

        // Read ServerHello
        let mut sh = vec![0u8; 1024];
        let n = self.read_record(0x16, &mut sh)?; // Handshake type
        sh.truncate(n);

        // Parse ServerHello (simplified)
        if sh[0] != 0x02 {
            anyhow::bail!("Not ServerHello, byte 0 should be 0x02");
        }
        let server_random = &sh[6..38];
        let server_pub_bytes = &sh[81..113]; // Simplified offset for key share

        // Perform key exchange
        let server_pub = UnparsedPublicKey::new(&X25519, server_pub_bytes);
        let shared_secret = agree_ephemeral(client_priv, &server_pub, |secret| secret.to_vec())
            .map_err(|_| anyhow::anyhow!("agree_ephemeral"))?;

        // Derive keys (simplified HKDF for PoC)
        let write_key = hkdf_derive(&shared_secret, b"client write key");
        let read_key = hkdf_derive(&shared_secret, b"server write key");

        self.write_key = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &write_key).unwrap());
        self.read_key = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &read_key).unwrap());

        // Send Finished (simplified)
        let finished = compute_finished(&write_key, &client_random, server_random);
        self.write_record(0x16, &finished)?; // Handshake type

        Ok(())
    }

    fn write_record(&mut self, typ: u8, data: &[u8]) -> anyhow::Result<()> {
        let nonce = Self::nonce(self.write_seq);
        let mut in_out = data.clone();
        let tag = self
            .write_key
            .seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| anyhow::anyhow!("seal_in_place_separate_tag"))?;
        self.tcp.write_all(&[typ, 0x03, 0x03])?; // Type, version
        let tag_len: u16 = 16;
        self.tcp
            .write_all(&(tag_len + in_out.len() as u16).to_be_bytes())?; // Length
        self.tcp.write_all(in_out)?;
        self.tcp.write_all(tag.as_ref())?;
        self.write_seq += 1;
        Ok(())
    }

    fn read_record(&mut self, typ: u8, buf: &mut [u8]) -> anyhow::Result<usize> {
        let mut header = [0u8; 5];
        self.tcp.read_exact(&mut header)?;
        if header[0] != typ {
            anyhow::bail!("Unexpected record type");
        }
        let len = u16::from_be_bytes([header[3], header[4]]) as usize;
        if len > buf.len() {
            anyhow::bail!("Record too large");
        }
        self.tcp.read_exact(&mut buf[..len])?;

        let nonce = Self::nonce(self.read_seq);
        let mut data_and_tag = buf[..len].to_vec();
        let plaintext = self
            .read_key
            .open_in_place(nonce, Aad::empty(), &mut data_and_tag)
            .map_err(|_| anyhow::anyhow!("open_in_place err"))?;
        let plaintext_len = plaintext.len();
        buf[..plaintext_len].copy_from_slice(plaintext);
        self.read_seq += 1;
        Ok(plaintext_len)
    }

    fn nonce(seq: u64) -> Nonce {
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&seq.to_be_bytes());
        Nonce::assume_unique_for_key(nonce)
    }
}

impl Read for ManualTlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.write_buf.is_empty() {
            let mut record = [0u8; 16 * 1024];
            let n = self.read_record(0x17, &mut record).unwrap(); // Application data
            self.write_buf.extend_from_slice(&record[..n]);
        }
        let len = std::cmp::min(buf.len(), self.write_buf.len());
        buf[..len].copy_from_slice(&self.write_buf[..len]);
        self.write_buf.drain(..len);
        Ok(len)
    }
}

impl Write for ManualTlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_record(0x17, buf)
            .map_err(|err| io::Error::other(err)); // Application data
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}

fn hkdf_derive(secret: &[u8], label: &[u8]) -> [u8; 32] {
    let salt = &[0u8; 32]; // No salt for PoC
    let mut key = [0u8; 32];
    let prk = hmac::Key::new(hmac::HMAC_SHA256, salt);
    let mut context = Context::with_prk(&prk, label).unwrap();
    context.update(secret);
    let digest = context.finish();
    key.copy_from_slice(digest.as_ref());
    key
}

fn compute_finished(key: &[u8], client_random: &[u8], server_random: &[u8]) -> Vec<u8> {
    let mut ctx = Context::new(&SHA256);
    ctx.update(client_random);
    ctx.update(server_random);
    ctx.update(key);
    let digest = ctx.finish();
    digest.as_ref().to_vec()
}
