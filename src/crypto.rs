//! Cryptographic primitives used by the UU cloud and the Reyee router.

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::{Aes128, Aes256};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use md5::{Digest, Md5};

pub fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub fn md5_hex(data: &[u8]) -> String {
    let mut h = Md5::new();
    h.update(data);
    hex_lower(&h.finalize())
}

/// AES-128/ECB/PKCS7 encrypt (used for UU `/app/*` request bodies).
pub fn aes_ecb_encrypt(key: &[u8], plain: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(&key[..16]).expect("key");
    let mut data = plain.to_vec();
    let pad = 16 - (data.len() % 16);
    data.extend(std::iter::repeat(pad as u8).take(pad));
    for chunk in data.chunks_mut(16) {
        let block = aes::cipher::generic_array::GenericArray::from_mut_slice(chunk);
        cipher.encrypt_block(block);
    }
    data
}

pub fn aes_ecb_decrypt(key: &[u8], cipher_text: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(&key[..16]).expect("key");
    let mut data = cipher_text.to_vec();
    for chunk in data.chunks_mut(16) {
        let block = aes::cipher::generic_array::GenericArray::from_mut_slice(chunk);
        cipher.decrypt_block(block);
    }
    if let Some(&pad) = data.last() {
        let pad = pad as usize;
        if pad >= 1 && pad <= 16 && pad <= data.len() {
            data.truncate(data.len() - pad);
        }
    }
    data
}

pub fn aes_ecb_encrypt_b64(key: &[u8], plain: &str) -> String {
    B64.encode(aes_ecb_encrypt(key, plain.as_bytes()))
}

pub fn aes_ecb_decrypt_b64(key: &[u8], b64: &str) -> Option<String> {
    let raw = B64.decode(b64.trim()).ok()?;
    String::from_utf8(aes_ecb_decrypt(key, &raw)).ok()
}

/// OpenSSL EVP_BytesToKey (MD5, one round, no IV in the digest chain beyond key+iv).
fn evp_bytes_to_key(pass: &[u8], salt: &[u8], key_len: usize, iv_len: usize) -> (Vec<u8>, Vec<u8>) {
    let mut out: Vec<u8> = Vec::new();
    let mut prev: Vec<u8> = Vec::new();
    while out.len() < key_len + iv_len {
        let mut h = Md5::new();
        h.update(&prev);
        h.update(pass);
        h.update(salt);
        prev = h.finalize().to_vec();
        out.extend_from_slice(&prev);
    }
    (out[..key_len].to_vec(), out[key_len..key_len + iv_len].to_vec())
}

/// Reyee "GibberishAES": AES-256-CBC, PKCS7, prefixed with `Salted__` + 8-byte salt,
/// then Base64. Matches the router web UI's `pwd` field.
pub fn gibberish_encrypt(plain: &str, passphrase: &str, salt: &[u8; 8]) -> String {
    let (key, iv) = evp_bytes_to_key(passphrase.as_bytes(), salt, 32, 16);
    let mut buf = vec![0u8; plain.len() + 16];
    buf[..plain.len()].copy_from_slice(plain.as_bytes());
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    let key_ga = aes::cipher::generic_array::GenericArray::from_slice(&key);
    let iv_ga = aes::cipher::generic_array::GenericArray::from_slice(&iv);
    let enc = cbc::Encryptor::<Aes256>::new(key_ga, iv_ga);
    let ct = enc
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plain.len())
        .expect("encrypt");
    let mut out = Vec::with_capacity(16 + ct.len());
    out.extend_from_slice(b"Salted__");
    out.extend_from_slice(salt);
    out.extend_from_slice(ct);
    B64.encode(out)
}

/// Current unix time in seconds as a string.
pub fn unix_secs() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

pub fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
