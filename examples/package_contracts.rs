//! Decode complete native packages and verify a synthetic signed provider independently; no identity, network, SQL or payment execution.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde_json::{Value, json};
use veyndo_app_model::{
    distribution::{self, SignatureVerifier},
    manifest::{Manifest, canonical_digest, signature_message},
    payment_provider, presentation,
};
struct OperatorKeys(Value);
impl SignatureVerifier for OperatorKeys {
    type Error = &'static str;
    fn verify_signature(&self, m: &Manifest) -> Result<(), Self::Error> {
        let d = m.distribution.as_ref().ok_or("Missing signed metadata")?;
        let public = self.0[&d.publisher][&d.key_id]
            .as_str()
            .ok_or("Publisher key is not registered")?;
        let key = STANDARD.decode(public).map_err(|_| "Invalid public key")?;
        let signature = STANDARD
            .decode(&d.signature)
            .map_err(|_| "Invalid signature")?;
        UnparsedPublicKey::new(&ED25519, key)
            .verify(
                &signature_message(m).ok_or("Missing signature message")?,
                &signature,
            )
            .map_err(|_| "Signature mismatch")
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../model/tests/fixtures/complete_packages.json"
    ))?;
    for fixture in &fixtures {
        let m: Manifest = serde_json::from_value(fixture["manifest"].clone())?;
        assert_eq!(canonical_digest(&m), fixture["digest"].as_str().unwrap());
    }
    let fixture = fixtures
        .iter()
        .find(|v| v["path"].as_str().unwrap().contains("/payment-provider/"))
        .unwrap();
    let mut m: Manifest = serde_json::from_value(fixture["manifest"].clone())?;
    m.id = "fixture_payment".into();
    m.presentation = Some(serde_json::from_value(
        json!({"icon":"/assets/payment.svg","description":{"en-GB":"Synthetic provider","de-DE":"Testanbieter"}}),
    )?);
    m.distribution = Some(serde_json::from_value(
        json!({"publisher":"fixture","keyId":"first","signature":"","channel":"development"}),
    )?);
    payment_provider::validate_contract(&m)?;
    presentation::validate(m.presentation.as_ref())?;
    // Deliberately public synthetic seed. Real hosts use their approved key storage.
    let pair = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    m.distribution.as_mut().unwrap().signature =
        STANDARD.encode(pair.sign(&signature_message(&m).unwrap()).as_ref());
    let host =
        OperatorKeys(json!({"fixture":{"first":STANDARD.encode(pair.public_key().as_ref())}}));
    distribution::verify(&m, &host).unwrap();
    let mut tampered = m.clone();
    tampered.permissions.push("customers.pii".into());
    assert!(distribution::verify(&tampered, &host).is_err());
    assert!(distribution::verify(&m, &OperatorKeys(json!({}))).is_err());
    println!(
        "Decoded {} complete packages with unchanged fingerprints; verified signed provider, tamper rejection and revoked operator key",
        fixtures.len()
    );
    Ok(())
}
