use std::fs;

use tempfile::tempdir;

use roger_storage::{Result, RogerStore, SemanticAssetManifest};

const MODEL_DIGEST: &str =
    "sha256:0d05f729f928b76c15e31e5097fb25f1f11909706e64d9c582607e5d227166c3";

fn sample_manifest() -> SemanticAssetManifest {
    SemanticAssetManifest {
        schema_version: 1,
        package_id: "fastembed-mini".to_owned(),
        revision: "2026-03-31".to_owned(),
        artifact_rel_path: "fastembed/model.bin".to_owned(),
        artifact_digest: MODEL_DIGEST.to_owned(),
        installed_at: 1_743_380_000,
    }
}

fn write_asset(store: &RogerStore, rel_path: &str, payload: &[u8]) -> Result<()> {
    let path = store.layout().semantic_asset_root().join(rel_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, payload)?;
    Ok(())
}

#[test]
fn semantic_asset_manifest_roundtrips_deterministically() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    let manifest = sample_manifest();

    store.install_semantic_asset_manifest(&manifest)?;
    let first = fs::read(store.layout().semantic_asset_manifest_path())?;
    store.install_semantic_asset_manifest(&manifest)?;
    let second = fs::read(store.layout().semantic_asset_manifest_path())?;

    assert_eq!(first, second);
    assert_eq!(store.semantic_asset_manifest()?, Some(manifest));
    Ok(())
}

#[test]
fn semantic_asset_verification_fails_closed_when_artifact_is_missing() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    let manifest = sample_manifest();

    store.install_semantic_asset_manifest(&manifest)?;
    let verification = store.verify_semantic_asset_manifest()?;

    assert!(!verification.verified);
    assert!(
        verification
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("artifact path not found"))
    );
    assert_eq!(verification.manifest, Some(manifest));
    Ok(())
}

#[test]
fn semantic_asset_verification_fails_closed_on_digest_mismatch() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    let mut manifest = sample_manifest();
    manifest.artifact_digest = "sha256:deadbeef".to_owned();

    write_asset(&store, &manifest.artifact_rel_path, b"semantic-v1")?;
    store.install_semantic_asset_manifest(&manifest)?;
    let verification = store.verify_semantic_asset_manifest()?;

    assert!(!verification.verified);
    assert!(
        verification
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("digest mismatch"))
    );
    assert_eq!(verification.manifest, Some(manifest));
    Ok(())
}

#[test]
fn semantic_asset_verification_succeeds_when_manifest_matches_payload() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    let manifest = sample_manifest();

    write_asset(&store, &manifest.artifact_rel_path, b"semantic-v1")?;
    store.install_semantic_asset_manifest(&manifest)?;
    let verification = store.verify_semantic_asset_manifest()?;

    assert!(verification.verified);
    assert_eq!(verification.reason, None);
    assert_eq!(verification.manifest, Some(manifest));
    Ok(())
}
