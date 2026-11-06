//! Test/example-only AES-GCM host port with synthetic or ephemeral keys; never a production key owner.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use veyndo_app_storage::secrets::Crypto;
pub struct FixtureCrypto(pub [u8; 32]);
impl Crypto for FixtureCrypto {
    type Error = &'static str;
    fn seal(&self, binding: &str, plaintext: &str) -> Result<String, Self::Error> {
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, &self.0).map_err(|_| "key")?,
        );
        let mut nonce = [0u8; 12];
        SystemRandom::new().fill(&mut nonce).map_err(|_| "nonce")?;
        let mut bytes = plaintext.as_bytes().to_vec();
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(binding.as_bytes()),
            &mut bytes,
        )
        .map_err(|_| "seal")?;
        Ok(STANDARD.encode([nonce.to_vec(), bytes].concat()))
    }
    fn open(&self, binding: &str, ciphertext: &str) -> Result<String, Self::Error> {
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, &self.0).map_err(|_| "key")?,
        );
        let bytes = STANDARD.decode(ciphertext).map_err(|_| "encoding")?;
        if bytes.len() < 12 {
            return Err("envelope");
        }
        let nonce = bytes[..12].try_into().map_err(|_| "nonce")?;
        let mut bytes = bytes[12..].to_vec();
        let plain = key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(binding.as_bytes()),
                &mut bytes,
            )
            .map_err(|_| "authentication")?;
        String::from_utf8(plain.to_vec()).map_err(|_| "utf8")
    }
}
