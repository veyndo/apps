//! Original platform AES-GCM envelope; caller owns key loading and rotation. No environment, SQL or plaintext API.
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, AeadCore, OsRng, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};
pub struct Vault(Aes256Gcm);
impl Vault {
    pub fn new(key: &[u8]) -> Result<Self, String> {
        Aes256Gcm::new_from_slice(key)
            .map(Self)
            .map_err(|_| "Invalid platform encryption key".into())
    }
    pub fn seal(&self, binding: &str, secret: &str) -> Result<String, String> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let encrypted = self
            .0
            .encrypt(
                &nonce,
                Payload {
                    msg: secret.as_bytes(),
                    aad: binding.as_bytes(),
                },
            )
            .map_err(|_| "Could not store provider key")?;
        Ok(STANDARD.encode([&nonce[..], &encrypted].concat()))
    }
    pub fn open(&self, binding: &str, encoded: &str) -> Result<String, String> {
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "Invalid provider secret")?;
        if bytes.len() < 28 {
            return Err("Invalid provider secret".into());
        }
        let plaintext = self
            .0
            .decrypt(
                &Nonce::from(<[u8; 12]>::try_from(&bytes[..12]).map_err(|_| "Invalid nonce")?),
                Payload {
                    msg: &bytes[12..],
                    aad: binding.as_bytes(),
                },
            )
            .map_err(|_| "Provider key cannot be decrypted; check the platform encryption key")?;
        String::from_utf8(plaintext).map_err(|_| "Invalid provider secret".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_envelope_cross_opens_and_authenticates_every_binding() {
        let vault = Vault::new(&[7; 32]).unwrap();
        let encoded = vault
            .seal("app:a:advice:settings:digest:1", "synthetic")
            .unwrap();
        assert_eq!(
            vault
                .open("app:a:advice:settings:digest:1", &encoded)
                .unwrap(),
            "synthetic"
        );
        for binding in [
            "app:b:advice:settings:digest:1",
            "app:a:other:settings:digest:1",
            "app:a:advice:service:digest:1",
            "app:a:advice:settings:new:1",
            "app:a:advice:settings:digest:2",
        ] {
            assert!(vault.open(binding, &encoded).is_err());
        }
        assert!(
            Vault::new(&[8; 32])
                .unwrap()
                .open("app:a:advice:settings:digest:1", &encoded)
                .is_err()
        );
        let bytes = STANDARD.decode(&encoded).unwrap();
        let original = Aes256Gcm::new_from_slice(&[7; 32]).unwrap();
        assert_eq!(
            original
                .decrypt(
                    &Nonce::from(<[u8; 12]>::try_from(&bytes[..12]).unwrap()),
                    Payload {
                        msg: &bytes[12..],
                        aad: b"app:a:advice:settings:digest:1"
                    }
                )
                .unwrap(),
            b"synthetic"
        );
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let old = original
            .encrypt(
                &nonce,
                Payload {
                    msg: b"older persisted value",
                    aad: b"native",
                },
            )
            .unwrap();
        let old = STANDARD.encode([&nonce[..], &old].concat());
        assert_eq!(vault.open("native", &old).unwrap(), "older persisted value");
        assert!(vault.open("x", "invalid").is_err());
        assert!(Vault::new(&[7; 31]).is_err());
    }
}
