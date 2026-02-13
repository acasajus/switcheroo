use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use rand::RngCore;
use rsa::pkcs8::DecodePublicKey;
use rsa::{Oaep, RsaPublicKey, sha2::Sha256};
use std::io::Write;

const TINFOIL_PUBLIC_KEY: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAvPdrJigQ0rZAy+jla7hS
jwen8gkF0gjtl+lZGY59KatNd9Kj2gfY7dTMM+5M2tU4Wr3nk8KWr5qKm3hzo/2C
Gbc55im3tlRl6yuFxWQ+c/I2SM5L3xp6eiLUcumMsEo0B7ELmtnHTGCCNAIzTFzV
4XcWGVbkZj83rTFxpLsa1oArTdcz5CG6qgyVe7KbPsft76DAEkV8KaWgnQiG0Dps
INFy4vISmf6L1TgAryJ8l2K4y8QbymyLeMsABdlEI3yRHAm78PSezU57XtQpHW5I
aupup8Es6bcDZQKkRsbOeR9T74tkj+k44QrjZo8xpX9tlJAKEEmwDlyAg0O5CLX3
CQIDAQAB
-----END PUBLIC KEY-----"#;

pub fn encrypt_shop(json_data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // 1. Generate random 128-bit AES key
    let mut aes_key = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut aes_key);

    // 2. Zstd compress input
    let compressed = zstd::encode_all(json_data, 22)?;
    let compressed_size = compressed.len();

    // 3. Encrypt AES key with RSA (Tinfoil public key)
    let pub_key = RsaPublicKey::from_public_key_pem(TINFOIL_PUBLIC_KEY)?;
    let mut rng = rand::thread_rng();
    let padding = Oaep::new::<Sha256>();
    let encrypted_aes_key = pub_key.encrypt(&mut rng, padding, &aes_key)?;

    // 4. Encrypt compressed data with AES-ECB (yes, Tinfoil uses ECB)
    let cipher = Aes128::new(GenericArray::from_slice(&aes_key));

    // Pad to block size
    let mut padded_data = compressed.clone();
    let pad_len = 16 - (compressed_size % 16);
    if pad_len < 16 {
        padded_data.extend(vec![0u8; pad_len]);
    }

    let mut encrypted_data = Vec::with_capacity(padded_data.len());
    for chunk in padded_data.chunks_exact(16) {
        let mut block = *GenericArray::from_slice(chunk);
        cipher.encrypt_block(&mut block);
        encrypted_data.extend_from_slice(&block);
    }

    // 5. Construct binary format
    let mut output = Vec::new();
    output.write_all(b"TINFOIL")?;
    output.write_all(&[0xFD])?; // flag
    output.write_all(&encrypted_aes_key)?;
    output.write_all(&(compressed_size as u64).to_le_bytes())?;
    output.write_all(&encrypted_data)?;

    Ok(output)
}

#[cfg(test)]

mod tests {

    use super::*;

    #[test]

    fn test_encrypt_shop() {
        let data = b"{\"files\": []}";

        let result = encrypt_shop(data);

        assert!(result.is_ok());

        let encrypted = result.unwrap();

        assert!(encrypted.starts_with(b"TINFOIL"));

        assert!(encrypted.len() > 7 + 1 + 256 + 8); // Header + flag + RSA + size
    }
}
