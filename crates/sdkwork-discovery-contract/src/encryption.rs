use std::fmt;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionConfig {
    pub enabled: bool,
    pub algorithm: EncryptionAlgorithm,
    pub key_file: Option<String>,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            key_file: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedValue {
    pub algorithm: EncryptionAlgorithm,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl EncryptedValue {
    pub fn to_storage_string(&self) -> String {
        let nonce_b64 = STANDARD.encode(&self.nonce);
        let ciphertext_b64 = STANDARD.encode(&self.ciphertext);
        format!("encrypted:aes-256-gcm:{nonce_b64}:{ciphertext_b64}")
    }

    pub fn from_storage_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(4, ':').collect();
        if parts.len() == 4 && parts[0] == "encrypted" && parts[1] == "aes-256-gcm" {
            let nonce = STANDARD.decode(parts[2]).ok()?;
            let ciphertext = STANDARD.decode(parts[3]).ok()?;
            Some(Self {
                algorithm: EncryptionAlgorithm::Aes256Gcm,
                nonce,
                ciphertext,
            })
        } else {
            None
        }
    }

    pub fn is_encrypted(s: &str) -> bool {
        s.starts_with("encrypted:aes-256-gcm:")
    }
}

impl fmt::Display for EncryptedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_storage_string())
    }
}

pub trait ConfigEncryptor: Send + Sync {
    fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedValue, String>;
    fn decrypt(&self, encrypted: &EncryptedValue) -> Result<Vec<u8>, String>;
    fn is_enabled(&self) -> bool;
}

pub struct NoopConfigEncryptor;

impl ConfigEncryptor for NoopConfigEncryptor {
    fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedValue, String> {
        Ok(EncryptedValue {
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            nonce: vec![0; 12],
            ciphertext: plaintext.to_vec(),
        })
    }

    fn decrypt(&self, encrypted: &EncryptedValue) -> Result<Vec<u8>, String> {
        Ok(encrypted.ciphertext.clone())
    }

    fn is_enabled(&self) -> bool {
        false
    }
}
