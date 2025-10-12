//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! ---------------------- Minimal TLS 1.3 client (AES-128-GCM + X25519) -------

use std::fs::File;
use std::io::{self, Read, Write};
use std::net::TcpStream;

use ring::aead;
use ring::agreement::{self, EphemeralPrivateKey, PublicKey, UnparsedPublicKey, X25519};
use ring::digest;
use ring::hkdf;
use ring::hkdf::KeyType;
use ring::rand::{SecureRandom as _, SystemRandom};

const DEBUG_LOG: bool = false;

const REC_TYPE_CHANGE_CIPHER_SPEC: u8 = 20; // 0x14
const REC_TYPE_ALERT: u8 = 21; // 0x15
const REC_TYPE_HANDSHAKE: u8 = 22; // 0x16
const REC_TYPE_APPDATA: u8 = 23; // 0x17
const LEGACY_REC_VER: u16 = 0x0303;

const HS_CLIENT_HELLO: u8 = 1;
const HS_SERVER_HELLO: u8 = 2;
//const HS_NEW_SESSION_TICKET: u8 = 4;
//const HS_ENCRYPTED_EXTENSIONS: u8 = 8;
//const HS_CERTIFICATE: u8 = 11;
//const HS_CERT_VERIFY: u8 = 15;
const HS_FINISHED: u8 = 20; // 0x14

// TLS_AES_128_GCM_SHA256
const CIPHER_TLS_AES_128_GCM_SHA256: u16 = 0x1301;
// supported_versions (TLS 1.3)
const TLS13: u16 = 0x0304;
// supported group: x25519
const GROUP_X25519: u16 = 0x001d;

// Extensions
const EXT_SERVER_NAME: u16 = 0x0000;
const EXT_SUPPORTED_GROUPS: u16 = 0x000a;
const EXT_SIGNATURE_ALGS: u16 = 0x000d;
//const EXT_ALPN: u16 = 0x0010;
const EXT_SUPPORTED_VERSIONS: u16 = 0x002b;
//const EXT_PSK_MODES: u16 = 0x002d;
const EXT_KEY_SHARE: u16 = 0x0033;

// AEAD tag length (GCM)
const AEAD_TAG_LEN: usize = 16;

// Tiny helper to write BE ints
fn put_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}
fn put_u24(buf: &mut Vec<u8>, v: usize) {
    let v = v as u32;
    buf.extend_from_slice(&[(v >> 16) as u8, (v >> 8) as u8, v as u8]);
}

// HKDF-Expand-Label as per RFC8446
fn hkdf_expand_label(prk: &hkdf::Prk, label: &str, context: &[u8], out: &mut [u8]) {
    let mut info = Vec::with_capacity(2 + 1 + 6 + label.len() + 1 + context.len());
    put_u16(&mut info, out.len() as u16);
    let full_label = format!("tls13 {}", label);
    info.push(full_label.len() as u8);
    info.extend_from_slice(full_label.as_bytes());
    info.push(context.len() as u8);
    info.extend_from_slice(context);

    let info_slice: &[&[u8]] = &[&info];
    let okm = prk.expand(info_slice, Len(out.len())).expect("HKDF expand");
    okm.fill(out).expect("HKDF fill");
}

fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> hkdf::Prk {
    hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(ikm)
}

fn digest_bytes(data: &[u8]) -> [u8; 32] {
    let d = digest::digest(&digest::SHA256, data);
    let mut out = [0u8; 32];
    out.copy_from_slice(d.as_ref());
    out
}

// AEAD nonce = iv XOR seq (seq in BE on the rightmost 8 bytes)
fn nonce_xor(iv12: &[u8; 12], seq: u64) -> aead::Nonce {
    // seq number in big endian on rightmost 8 bytes
    let mut nonce_bytes = [[0, 0, 0, 0].as_ref(), &u64::to_be_bytes(seq)].concat();
    // xor them
    nonce_bytes.iter_mut().zip(iv12.iter()).for_each(|(s, iv)| {
        *s ^= *iv;
    });
    aead::Nonce::assume_unique_for_key(nonce_bytes[..12].try_into().unwrap())
}

// Very small record writer/reader after handshake
pub struct TlsStream {
    io: TcpStream,
    // Application traffic
    aead_enc: aead::LessSafeKey,
    aead_dec: aead::LessSafeKey,
    iv_enc: [u8; 12],
    iv_dec: [u8; 12],
    seq_enc: u64,
    seq_dec: u64,
    // read buffer for decrypted application data
    rbuf: Vec<u8>,
    rpos: usize,
}

fn client_hello_body(
    rng: &SystemRandom,
    sni_host: &str,
    client_pub: &PublicKey,
) -> anyhow::Result<Vec<u8>> {
    let mut ch_body = Vec::with_capacity(512);

    // X25519
    let mut random = [0u8; 32];
    let _ = rng.fill(&mut random);
    let mut session_id = [0u8; 32];
    let _ = rng.fill(&mut session_id);

    // legacy_version
    ch_body.extend_from_slice(&0x0303u16.to_be_bytes());
    // random
    ch_body.extend_from_slice(&random);
    // legacy_session_id
    ch_body.push(session_id.len() as u8);
    ch_body.extend_from_slice(&session_id);
    // cipher_suites: only TLS_AES_128_GCM_SHA256
    put_u16(&mut ch_body, 2);
    put_u16(&mut ch_body, CIPHER_TLS_AES_128_GCM_SHA256);
    // legacy_compression_methods: null
    ch_body.push(1);
    ch_body.push(0);

    // --- extensions ---
    let mut exts = Vec::with_capacity(512);

    // server_name
    {
        let host_bytes = sni_host.as_bytes();
        let mut snl = Vec::with_capacity(3 + host_bytes.len());
        snl.push(0); // host_name
        put_u16(&mut snl, host_bytes.len() as u16);
        snl.extend_from_slice(host_bytes);

        let mut sni = Vec::with_capacity(2 + snl.len());
        put_u16(&mut sni, snl.len() as u16);
        sni.extend_from_slice(&snl);

        put_u16(&mut exts, EXT_SERVER_NAME);
        put_u16(&mut exts, sni.len() as u16);
        exts.extend_from_slice(&sni);
    }

    // supported_versions: TLS 1.3
    {
        let mut sv = Vec::with_capacity(3);
        sv.push(2); // length in bytes
        sv.extend_from_slice(&TLS13.to_be_bytes());
        put_u16(&mut exts, EXT_SUPPORTED_VERSIONS);
        put_u16(&mut exts, sv.len() as u16);
        exts.extend_from_slice(&sv);
    }

    // supported_groups: x25519
    {
        let mut sg = Vec::with_capacity(2 + 2);
        put_u16(&mut sg, 2);
        put_u16(&mut sg, GROUP_X25519);
        put_u16(&mut exts, EXT_SUPPORTED_GROUPS);
        put_u16(&mut exts, sg.len() as u16);
        exts.extend_from_slice(&sg);
    }

    // signature_algorithms: minimal list
    {
        const ECDSA_SECP256R1_SHA256: u16 = 0x0403;
        const RSA_PSS_RSAE_SHA256: u16 = 0x0804;
        const RSA_PKCS1_SHA256: u16 = 0x0401;

        let mut sa = Vec::with_capacity(2 + 6);
        put_u16(&mut sa, 6);
        put_u16(&mut sa, ECDSA_SECP256R1_SHA256);
        put_u16(&mut sa, RSA_PSS_RSAE_SHA256);
        put_u16(&mut sa, RSA_PKCS1_SHA256);

        put_u16(&mut exts, EXT_SIGNATURE_ALGS);
        put_u16(&mut exts, sa.len() as u16);
        exts.extend_from_slice(&sa);
    }

    // key_share: x25519
    {
        let mut ks = Vec::with_capacity(2 + 2 + 2 + 32);
        // client_shares vector
        let mut entry = Vec::with_capacity(2 + 2 + 32);
        put_u16(&mut entry, GROUP_X25519);
        put_u16(&mut entry, 32);
        entry.extend_from_slice(client_pub.as_ref());
        put_u16(&mut ks, entry.len() as u16);
        ks.extend_from_slice(&entry);

        put_u16(&mut exts, EXT_KEY_SHARE);
        put_u16(&mut exts, ks.len() as u16);
        exts.extend_from_slice(&ks);
    }

    // add extensions to CH
    put_u16(&mut ch_body, exts.len() as u16);
    ch_body.extend_from_slice(&exts);

    Ok(ch_body)
}

/// --- Build ClientHello (single cipher: TLS_AES_128_GCM_SHA256) ---
fn client_hello_msg(
    rng: &SystemRandom,
    sni_host: &str,
    client_private_key: &EphemeralPrivateKey,
) -> anyhow::Result<Vec<u8>> {
    let client_pub = client_private_key
        .compute_public_key()
        .map_err(|_| anyhow::anyhow!("x25519 pub"))?;

    let ch_body = client_hello_body(rng, sni_host, &client_pub)?;

    // Handshake framing: ClientHello
    let mut ch_msg = Vec::with_capacity(4 + ch_body.len());
    ch_msg.push(HS_CLIENT_HELLO);
    put_u24(&mut ch_msg, ch_body.len());
    ch_msg.extend_from_slice(&ch_body);

    Ok(ch_msg)
}

/// Read ServerHello (plaintext Handshake record)
fn read_server_hello(io: &mut TcpStream) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let (typ, payload) = read_record_plain(io)?;
    if typ != REC_TYPE_HANDSHAKE {
        anyhow::bail!("expected Handshake");
    }
    let sh_buf = payload;

    // There can be multiple handshake messages; we need the ServerHello bytes specifically
    let mut rd = &sh_buf[..];
    let (sh_typ, sh_body, sh_full) = read_handshake_message(&mut rd)?;
    if sh_typ != HS_SERVER_HELLO {
        anyhow::bail!("expected ServerHello");
    }

    // TODO: later remove the copy. The slices are into sh_buf
    Ok((sh_body.to_vec(), sh_full.to_vec()))
}

struct HandshakeState {
    handshake_secret: hkdf::Prk,
    client_hs_ts: [u8; 32],
    server_hs_ts: [u8; 32],
    client_handshake_iv: [u8; 12],
    server_handshake_iv: [u8; 12],
    aead_enc_hs: aead::LessSafeKey,
    aead_dec_hs: aead::LessSafeKey,
    empty_hash: [u8; 32],
}

impl TlsStream {
    pub fn connect(mut io: TcpStream, sni_host: &str) -> anyhow::Result<Self> {
        let rng = ring::rand::SystemRandom::new();

        // transcript = full Handshake message encodings (headers + bodies)
        let mut transcript = Vec::with_capacity(1024);

        let client_private_key = EphemeralPrivateKey::generate(&X25519, &rng)
            .map_err(|_| anyhow::anyhow!("x25519 keygen"))?;

        Self::send_client_hello(
            &mut io,
            sni_host,
            &mut transcript,
            &rng,
            &client_private_key,
        )?;

        let (sh_body, sh_full) = Self::receive_server_hello(&mut io, &mut transcript)?;

        let handshake = Self::derive_handshake_keys(client_private_key, &sh_body, &transcript)?;

        let mut seq_dec_hs = 0u64;
        let mut seq_enc_hs = 0u64;

        // --- Skip Change Cipher Spec. We could do this earlier, before calculating keys.
        // In TLS 1.3 it is used to confuse middleboxes expecting TLS 1.2
        let (typ, _) = read_record_plain(&mut io)?;
        if typ != REC_TYPE_CHANGE_CIPHER_SPEC {
            anyhow::bail!("Expected server to send dummy Change Cipher Spec");
        }

        // ---- Receive EncryptedExtensions, (Certificate, CertVerify), Finished ----
        let (typ, ct, _inner_type) = read_record_cipher(
            &mut io,
            &handshake.aead_dec_hs,
            &handshake.server_handshake_iv,
            &mut seq_dec_hs,
        )?;
        if typ != REC_TYPE_APPDATA {
            anyhow::bail!("expected encrypted records");
        }

        // Decrypted TLSInnerPlaintext: ... | content_type
        // May contain multiple handshake messages; parse & append to transcript.
        let mut p = &ct[..];
        while !p.is_empty() {
            // On TLS 1.3: content_type is last byte; but ring decrypt gives only plaintext,
            // here ct already stripped of content-type 0x16 by read_record_cipher().
            let (mtyp, body, full) = match read_handshake_message(&mut p) {
                Ok(x) => x,
                Err(_) => anyhow::bail!("bad handshake fragment"),
            };
            transcript.extend_from_slice(full);

            if mtyp == HS_FINISHED {
                // verify server Finished
                let mut s_finished_key = [0u8; 32];
                let s_prk = hkdf::Prk::new_less_safe(hkdf::HKDF_SHA256, &handshake.server_hs_ts);
                hkdf_expand_label(&s_prk, "finished", &[], &mut s_finished_key);

                let thash = digest_bytes(&transcript[..transcript.len() - full.len()]);
                let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &s_finished_key);
                let expected = ring::hmac::sign(&key, &thash);
                if expected.as_ref() != body {
                    anyhow::bail!("server Finished verify failed");
                }
                // Done collecting server handshake.
                break;
            }
            // Ignore other handshake types’ contents (no cert validation).
        }

        // ---- Derive application traffic keys ----
        // This is correct
        let mut derived2_bytes = [0u8; 32];
        hkdf_expand_label(
            &handshake.handshake_secret,
            "derived",
            &handshake.empty_hash,
            &mut derived2_bytes,
        );
        debug_print("derived2_bytes", &derived2_bytes);

        let zero: [u8; 32] = [0u8; 32];
        let master_secret = hkdf_extract(&derived2_bytes, &zero);
        let thash_srv_fin = digest_bytes(&transcript);

        let mut c_ap_ts = [0u8; 32];
        let mut s_ap_ts = [0u8; 32];
        hkdf_expand_label(&master_secret, "c ap traffic", &thash_srv_fin, &mut c_ap_ts);
        hkdf_expand_label(&master_secret, "s ap traffic", &thash_srv_fin, &mut s_ap_ts);
        debug_print("c_ap_ts", &c_ap_ts);
        debug_print("s_ap_ts", &s_ap_ts);

        let mut cak = [0u8; 16];
        let mut caiv = [0u8; 12];
        let mut sak = [0u8; 16];
        let mut saiv = [0u8; 12];
        {
            let c_prk = hkdf::Prk::new_less_safe(hkdf::HKDF_SHA256, &c_ap_ts);
            let s_prk = hkdf::Prk::new_less_safe(hkdf::HKDF_SHA256, &s_ap_ts);
            let (key_len, iv_len) = (16, 12);
            hkdf_expand_label(&c_prk, "key", &[], &mut cak[..key_len]);
            hkdf_expand_label(&c_prk, "iv", &[], &mut caiv[..iv_len]);
            hkdf_expand_label(&s_prk, "key", &[], &mut sak[..key_len]);
            hkdf_expand_label(&s_prk, "iv", &[], &mut saiv[..iv_len]);
            debug_print("cak", &cak);
            debug_print("caiv", &caiv);
            debug_print("sak", &sak);
            debug_print("saiv", &saiv);
        }

        let aead_alg = &aead::AES_128_GCM;
        let aead_app_enc = aead::LessSafeKey::new(aead::UnboundKey::new(aead_alg, &cak).unwrap());
        let aead_app_dec = aead::LessSafeKey::new(aead::UnboundKey::new(aead_alg, &sak).unwrap());
        let seq_app_enc = 0u64;
        let seq_app_dec = 0u64;

        // Client Change Cipher Spec
        // This is optional, to "confuse middleboxes" which expect TLS 1.2. Works without.
        //write_record_plain(&mut io, REC_TYPE_CHANGE_CIPHER_SPEC, &[0x01])?;

        // ---- Send our Finished (under handshake keys) ----
        {
            let mut c_finished_key = [0u8; 32];
            let c_prk = hkdf::Prk::new_less_safe(hkdf::HKDF_SHA256, &handshake.client_hs_ts);
            hkdf_expand_label(&c_prk, "finished", &[], &mut c_finished_key);
            debug_print("c_finished", &c_finished_key);

            let thash_client_fin = digest_bytes(&transcript);
            let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &c_finished_key);
            let verify_data = ring::hmac::sign(&key, &thash_client_fin);
            debug_print("verify_data", verify_data.as_ref());

            let mut fin = Vec::with_capacity(4 + verify_data.as_ref().len());
            fin.push(HS_FINISHED);
            put_u24(&mut fin, verify_data.as_ref().len());
            fin.extend_from_slice(verify_data.as_ref());

            // append to transcript before switching keys
            transcript.extend_from_slice(&fin);

            write_record_cipher(
                &mut io,
                REC_TYPE_HANDSHAKE,
                &fin,
                &handshake.aead_enc_hs,
                &handshake.client_handshake_iv,
                &mut seq_enc_hs,
            )?;
        }
        // From now on we use application traffic keys.
        Ok(TlsStream {
            io,
            aead_enc: aead_app_enc,
            aead_dec: aead_app_dec,
            iv_enc: caiv,
            iv_dec: saiv,
            seq_enc: seq_app_enc,
            seq_dec: seq_app_dec,
            rbuf: Vec::with_capacity(16 * 1024),
            rpos: 0,
        })
    }

    fn send_client_hello(
        io: &mut TcpStream,
        sni_host: &str,
        transcript: &mut Vec<u8>,
        rng: &SystemRandom,
        client_private_key: &EphemeralPrivateKey,
    ) -> anyhow::Result<()> {
        let ch_msg = client_hello_msg(rng, sni_host, client_private_key)?;
        write_record_plain(io, REC_TYPE_HANDSHAKE, &ch_msg)?;
        transcript.extend_from_slice(&ch_msg);
        Ok(())
    }

    fn receive_server_hello(
        io: &mut TcpStream,
        transcript: &mut Vec<u8>,
    ) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let (sh_body, sh_full) = read_server_hello(io)?;
        transcript.extend_from_slice(&sh_full);
        Ok((sh_body, sh_full))
    }

    fn derive_handshake_keys(
        client_private_key: EphemeralPrivateKey,
        sh_body: &[u8],
        transcript: &[u8],
    ) -> anyhow::Result<HandshakeState> {
        // Parse minimal ServerHello to get cipher & key_share
        let (cipher, server_public_key_bytes) = parse_server_hello_for_keys(sh_body)?;
        if cipher != CIPHER_TLS_AES_128_GCM_SHA256 {
            anyhow::bail!("server picked unsupported cipher");
        }

        // ECDH(X25519) shared secret
        let server_public_key = UnparsedPublicKey::new(&X25519, &server_public_key_bytes);

        // This shared secret is correct, I checked it with `curve25519-mult`
        let hs_shared_secret =
            agreement::agree_ephemeral(client_private_key, &server_public_key, |secret| {
                secret.to_vec()
            })
            .map_err(|_| anyhow::anyhow!("ECDH failed"))?;
        debug_print("hs shared secret", &hs_shared_secret);

        // Same as: `echo -n "" | openssl sha256`
        let empty_hash = digest_bytes(&[]);
        debug_print("empty_hash", &empty_hash);

        let zero: [u8; 32] = [0u8; 32];
        let early_secret = hkdf_extract(&zero, &zero);

        let mut derived_secret_bytes = [0u8; 32];
        hkdf_expand_label(
            &early_secret,
            "derived",
            &empty_hash,
            &mut derived_secret_bytes,
        );
        debug_print("derived", &derived_secret_bytes);

        let handshake_secret = hkdf_extract(&derived_secret_bytes, &hs_shared_secret);

        let ch_sh_hash = digest_bytes(transcript);
        debug_print("digest bytes", &ch_sh_hash);

        let mut c_hs_ts = [0u8; 32];
        let mut s_hs_ts = [0u8; 32];
        hkdf_expand_label(&handshake_secret, "c hs traffic", &ch_sh_hash, &mut c_hs_ts);
        hkdf_expand_label(&handshake_secret, "s hs traffic", &ch_sh_hash, &mut s_hs_ts);
        debug_print("c hs traffic", &c_hs_ts);
        debug_print("s hs traffic", &s_hs_ts);

        // handshake AEAD keys/IVs
        let mut client_handshake_key = [0u8; 16];
        let mut client_handshake_iv = [0u8; 12];
        let c_prk = hkdf::Prk::new_less_safe(hkdf::HKDF_SHA256, &c_hs_ts);
        hkdf_expand_label(&c_prk, "key", &[], &mut client_handshake_key);
        hkdf_expand_label(&c_prk, "iv", &[], &mut client_handshake_iv);

        let mut server_handshake_key = [0u8; 16];
        let mut server_handshake_iv = [0u8; 12];
        let s_prk = hkdf::Prk::new_less_safe(hkdf::HKDF_SHA256, &s_hs_ts);
        hkdf_expand_label(&s_prk, "key", &[], &mut server_handshake_key);
        hkdf_expand_label(&s_prk, "iv", &[], &mut server_handshake_iv);

        debug_print("client_handshake_key", &client_handshake_key);
        debug_print("client_handshake_iv", &client_handshake_iv);
        debug_print("server_handshake_key", &server_handshake_key);
        debug_print("server_handshake_iv", &server_handshake_iv);

        let aead_alg = &aead::AES_128_GCM;
        let aead_dec_hs =
            aead::LessSafeKey::new(aead::UnboundKey::new(aead_alg, &server_handshake_key).unwrap());
        let aead_enc_hs =
            aead::LessSafeKey::new(aead::UnboundKey::new(aead_alg, &client_handshake_key).unwrap());

        Ok(HandshakeState {
            handshake_secret,
            client_hs_ts: c_hs_ts,
            server_hs_ts: s_hs_ts,
            client_handshake_iv,
            server_handshake_iv,
            aead_enc_hs,
            aead_dec_hs,
            empty_hash,
        })
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write_record_cipher(
            &mut self.io,
            REC_TYPE_APPDATA,
            buf,
            &self.aead_enc,
            &self.iv_enc,
            &mut self.seq_enc,
        )
        .map(|_| buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}

impl Read for TlsStream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.rpos < self.rbuf.len() {
            let n = std::cmp::min(out.len(), self.rbuf.len() - self.rpos);
            out[..n].copy_from_slice(&self.rbuf[self.rpos..self.rpos + n]);
            self.rpos += n;
            if self.rpos == self.rbuf.len() {
                self.rbuf.clear();
                self.rpos = 0;
            }
            return Ok(n);
        }
        loop {
            let (typ, plaintext, inner_type) = read_record_cipher(
                &mut self.io,
                &self.aead_dec,
                &self.iv_dec,
                &mut self.seq_dec,
            )?;
            if typ != REC_TYPE_APPDATA {
                // Ignore unexpected (e.g., post-handshake Handshake like NewSessionTicket)
                continue;
            }
            // plaintext ends with inner content type byte; for app data it is 0x17.
            if plaintext.is_empty() {
                continue;
            }
            if inner_type == REC_TYPE_HANDSHAKE {
                // Drop post-handshake messages (tickets, etc.)
                continue;
            }
            if inner_type == REC_TYPE_ALERT {
                let level = match plaintext[0] {
                    1 => "warning",
                    2 => "fatal",
                    _ => "unknown",
                };
                // See https://www.rfc-editor.org/rfc/rfc8446#appendix-B search for
                // "unexpected_message" for all types
                return Err(io_err(&format!("{level} alert: {}", plaintext[1])));
            }
            if inner_type != REC_TYPE_APPDATA {
                // Some servers pad with 0x00.. then type; we already consumed type.
                // If not 0x17, treat preceding bytes (if any) as app anyway.
            }
            if plaintext.is_empty() {
                continue;
            }

            self.rbuf.extend_from_slice(&plaintext);
            self.rpos = 0;
            // Now serve from buffer
            let n = std::cmp::min(out.len(), self.rbuf.len());
            out[..n].copy_from_slice(&self.rbuf[..n]);
            self.rpos = n;
            if n == self.rbuf.len() {
                self.rbuf.clear();
                self.rpos = 0;
            }
            return Ok(n);
        }
    }
}

// ---------------------- Record I/O helpers ----------------------------------

fn write_record_plain<W: Write>(w: &mut W, typ: u8, body: &[u8]) -> io::Result<()> {
    let mut hdr = [0u8; 5];
    hdr[0] = typ;
    hdr[1..3].copy_from_slice(&LEGACY_REC_VER.to_be_bytes());
    hdr[3..5].copy_from_slice(&(body.len() as u16).to_be_bytes());
    w.write_all(&hdr)?;
    w.write_all(body)?;
    Ok(())
}

fn read_exact_n<R: Read>(r: &mut R, n: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

fn read_record_plain(r: &mut TcpStream) -> io::Result<(u8, Vec<u8>)> {
    let hdr = read_exact_n(r, 5)?; // Record Header, e.g. 16 03 03 len
    let typ = hdr[0];
    let len = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
    let body = read_exact_n(r, len)?;
    //let _ = write_bytes_to_file(&[&hdr[..], &body].concat(), debug_filename);
    Ok((typ, body))
}

fn write_record_cipher<W: Write>(
    w: &mut W,
    outer_type: u8,
    inner: &[u8],
    key: &aead::LessSafeKey,
    iv12: &[u8; 12],
    seq: &mut u64,
) -> io::Result<()> {
    // AES / GCM plaintext and ciphertext have the same length
    let total_len = inner.len() + 1 + AEAD_TAG_LEN;
    let mut plain = Vec::with_capacity(total_len);
    plain.extend_from_slice(inner);
    plain.push(outer_type);

    debug_print("write_record_cipher plaintext", &plain);

    let nonce = nonce_xor(iv12, *seq);
    *seq = seq.wrapping_add(1);

    let mut hdr = [0u8; 5];
    hdr[0] = REC_TYPE_APPDATA;
    hdr[1..3].copy_from_slice(&LEGACY_REC_VER.to_be_bytes());
    hdr[3..5].copy_from_slice(&(total_len as u16).to_be_bytes());

    let mut inout = plain;
    key.seal_in_place_append_tag(nonce, aead::Aad::from(&hdr), &mut inout)
        .map_err(|_| io_err("seal aad"))?;

    debug_print("write_record_cipher header", &hdr);
    let final_label = format!("write_record_cipher final {total_len}");
    debug_print(final_label.as_str(), &inout);

    w.write_all(&hdr)?;
    w.write_all(&inout)?;
    Ok(())
}

fn read_record_cipher<R: Read>(
    r: &mut R,
    key: &aead::LessSafeKey,
    iv12: &[u8; 12],
    seq: &mut u64,
) -> io::Result<(u8, Vec<u8>, u8)> {
    let hdr = read_exact_n(r, 5)?;
    let typ = hdr[0];
    let len = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
    let mut ct = read_exact_n(r, len)?;
    if len < AEAD_TAG_LEN {
        return Err(io_err("short record"));
    }
    debug_print("read_record_cipher hdr", &hdr);
    debug_print("read_record_cipher ct", &ct);

    // Decrypt ciphertext
    // ct will contain the plaintext

    let nonce = nonce_xor(iv12, *seq);
    *seq = seq.wrapping_add(1);

    let aad = aead::Aad::from(&hdr);
    let mut ct = key
        .open_in_place(nonce, aad, &mut ct)
        .map_err(|_| io_err("open"))?
        .to_vec();
    debug_print("read_record_cipher plaintext hdr", &hdr);
    debug_print("read_record_cipher plaintext", &ct);

    if ct.is_empty() {
        return Ok((typ, ct, 0));
    }
    // Strip inner content-type byte
    let inner_type = *ct.last().unwrap();
    ct.truncate(ct.len() - 1);
    Ok((typ, ct, inner_type))
}

// ---------------------- Handshake parsing helpers ---------------------------

fn read_handshake_message<'a>(rd: &mut &'a [u8]) -> io::Result<(u8, &'a [u8], &'a [u8])> {
    if rd.len() < 4 {
        return Err(io_err("short hs"));
    }
    let typ = rd[0];
    let len = ((rd[1] as usize) << 16) | ((rd[2] as usize) << 8) | rd[3] as usize;
    if rd.len() < 4 + len {
        return Err(io_err("short hs body"));
    }
    let full = &rd[..4 + len];
    let body = &rd[4..4 + len];
    *rd = &rd[4 + len..];
    Ok((typ, body, full))
}

fn parse_server_hello_for_keys(sh: &[u8]) -> io::Result<(u16, [u8; 32])> {
    // minimal parse: skip legacy_version(2), random(32), sid, cipher(2), comp(1), exts
    if sh.len() < 2 + 32 + 1 + 2 + 1 + 2 {
        return Err(io_err("sh too short"));
    }
    let mut p = sh;

    p = &p[2..]; // legacy_version
    p = &p[32..]; // random
    let sid_len = p[0] as usize;
    p = &p[1..];
    if p.len() < sid_len + 2 + 1 + 2 {
        return Err(io_err("sh sid"));
    }
    p = &p[sid_len..];
    let cipher = u16::from_be_bytes([p[0], p[1]]);
    p = &p[2..];
    let _comp = p[0];
    p = &p[1..];
    let ext_len = u16::from_be_bytes([p[0], p[1]]) as usize;
    p = &p[2..];
    if p.len() < ext_len {
        return Err(io_err("sh ext too short"));
    }
    let mut ex = &p[..ext_len];

    let mut server_pub = None;

    while !ex.is_empty() {
        if ex.len() < 4 {
            return Err(io_err("ext short"));
        }
        let et = u16::from_be_bytes([ex[0], ex[1]]);
        let el = u16::from_be_bytes([ex[2], ex[3]]) as usize;
        ex = &ex[4..];
        if ex.len() < el {
            return Err(io_err("ext len"));
        }
        let ed = &ex[..el];
        ex = &ex[el..];

        match et {
            EXT_KEY_SHARE => {
                // KeyShareServerHello: group(2) kx_len(2) kx
                if ed.len() < 2 + 2 + 32 {
                    return Err(io_err("ks sh"));
                }
                let grp = u16::from_be_bytes([ed[0], ed[1]]);
                if grp != GROUP_X25519 {
                    return Err(io_err("server group != x25519"));
                }
                let kx_len = u16::from_be_bytes([ed[2], ed[3]]) as usize;
                if ed.len() < 4 + kx_len || kx_len != 32 {
                    return Err(io_err("kx len"));
                }
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&ed[4..4 + 32]);
                server_pub = Some(pk);
            }
            EXT_SUPPORTED_VERSIONS => {
                if ed.len() != 2 || u16::from_be_bytes([ed[0], ed[1]]) != TLS13 {
                    return Err(io_err("server not TLS1.3"));
                }
            }
            _ => {}
        }
    }

    let sp = server_pub.ok_or_else(|| io_err("no server key"))?;
    Ok((cipher, sp))
}

// Fix `ring` weird size choices
// rustls does this too
struct Len(usize);

impl KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn io_err(msg: &str) -> io::Error {
    io::Error::other(msg)
}

fn debug_print(name: &str, value: &[u8]) {
    if !DEBUG_LOG {
        return;
    }
    eprintln!("\n{name} {}:", value.len());
    print_hex(value);
}

fn print_hex(v: &[u8]) {
    let hex: String = v
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("");
    eprintln!("{hex}");
}

#[allow(dead_code)]
fn write_bytes_to_file(bytes: &[u8], file_path: &str) -> std::io::Result<()> {
    let mut file = File::create(file_path)?;
    file.write_all(bytes)?;
    Ok(())
}
