use anyhow::Result;
use mdrn_core::identity::{Keypair, Vouch};
use std::path::PathBuf;

pub fn run_keygen(key_type: &str, output: Option<&PathBuf>) -> Result<()> {
    let keypair = match key_type {
        "secp256k1" => Keypair::generate_secp256k1()
            .map_err(|e| anyhow::anyhow!("Key generation failed: {}", e))?,
        _ => Keypair::generate_ed25519()
            .map_err(|e| anyhow::anyhow!("Key generation failed: {}", e))?,
    };

    let cbor = keypair
        .to_cbor()
        .map_err(|e| anyhow::anyhow!("Serialization failed: {}", e))?;

    let out_path = output
        .cloned()
        .unwrap_or_else(|| PathBuf::from(format!("{}", std::env::var("HOME").unwrap_or_default() + "/.mdrn/keypair.cbor")));

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&out_path, &cbor)?;

    let identity = keypair.identity();
    tracing::info!(
        path = %out_path.display(),
        identity = %hex::encode(identity.as_bytes()),
        key_type = key_type,
        "Keypair generated"
    );

    println!("Identity: {}", hex::encode(identity.as_bytes()));
    println!("Saved to: {}", out_path.display());

    // Auto-generate a genesis self-vouch so this keypair can broadcast immediately
    let vouch_path = out_path.with_extension("vouch.cbor");
    let self_vouch = Vouch::create(keypair.identity().clone(), &keypair, None)
        .map_err(|e| anyhow::anyhow!("Self-vouch failed: {}", e))?;
    let mut vouch_bytes = Vec::new();
    ciborium::into_writer(&self_vouch, &mut vouch_bytes)
        .map_err(|e| anyhow::anyhow!("Vouch serialization failed: {}", e))?;
    std::fs::write(&vouch_path, &vouch_bytes)?;
    println!("Genesis vouch: {}", vouch_path.display());
    println!("Hint: MDRN_VOUCH={} mdrn broadcast ...", vouch_path.display());

    Ok(())
}
