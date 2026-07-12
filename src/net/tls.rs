//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! ---------------------- Minimal TLS 1.3 client (AES-128-GCM + X25519) -------

use core::{cmp, ffi::CStr};

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    Context, ErrorKind, OrtResult, Read, Write, common::utils::to_ascii, net::AsFd, ort_error,
    syscall,
};

mod aead;
mod ecdh;
mod hkdf;
mod hmac;
mod sha2;

#[allow(unused)]
const DEBUG_LOG: bool = false;

/// RFC 8445 5.1, "carrying data in chunks of 2^14 bytes or less"
const MAX_PLAINTEXT_SIZE: usize = 16 * 1024;

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

fn hkdf_expand_label<const N: usize>(prk: &[u8], label: &str, data: &[u8]) -> [u8; N] {
    let mut info = Vec::with_capacity(2 + 1 + 6 + label.len() + 1 + data.len());
    put_u16(&mut info, N as u16);
    info.push(("tls13 ".len() + label.len()) as u8);
    info.extend_from_slice("tls13 ".as_bytes());
    info.extend_from_slice(label.as_bytes());
    info.push(data.len() as u8);
    info.extend_from_slice(data);

    hkdf::hkdf_expand(prk, &info, N).try_into().unwrap()
}

fn digest_bytes(data: &[u8]) -> [u8; 32] {
    let d = sha2::sha256(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(d.as_ref());
    out
}

// AEAD nonce = iv XOR seq (seq in BE on the rightmost 8 bytes)
fn nonce_xor(iv12: &[u8; 12], seq: u64) -> [u8; 12] {
    // seq number in big endian on rightmost 8 bytes
    let mut nonce_bytes = [[0, 0, 0, 0].as_ref(), &u64::to_be_bytes(seq)].concat();
    // xor them
    nonce_bytes.iter_mut().zip(iv12.iter()).for_each(|(s, iv)| {
        *s ^= *iv;
    });
    nonce_bytes[..12].try_into().unwrap()
}

// Very small record writer/reader after handshake
pub struct TlsStream<T: Read + Write> {
    io: T,
    // Application traffic
    aead_enc: [u8; 16],
    aead_dec: [u8; 16],
    iv_enc: [u8; 12],
    iv_dec: [u8; 12],
    seq_enc: u64,
    seq_dec: u64,
    // read buffer for decrypted application data
    rbuf: Vec<u8>,
    rpos: usize,
}

fn client_hello_body(sni_host: &str, client_pub: &[u8]) -> Vec<u8> {
    let mut ch_body = Vec::with_capacity(512);

    // X25519
    let mut random = [0u8; 32];
    syscall::getrandom(&mut random);

    let mut session_id = [0u8; 32];
    syscall::getrandom(&mut session_id);

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

    // cert signature_algorithms
    // we don't validate signatures so support can be broad
    {
        const RSA_PKCS1_SHA256: u16 = 0x0401;
        const ECDSA_SECP256R1_SHA256: u16 = 0x0403;
        const RSA_PSS_RSAE_SHA256: u16 = 0x0804;
        const ED25519: u16 = 0x0807;

        let mut sa = Vec::with_capacity(2 + 8);
        put_u16(&mut sa, 8);
        put_u16(&mut sa, ECDSA_SECP256R1_SHA256);
        put_u16(&mut sa, RSA_PSS_RSAE_SHA256);
        put_u16(&mut sa, RSA_PKCS1_SHA256);
        put_u16(&mut sa, ED25519);

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
        entry.extend_from_slice(client_pub);
        put_u16(&mut ks, entry.len() as u16);
        ks.extend_from_slice(&entry);

        put_u16(&mut exts, EXT_KEY_SHARE);
        put_u16(&mut exts, ks.len() as u16);
        exts.extend_from_slice(&ks);
    }

    // add extensions to CH
    put_u16(&mut ch_body, exts.len() as u16);
    ch_body.extend_from_slice(&exts);

    ch_body
}

/// --- Build ClientHello (single cipher: TLS_AES_128_GCM_SHA256) ---
fn client_hello_msg(sni_host: &str, client_private_key: &[u8]) -> OrtResult<Vec<u8>> {
    let client_pub_key = ecdh::x25519_public_key(client_private_key);
    let client_pub_ref = &client_pub_key;
    debug_print("Client public key", client_pub_ref);

    let ch_body = client_hello_body(sni_host, client_pub_ref);

    // Handshake framing: ClientHello
    let mut ch_msg = Vec::with_capacity(4 + ch_body.len());
    ch_msg.push(HS_CLIENT_HELLO);
    put_u24(&mut ch_msg, ch_body.len());
    ch_msg.extend_from_slice(&ch_body);

    Ok(ch_msg)
}

/// Read ServerHello (plaintext Handshake record)
fn read_server_hello<R: Read>(io: &mut R) -> OrtResult<(Vec<u8>, Vec<u8>)> {
    let (typ, payload) = read_record_plain(io).context("read_record_plain in read_server_hello")?;
    if typ != REC_TYPE_HANDSHAKE {
        return Err(ort_error(ErrorKind::TlsExpectedHandshakeRecord, ""));
    }
    let sh_buf = payload;

    // There can be multiple handshake messages; we need the ServerHello bytes specifically
    let mut rd = &sh_buf[..];
    let (sh_typ, sh_body, sh_full) =
        read_handshake_message(&mut rd).context("read_handshake_message")?;
    if sh_typ != HS_SERVER_HELLO {
        return Err(ort_error(ErrorKind::TlsExpectedServerHello, ""));
    }

    // TODO: later remove the copy. The slices are into sh_buf
    Ok((sh_body.to_vec(), sh_full.to_vec()))
}

struct HandshakeState {
    handshake_secret: [u8; 32],
    client_hs_ts: [u8; 32],
    server_hs_ts: [u8; 32],
    client_handshake_iv: [u8; 12],
    server_handshake_iv: [u8; 12],
    aead_enc_hs: [u8; 16],
    aead_dec_hs: [u8; 16],
    empty_hash: [u8; 32],
}

struct ApplicationKeys {
    aead_app_enc: [u8; 16],
    aead_app_dec: [u8; 16],
    iv_enc: [u8; 12],
    iv_dec: [u8; 12],
}

impl<T: Read + Write> TlsStream<T> {
    pub fn connect(mut io: T, sni_host: &str) -> OrtResult<Self> {
        // transcript = full Handshake message encodings (headers + bodies)
        // Feb 18 2026 full transcript is 5674 bytes
        let mut transcript = Vec::with_capacity(8192);

        // A private key is simply random bytes. /dev/urandom is cryptographically secure.
        let mut client_private_key = [0u8; 32];
        syscall::getrandom(&mut client_private_key);
        debug_print("Client private key", &client_private_key);

        debug_print("MSG -> ClientHello", &[]);
        Self::send_client_hello(&mut io, sni_host, &mut transcript, &client_private_key)?;

        debug_print("MSG <- ServerHello", &[]);
        let sh_body = Self::receive_server_hello(&mut io, &mut transcript)?;

        let handshake = Self::derive_handshake_keys(&client_private_key, &sh_body, &transcript)?;

        let mut first_encrypted_record = {
            debug_print("MSG <- ChangeCipherSpec (dummy, optional)", &[]);
            Self::skip_dummy_change_cipher_specs(&mut io)?
        };

        let mut seq_dec_hs = 0u64;
        let mut seq_enc_hs = 0u64;
        let mut is_finished: bool = false;
        while !is_finished {
            debug_print("MSG <- Server flight", &[]);
            is_finished = Self::receive_server_encrypted_flight(
                &mut io,
                &mut seq_dec_hs,
                &handshake,
                &mut transcript,
                &mut first_encrypted_record,
            )?;
        }

        let ApplicationKeys {
            aead_app_enc,
            aead_app_dec,
            iv_enc: caiv,
            iv_dec: saiv,
        } = Self::derive_application_keys(
            &handshake.handshake_secret,
            &handshake.empty_hash,
            &transcript,
        );

        let seq_app_enc = 0u64;
        let seq_app_dec = 0u64;

        // Client Change Cipher Spec
        // This is optional, to "confuse middleboxes" which expect TLS 1.2. Works without.
        //write_record_plain(&mut io, REC_TYPE_CHANGE_CIPHER_SPEC, &[0x01])?;

        debug_print("MSG -> ClientFinished", &[]);
        Self::send_client_finished(&mut io, &handshake, &mut transcript, &mut seq_enc_hs)?;

        debug_print("TLS connect done", &[]);
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

    pub fn has_buffered_data(&self) -> bool {
        /*
        let out = self.rpos < self.rbuf.len();
        let msg = alloc::string::ToString::to_string(&"rpos = ")
            + &utils::num_to_string(self.rpos)
            + ", rbuf.len() = "
            + &utils::num_to_string(self.rbuf.len())
            + " . "
            + if out { "true" } else { "false" };
        utils::print_string(c"tls has_buffered_data: ", &msg);
        */
        self.rpos < self.rbuf.len()
    }

    fn send_client_hello<W: Write>(
        io: &mut W,
        sni_host: &str,
        transcript: &mut Vec<u8>,
        client_private_key: &[u8; 32],
    ) -> OrtResult<()> {
        let ch_msg = client_hello_msg(sni_host, client_private_key)?;
        write_record_plain(io, REC_TYPE_HANDSHAKE, &ch_msg).context("write ClientHello")?;
        transcript.extend_from_slice(&ch_msg);
        Ok(())
    }

    fn receive_server_hello<R: Read>(io: &mut R, transcript: &mut Vec<u8>) -> OrtResult<Vec<u8>> {
        let (sh_body, sh_full) = read_server_hello(io)?;
        transcript.extend_from_slice(&sh_full);
        Ok(sh_body)
    }

    fn skip_dummy_change_cipher_specs<R: Read>(io: &mut R) -> OrtResult<Option<Record>> {
        // Some servers send TLS 1.2-style ChangeCipherSpec for middlebox compatibility.
        // TLS 1.3 peers are also allowed to omit this entirely, so stop at the
        // first non-CCS record and hand it back to the encrypted-flight reader.
        loop {
            let record =
                read_record_raw(io).context("read_record_raw for optional dummy change cipher")?;
            if record.typ != REC_TYPE_CHANGE_CIPHER_SPEC {
                return Ok(Some(record));
            }
            if record.body != [0x01] {
                return Err(ort_error(ErrorKind::TlsExpectedChangeCipherSpec, ""));
            }
        }
    }

    /// Should be called multiple times until it returns true.
    /// The TLS messages for this stage might come as separate packets, or all in one.
    fn receive_server_encrypted_flight<R: Read>(
        io: &mut R,
        seq_dec_hs: &mut u64,
        handshake: &HandshakeState,
        transcript: &mut Vec<u8>,
        first_record: &mut Option<Record>,
    ) -> OrtResult<bool> {
        let (typ, ct, _inner_type) = if let Some(record) = first_record.take() {
            read_record_cipher_from_record(
                record,
                &handshake.aead_dec_hs,
                &handshake.server_handshake_iv,
                seq_dec_hs,
            )?
        } else {
            read_record_cipher(
                io,
                &handshake.aead_dec_hs,
                &handshake.server_handshake_iv,
                seq_dec_hs,
            )?
        };
        if typ != REC_TYPE_APPDATA {
            return Err(ort_error(ErrorKind::TlsExpectedEncryptedRecords, ""));
        }

        // Decrypted TLSInnerPlaintext: ... | content_type
        // May contain multiple handshake messages; parse & append to transcript.
        let mut p = &ct[..];
        while !p.is_empty() {
            let (mtyp, body, full) = match read_handshake_message(&mut p) {
                Ok(x) => x,
                Err(err) => {
                    crate::utils::print_string(c"read_handshake_message error: ", &err.as_string());
                    return Err(ort_error(ErrorKind::TlsBadHandshakeFragment, ""));
                }
            };
            transcript.extend_from_slice(full);
            debug_print("handshake message (type is first byte)", full);

            if mtyp == HS_FINISHED {
                // verify server Finished
                let s_finished_key =
                    hkdf_expand_label::<32>(&handshake.server_hs_ts, "finished", &[]);

                let thash = digest_bytes(&transcript[..transcript.len() - full.len()]);
                let expected = hmac::sign(&s_finished_key, &thash);
                if expected.as_slice() != body {
                    return Err(ort_error(ErrorKind::TlsFinishedVerifyFailed, ""));
                }
                // Done collecting server handshake.
                return Ok(true);
            }
            // Ignore other handshake types’ contents (no cert validation).
        }
        Ok(false)
    }

    fn derive_handshake_keys(
        client_private_key: &[u8; 32],
        sh_body: &[u8],
        transcript: &[u8],
    ) -> OrtResult<HandshakeState> {
        // Parse minimal ServerHello to get cipher & key_share
        let (cipher, server_public_key_bytes) = parse_server_hello_for_keys(sh_body)?;
        debug_print("Server public key", &server_public_key_bytes);
        if cipher != CIPHER_TLS_AES_128_GCM_SHA256 {
            return Err(ort_error(
                ErrorKind::TlsUnsupportedCipher,
                "server picked unsupported cipher",
            ));
        }

        // ECDH(X25519) shared secret
        let hs_shared_secret = ecdh::x25519_agreement(client_private_key, &server_public_key_bytes);
        debug_print("hs shared secret", &hs_shared_secret);

        // Same as: `echo -n "" | openssl sha256`
        let empty_hash = digest_bytes(&[]);
        debug_print("empty_hash", &empty_hash);

        let zero: [u8; 32] = [0u8; 32];
        let early_secret = hkdf::hkdf_extract(&zero, &zero);

        let derived_secret_bytes = hkdf_expand_label::<32>(&early_secret, "derived", &empty_hash);
        debug_print("derived", &derived_secret_bytes);

        let handshake_secret = hkdf::hkdf_extract(&derived_secret_bytes, &hs_shared_secret);
        debug_print("handshake_secret", &handshake_secret);

        let ch_sh_hash = digest_bytes(transcript);
        debug_print("digest bytes", &ch_sh_hash);

        let c_hs_ts = hkdf_expand_label(&handshake_secret, "c hs traffic", &ch_sh_hash);
        let s_hs_ts = hkdf_expand_label(&handshake_secret, "s hs traffic", &ch_sh_hash);

        debug_print("c hs traffic", &c_hs_ts);
        debug_print("s hs traffic", &s_hs_ts);

        // handshake AEAD keys/IVs
        let client_handshake_key: [u8; 16] = hkdf_expand_label::<16>(&c_hs_ts, "key", &[])
            .as_slice()[..16]
            .try_into()
            .unwrap();
        debug_print("client_handshake_key", &client_handshake_key);
        let client_handshake_iv: [u8; 12] = hkdf_expand_label::<12>(&c_hs_ts, "iv", &[]).as_slice()
            [..12]
            .try_into()
            .unwrap();
        debug_print("client_handshake_iv", &client_handshake_iv);

        let server_handshake_key: [u8; 16] = hkdf_expand_label::<16>(&s_hs_ts, "key", &[])
            .as_slice()[..16]
            .try_into()
            .unwrap();
        debug_print("server_handshake_key", &server_handshake_key);
        let server_handshake_iv: [u8; 12] = hkdf_expand_label::<12>(&s_hs_ts, "iv", &[]).as_slice()
            [..12]
            .try_into()
            .unwrap();
        debug_print("server_handshake_iv", &server_handshake_iv);

        Ok(HandshakeState {
            handshake_secret,
            client_hs_ts: c_hs_ts,
            server_hs_ts: s_hs_ts,
            client_handshake_iv,
            server_handshake_iv,
            aead_enc_hs: client_handshake_key,
            aead_dec_hs: server_handshake_key,
            empty_hash,
        })
    }

    fn derive_application_keys(
        handshake_secret: &[u8; 32],
        empty_hash: &[u8; 32],
        transcript: &[u8],
    ) -> ApplicationKeys {
        let derived2_bytes = hkdf_expand_label::<32>(handshake_secret, "derived", empty_hash);
        debug_print("derived2_bytes", &derived2_bytes);

        let zero: [u8; 32] = [0u8; 32];
        let master_secret = hkdf::hkdf_extract(&derived2_bytes, &zero);
        let thash_srv_fin = digest_bytes(transcript);

        let c_ap_ts = hkdf_expand_label::<32>(&master_secret, "c ap traffic", &thash_srv_fin);
        let s_ap_ts = hkdf_expand_label::<32>(&master_secret, "s ap traffic", &thash_srv_fin);
        debug_print("c_ap_ts", &c_ap_ts);
        debug_print("s_ap_ts", &s_ap_ts);

        let cak: [u8; 16] = hkdf_expand_label::<16>(&c_ap_ts, "key", &[]).as_slice()[..16]
            .try_into()
            .unwrap();
        let caiv: [u8; 12] = hkdf_expand_label::<12>(&c_ap_ts, "iv", &[]).as_slice()[..12]
            .try_into()
            .unwrap();
        debug_print("cak", &cak);
        debug_print("caiv", &caiv);

        let sak: [u8; 16] = hkdf_expand_label::<16>(&s_ap_ts, "key", &[]).as_slice()[..16]
            .try_into()
            .unwrap();
        let saiv: [u8; 12] = hkdf_expand_label::<12>(&s_ap_ts, "iv", &[]).as_slice()[..12]
            .try_into()
            .unwrap();
        debug_print("sak", &sak);
        debug_print("saiv", &saiv);

        ApplicationKeys {
            aead_app_enc: cak,
            aead_app_dec: sak,
            iv_enc: caiv,
            iv_dec: saiv,
        }
    }

    fn send_client_finished<W: Write>(
        io: &mut W,
        handshake: &HandshakeState,
        transcript: &mut Vec<u8>,
        seq_enc_hs: &mut u64,
    ) -> OrtResult<()> {
        let c_finished_key = hkdf_expand_label::<32>(&handshake.client_hs_ts, "finished", &[]);
        debug_print("c_finished", &c_finished_key);

        let thash_client_fin = digest_bytes(transcript.as_slice());
        let verify_data = hmac::sign(&c_finished_key, &thash_client_fin);
        debug_print("verify_data", &verify_data);

        let mut fin = Vec::with_capacity(4 + verify_data.as_ref().len());
        fin.push(HS_FINISHED);
        put_u24(&mut fin, verify_data.as_ref().len());
        fin.extend_from_slice(verify_data.as_ref());

        // append to transcript before switching keys
        transcript.extend_from_slice(&fin);

        write_record_cipher(
            io,
            REC_TYPE_HANDSHAKE,
            &fin,
            &handshake.aead_enc_hs,
            &handshake.client_handshake_iv,
            seq_enc_hs,
        )
        .context("write_record_cipher write_all failed")?;

        Ok(())
    }
}

impl<T: Read + Write> Write for TlsStream<T> {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let mut bytes_sent = 0;
        for chunk in buf.chunks(MAX_PLAINTEXT_SIZE) {
            write_record_cipher(
                &mut self.io,
                REC_TYPE_APPDATA,
                chunk,
                &self.aead_enc,
                &self.iv_enc,
                &mut self.seq_enc,
            )?;
            bytes_sent += chunk.len();
        }
        Ok(bytes_sent)
    }
    fn flush(&mut self) -> OrtResult<()> {
        self.io.flush()
    }
}

impl<T: Read + Write> Read for TlsStream<T> {
    fn read(&mut self, out: &mut [u8]) -> OrtResult<usize> {
        if self.rpos < self.rbuf.len() {
            debug_print("TlsStream.read using buf", &[]);

            let n = cmp::min(out.len(), self.rbuf.len() - self.rpos);
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
                let err_level = CString::new(level.to_string() + " alert: ").unwrap();

                // See https://www.rfc-editor.org/rfc/rfc8446#appendix-B search for
                // "unexpected_message" for all types
                let mut err_code_buf: [u8; 5] = [0u8; 5];
                let len = to_ascii(plaintext[1] as usize, &mut err_code_buf);
                let err_code = unsafe { CStr::from_bytes_with_nul_unchecked(&err_code_buf[..len]) };
                syscall::write(2, err_level.as_ptr().cast(), err_level.count_bytes());
                syscall::write(2, err_code.as_ptr().cast(), err_code.count_bytes());

                return Err(ort_error(ErrorKind::TlsAlertReceived, ""));
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
            let n = cmp::min(out.len(), self.rbuf.len());
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

impl<T: Read + Write + AsFd> AsFd for TlsStream<T> {
    fn as_fd(&self) -> i32 {
        self.io.as_fd()
    }
}

// ---------------------- Record I/O helpers ----------------------------------

struct Record {
    hdr: [u8; 5],
    typ: u8,
    body: Vec<u8>,
}

fn write_record_plain<W: Write>(w: &mut W, typ: u8, body: &[u8]) -> OrtResult<()> {
    let mut hdr = [0u8; 5];
    hdr[0] = typ;
    hdr[1..3].copy_from_slice(&LEGACY_REC_VER.to_be_bytes());
    hdr[3..5].copy_from_slice(&(body.len() as u16).to_be_bytes());
    w.write_all(&hdr)?;
    w.write_all(body)?;
    Ok(())
}

fn read_exact_n<R: Read>(r: &mut R, n: usize) -> OrtResult<Vec<u8>> {
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

fn read_record_plain<R: Read>(r: &mut R) -> OrtResult<(u8, Vec<u8>)> {
    let record = read_record_raw(r)?;
    //let _ = write_bytes_to_file(&[&hdr[..], &body].concat(), debug_filename);
    Ok((record.typ, record.body))
}

fn read_record_raw<R: Read>(r: &mut R) -> OrtResult<Record> {
    let mut hdr = [0u8; 5]; // Record Header, e.g. 16 03 03 len
    r.read_exact(&mut hdr)?;
    let typ = hdr[0];
    let len = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
    let body = read_exact_n(r, len)?;
    debug_print("read_record_plain hdr", &hdr);
    debug_print("read_record_plain body", &body);
    Ok(Record { hdr, typ, body })
}

fn write_record_cipher<W: Write>(
    w: &mut W,
    outer_type: u8,
    inner: &[u8],
    key: &[u8; 16],
    iv12: &[u8; 12],
    seq: &mut u64,
) -> OrtResult<()> {
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

    let out = aead::aes_128_gcm_encrypt(key, &nonce, &hdr, &plain).unwrap();

    debug_print("write_record_cipher header", &hdr);
    //let final_label = format!("write_record_cipher final {total_len}");
    //debug_print(final_label.as_str(), &out);

    w.write_all(&hdr)?;
    w.write_all(&out)?;
    Ok(())
}

fn read_record_cipher<R: Read>(
    r: &mut R,
    key: &[u8; 16],
    iv12: &[u8; 12],
    seq: &mut u64,
) -> OrtResult<(u8, Vec<u8>, u8)> {
    let record = read_record_raw(r)?;
    read_record_cipher_from_record(record, key, iv12, seq)
}

fn read_record_cipher_from_record(
    record: Record,
    key: &[u8; 16],
    iv12: &[u8; 12],
    seq: &mut u64,
) -> OrtResult<(u8, Vec<u8>, u8)> {
    let hdr = record.hdr;
    let typ = record.typ;
    let ciphertext = record.body;
    if ciphertext.len() < AEAD_TAG_LEN {
        return Err(ort_error(ErrorKind::TlsRecordTooShort, "short record"));
    }
    debug_print("read_record_cipher hdr", &hdr);
    debug_print("read_record_cipher ct", &ciphertext);

    //let size_expected = crate::utils::num_to_string(len);
    //let size_read = crate::utils::num_to_string(ciphertext.len());
    //crate::utils::print_string(c"size_expected ", &size_expected);
    //crate::utils::print_string(c"size_read ", &size_read);

    // Decrypt ciphertext

    let nonce = nonce_xor(iv12, *seq);
    *seq = seq.wrapping_add(1);

    let mut out = match aead::aes_128_gcm_decrypt(key, &nonce, &hdr, &ciphertext) {
        Ok(out) => out,
        Err(s) => {
            return Err(ort_error(ErrorKind::TlsAes128GcmDecryptFailed, s));
        }
    };

    debug_print("read_record_cipher plaintext hdr", &hdr);
    debug_print("read_record_cipher plaintext", &out);

    let inner_type = strip_tls_inner_plaintext(&mut out);
    Ok((typ, out, inner_type))
}

// The inner_type should be the last byte of the packet, but padding is allowed.
fn strip_tls_inner_plaintext(out: &mut Vec<u8>) -> u8 {
    // Skip padding (0) bytes backwards from the end to find inner_type
    let Some(inner_pos) = out.iter().rposition(|b| *b != 0) else {
        // We didn't find any non-0 bytes
        out.clear();
        return 0;
    };
    let inner_type = out[inner_pos];
    out.truncate(inner_pos);
    inner_type
}

// ---------------------- Handshake parsing helpers ---------------------------

fn read_handshake_message<'a>(rd: &mut &'a [u8]) -> OrtResult<(u8, &'a [u8], &'a [u8])> {
    if rd.len() < 4 {
        return Err(ort_error(ErrorKind::TlsHandshakeHeaderTooShort, ""));
    }
    let typ = rd[0];
    let len = ((rd[1] as usize) << 16) | ((rd[2] as usize) << 8) | rd[3] as usize;
    if rd.len() < 4 + len {
        return Err(ort_error(ErrorKind::TlsHandshakeBodyTooShort, ""));
    }
    let full = &rd[..4 + len];
    let body = &rd[4..4 + len];
    *rd = &rd[4 + len..];
    Ok((typ, body, full))
}

fn parse_server_hello_for_keys(sh: &[u8]) -> OrtResult<(u16, [u8; 32])> {
    // minimal parse: skip legacy_version(2), random(32), sid, cipher(2), comp(1), exts
    if sh.len() < 2 + 32 + 1 + 2 + 1 + 2 {
        return Err(ort_error(ErrorKind::TlsServerHelloTooShort, ""));
    }
    let mut p = sh;

    p = &p[2..]; // legacy_version
    p = &p[32..]; // random
    let sid_len = p[0] as usize;
    p = &p[1..];
    if p.len() < sid_len + 2 + 1 + 2 {
        return Err(ort_error(ErrorKind::TlsServerHelloSessionIdInvalid, ""));
    }
    p = &p[sid_len..];
    let cipher = u16::from_be_bytes([p[0], p[1]]);
    p = &p[2..];
    let _comp = p[0];
    p = &p[1..];
    let ext_len = u16::from_be_bytes([p[0], p[1]]) as usize;
    p = &p[2..];
    if p.len() < ext_len {
        return Err(ort_error(ErrorKind::TlsServerHelloExtTooShort, ""));
    }
    let mut ex = &p[..ext_len];

    let mut server_pub = None;

    while !ex.is_empty() {
        if ex.len() < 4 {
            return Err(ort_error(ErrorKind::TlsExtensionHeaderTooShort, ""));
        }
        let et = u16::from_be_bytes([ex[0], ex[1]]);
        let el = u16::from_be_bytes([ex[2], ex[3]]) as usize;
        ex = &ex[4..];
        if ex.len() < el {
            return Err(ort_error(ErrorKind::TlsExtensionLengthInvalid, ""));
        }
        let ed = &ex[..el];
        ex = &ex[el..];

        match et {
            EXT_KEY_SHARE => {
                // KeyShareServerHello: group(2) kx_len(2) kx
                if ed.len() < 2 + 2 + 32 {
                    return Err(ort_error(ErrorKind::TlsKeyShareServerHelloInvalid, ""));
                }
                let grp = u16::from_be_bytes([ed[0], ed[1]]);
                if grp != GROUP_X25519 {
                    return Err(ort_error(
                        ErrorKind::TlsServerGroupUnsupported,
                        "server group != x25519",
                    ));
                }
                let kx_len = u16::from_be_bytes([ed[2], ed[3]]) as usize;
                if ed.len() < 4 + kx_len || kx_len != 32 {
                    return Err(ort_error(ErrorKind::TlsKeyShareLengthInvalid, ""));
                }
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&ed[4..4 + 32]);
                server_pub = Some(pk);
            }
            EXT_SUPPORTED_VERSIONS
                if (ed.len() != 2 || u16::from_be_bytes([ed[0], ed[1]]) != TLS13) =>
            {
                return Err(ort_error(ErrorKind::TlsServerNotTls13, ""));
            }
            _ => {}
        }
    }

    let sp = server_pub.ok_or_else(|| ort_error(ErrorKind::TlsMissingServerKey, ""))?;
    Ok((cipher, sp))
}

#[allow(unused)]
fn debug_print(name: &str, value: &[u8]) {
    #[cfg(debug_assertions)]
    {
        if !DEBUG_LOG {
            return;
        }
        let c_str = CString::new(name).unwrap();
        if !value.is_empty() {
            crate::utils::print_hex(c_str.as_c_str(), value);
        } else {
            crate::utils::print_string(c_str.as_c_str(), "");
        }
    }
}

/*
#[allow(dead_code)]
fn write_bytes_to_file(bytes: &[u8], file_path: &str) -> std::io::Result<()> {
    let mut file = File::create(file_path)?;
    file.write_all(bytes)?;
    Ok(())
}
*/

#[cfg(test)]
pub mod tests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;

    struct TestIo {
        bytes: Vec<u8>,
        pos: usize,
    }

    impl TestIo {
        fn new(bytes: Vec<u8>) -> Self {
            Self { bytes, pos: 0 }
        }
    }

    impl Read for TestIo {
        fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
            let remaining = &self.bytes[self.pos..];
            let len = cmp::min(buf.len(), remaining.len());
            buf[..len].copy_from_slice(&remaining[..len]);
            self.pos += len;
            Ok(len)
        }
    }

    impl Write for TestIo {
        fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> OrtResult<()> {
            Ok(())
        }
    }

    fn plain_record(typ: u8, body: &[u8]) -> Vec<u8> {
        let mut record = Vec::new();
        record.push(typ);
        record.extend_from_slice(&LEGACY_REC_VER.to_be_bytes());
        record.extend_from_slice(&(body.len() as u16).to_be_bytes());
        record.extend_from_slice(body);
        record
    }

    pub fn string_to_bytes(s: &str) -> [u8; 32] {
        let mut bytes = s.as_bytes();
        if bytes.len() >= 2 && bytes[0] == b'0' && (bytes[1] == b'x' || bytes[1] == b'X') {
            bytes = &bytes[2..];
        }
        assert!(
            bytes.len() == 64,
            "hex string must be exactly 64 hex chars (32 bytes)"
        );

        let mut out = [0u8; 32];
        for i in 0..32 {
            let hi = hex_val(bytes[2 * i]);
            let lo = hex_val(bytes[2 * i + 1]);
            out[i] = (hi << 4) | lo;
        }
        out
    }

    pub fn hex_to_vec(s: &str) -> Vec<u8> {
        let mut bytes = s.as_bytes();
        if bytes.len() >= 2 && bytes[0] == b'0' && (bytes[1] == b'X' || bytes[1] == b'x') {
            bytes = &bytes[2..];
        }
        assert_eq!(bytes.len() % 2, 0, "hex string must have even length");
        let mut out = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            let hi = hex_val(chunk[0]);
            let lo = hex_val(chunk[1]);
            out.push((hi << 4) | lo);
        }
        out
    }

    fn hex_val(b: u8) -> u8 {
        match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => panic!("invalid hex character"),
        }
    }

    #[test]
    fn client_hello_advertises_ed25519_signature_algorithm() {
        let client_pub = [7u8; 32];
        let body = client_hello_body("localhost", &client_pub);
        let ed25519_sig_alg_ext = [0x00, 0x0d, 0x00, 0x0a, 0x00, 0x08, 0x08, 0x07];

        assert!(
            body.windows(ed25519_sig_alg_ext.len())
                .any(|window| window == ed25519_sig_alg_ext)
        );
    }

    #[test]
    fn skip_dummy_change_cipher_specs_accepts_no_ccs() {
        let appdata = plain_record(REC_TYPE_APPDATA, &[1, 2, 3]);
        let mut io = TestIo::new(appdata);

        let record = TlsStream::<TestIo>::skip_dummy_change_cipher_specs(&mut io)
            .unwrap()
            .unwrap();

        assert_eq!(record.typ, REC_TYPE_APPDATA);
        assert_eq!(record.body, alloc::vec![1, 2, 3]);
    }

    #[test]
    fn skip_dummy_change_cipher_specs_skips_multiple_valid_ccs_records() {
        let mut bytes = plain_record(REC_TYPE_CHANGE_CIPHER_SPEC, &[0x01]);
        bytes.extend_from_slice(&plain_record(REC_TYPE_CHANGE_CIPHER_SPEC, &[0x01]));
        bytes.extend_from_slice(&plain_record(REC_TYPE_APPDATA, &[4, 5, 6]));
        let mut io = TestIo::new(bytes);

        let record = TlsStream::<TestIo>::skip_dummy_change_cipher_specs(&mut io)
            .unwrap()
            .unwrap();

        assert_eq!(record.typ, REC_TYPE_APPDATA);
        assert_eq!(record.body, alloc::vec![4, 5, 6]);
    }

    #[test]
    fn skip_dummy_change_cipher_specs_rejects_invalid_ccs_record() {
        let mut io = TestIo::new(plain_record(REC_TYPE_CHANGE_CIPHER_SPEC, &[0x02]));

        match TlsStream::<TestIo>::skip_dummy_change_cipher_specs(&mut io) {
            Ok(_) => panic!("invalid ChangeCipherSpec should fail"),
            Err(err) => assert!(matches!(err.kind, ErrorKind::TlsExpectedChangeCipherSpec)),
        }
    }

    #[test]
    fn strip_tls_inner_plaintext_removes_padding() {
        let mut plaintext = alloc::vec![1, 2, 3, REC_TYPE_HANDSHAKE, 0, 0];

        let inner_type = strip_tls_inner_plaintext(&mut plaintext);

        assert_eq!(inner_type, REC_TYPE_HANDSHAKE);
        assert_eq!(plaintext, alloc::vec![1, 2, 3]);
    }
}
