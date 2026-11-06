//! Compile complete native packages independently with explicit host policies, actual Wasm kind parsing and no SQL/network/effects.
#[path = "support/compiler_host.rs"]
mod host;
use veyndo_app_model::{
    compiler,
    manifest::{Manifest, canonical_digest},
};
fn main() -> Result<(), String> {
    let fixtures = host::fixtures();
    for fixture in &fixtures {
        let m: Manifest =
            serde_json::from_value(fixture["manifest"].clone()).map_err(|e| e.to_string())?;
        compiler::compile(&m, &host::Host::default()).map_err(|e| format!("{}: {e:?}", m.id))?;
        assert_eq!(canonical_digest(&m), fixture["digest"].as_str().unwrap());
    }
    let mut unsafe_app = host::package("care_studio");
    unsafe_app.presentation =
        Some(serde_json::from_value(serde_json::json!({"icon":"javascript:alert(1)"})).unwrap());
    assert!(compiler::compile(&unsafe_app, &host::Host::default()).is_err());
    println!(
        "Compiled {} complete native packages through the full domain sequence; unchanged fingerprints and unsafe-artwork rejection",
        fixtures.len()
    );
    Ok(())
}
