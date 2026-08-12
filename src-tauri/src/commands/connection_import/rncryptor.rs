//! RNCryptor v3 password-based decrypt (TablePlus `.tableplusconnection`).

use super::super::error::CommandError;
use aes::Aes256;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
use sha2::Sha256;

type Aes256CbcDec = cbc::Decryptor<Aes256>;
type HmacSha256 = Hmac<Sha256>;

const PBKDF2_ITERS: u32 = 10_000;

pub fn decrypt_password(data: &[u8], password: &str) -> Result<Vec<u8>, CommandError> {
    if password.is_empty() {
        return Err(CommandError::Validation(
            "Password is required for TablePlus import".into(),
        ));
    }
    if data.len() < 66 {
        return Err(CommandError::Validation(
            "Invalid TablePlus file: too short".into(),
        ));
    }
    if data[0] != 0x03 {
        return Err(CommandError::Validation(
            "Invalid TablePlus file: unsupported RNCryptor version".into(),
        ));
    }
    if data[1] != 0x01 {
        return Err(CommandError::Validation(
            "Invalid TablePlus file: expected password-based encryption".into(),
        ));
    }

    let enc_salt = &data[2..10];
    let hmac_salt = &data[10..18];
    let iv = &data[18..34];
    let hmac = &data[data.len() - 32..];
    let ciphertext = &data[34..data.len() - 32];
    let header_and_cipher = &data[..data.len() - 32];

    let mut enc_key = [0u8; 32];
    let mut hmac_key = [0u8; 32];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), enc_salt, PBKDF2_ITERS, &mut enc_key);
    pbkdf2_hmac::<Sha1>(password.as_bytes(), hmac_salt, PBKDF2_ITERS, &mut hmac_key);

    let mut mac =
        HmacSha256::new_from_slice(&hmac_key).map_err(|e| CommandError::Internal(e.to_string()))?;
    mac.update(header_and_cipher);
    mac.verify_slice(hmac).map_err(|_| {
        CommandError::Validation(
            "TablePlus decryption failed: wrong password or corrupt file".into(),
        )
    })?;

    let mut buf = ciphertext.to_vec();
    let plain = Aes256CbcDec::new_from_slices(&enc_key, iv)
        .map_err(|e| CommandError::Internal(format!("AES init failed: {e}")))?
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| {
            CommandError::Validation(
                "TablePlus decryption failed: wrong password or corrupt file".into(),
            )
        })?;
    Ok(plain.to_vec())
}

/// Encrypt for unit tests / fixtures.
#[cfg(test)]
pub fn encrypt_password(plaintext: &[u8], password: &str) -> Result<Vec<u8>, CommandError> {
    use cbc::cipher::BlockEncryptMut;
    use rand::RngCore;

    type Aes256CbcEnc = cbc::Encryptor<Aes256>;

    if password.is_empty() {
        return Err(CommandError::Validation("Password is required".into()));
    }

    let mut enc_salt = [0u8; 8];
    let mut hmac_salt = [0u8; 8];
    let mut iv = [0u8; 16];
    let mut rng = rand::thread_rng();
    rng.fill_bytes(&mut enc_salt);
    rng.fill_bytes(&mut hmac_salt);
    rng.fill_bytes(&mut iv);

    let mut enc_key = [0u8; 32];
    let mut hmac_key = [0u8; 32];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), &enc_salt, PBKDF2_ITERS, &mut enc_key);
    pbkdf2_hmac::<Sha1>(password.as_bytes(), &hmac_salt, PBKDF2_ITERS, &mut hmac_key);

    // PKCS7 may expand by up to one block.
    let mut buf = plaintext.to_vec();
    let pad_len = 16 - (buf.len() % 16);
    buf.extend(std::iter::repeat(0u8).take(pad_len));
    let cipher_len = Aes256CbcEnc::new_from_slices(&enc_key, &iv)
        .map_err(|e| CommandError::Internal(format!("AES init failed: {e}")))?
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
        .map_err(|e| CommandError::Internal(format!("AES encrypt failed: {e}")))?
        .len();
    buf.truncate(cipher_len);

    let mut out = Vec::with_capacity(34 + buf.len() + 32);
    out.push(0x03);
    out.push(0x01);
    out.extend_from_slice(&enc_salt);
    out.extend_from_slice(&hmac_salt);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&buf);

    let mut mac =
        HmacSha256::new_from_slice(&hmac_key).map_err(|e| CommandError::Internal(e.to_string()))?;
    mac.update(&out);
    out.extend_from_slice(&mac.finalize().into_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let plain = br#"[{"ConnectionName":"demo","Driver":"PostgreSQL"}]"#;
        let enc = encrypt_password(plain, "secret").unwrap();
        assert_eq!(&enc[0..2], &[0x03, 0x01]);
        let dec = decrypt_password(&enc, "secret").unwrap();
        assert_eq!(dec, plain);
    }

    #[test]
    fn wrong_password_fails() {
        let enc = encrypt_password(b"hello", "right").unwrap();
        assert!(decrypt_password(&enc, "wrong").is_err());
    }
}
