use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use xccute_runtime::{
    RuntimeMaterialKind,
    RuntimeMaterialManifest,
    RuntimeMaterialSpec,
    RuntimeMaterialStatus,
    RuntimeOperation,
    RuntimeOperationPlan,
    RuntimePlanMaterialContract,
    StableDigest,
};

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xccute_runtime_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");
    dir
}

#[test]
fn material_spec_hashes_existing_script_bytes_and_verifies_expected_digest() {
    let dir = test_dir("material_script_hash");
    let script = dir.join("fleet_apply.sh");
    let bytes = b"#!/usr/bin/env bash\necho fleet apply\n";
    fs::write(&script, bytes).expect("write script");

    let expected = StableDigest::sha256(bytes);
    let spec = RuntimeMaterialSpec::required_file(
        "fleet.apply.script",
        RuntimeMaterialKind::Script,
        &script,
        expected.clone(),
    );

    let observation = spec.observe().expect("observe material");
    assert_eq!(observation.status, RuntimeMaterialStatus::Verified);
    assert!(observation.satisfies_contract());
    assert_eq!(observation.expected_digest, Some(expected.clone()));
    assert_eq!(observation.observed_digest, Some(expected));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn material_spec_detects_missing_required_script() {
    let dir = test_dir("material_missing_required");
    let script = dir.join("missing.sh");
    let spec = RuntimeMaterialSpec::required_file(
        "fleet.missing.script",
        RuntimeMaterialKind::Script,
        &script,
        StableDigest::sha256(b"expected bytes"),
    );

    let observation = spec.observe().expect("observe missing material");
    assert_eq!(observation.status, RuntimeMaterialStatus::MissingRequired);
    assert!(!observation.satisfies_contract());
    assert!(!observation.exists);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn material_manifest_reports_blocking_digest_mismatches() {
    let dir = test_dir("material_manifest_mismatch");
    let script = dir.join("envd_bootstrap.sh");
    fs::write(&script, b"actual bytes\n").expect("write script");

    let manifest = RuntimeMaterialManifest::new("envd.bootstrap.materials")
        .with_material(RuntimeMaterialSpec::required_file(
            "envd.bootstrap.script",
            RuntimeMaterialKind::Script,
            &script,
            StableDigest::sha256(b"expected different bytes\n"),
        ));

    let report = manifest.verify().expect("verify manifest");
    assert!(!report.is_fully_verified());
    assert_eq!(report.blocking_observations().len(), 1);
    let blocking = report.blocking_observations();
    assert!(matches!(
        &blocking[0].status,
        RuntimeMaterialStatus::DigestMismatch
    ));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn material_manifest_hashes_ordered_materials_deterministically() {
    let first = RuntimeMaterialSpec::required_unpinned(
        "fleet.config",
        RuntimeMaterialKind::Config,
        "fleet.toml",
    );
    let second = RuntimeMaterialSpec::required_unpinned(
        "fleet.script",
        RuntimeMaterialKind::Script,
        "apply.sh",
    );

    let manifest_a = RuntimeMaterialManifest::new("fleet.materials")
        .with_material(first.clone())
        .with_material(second.clone());
    let manifest_b = RuntimeMaterialManifest::new("fleet.materials")
        .with_material(first)
        .with_material(second);
    let manifest_reordered = RuntimeMaterialManifest::new("fleet.materials")
        .with_material(RuntimeMaterialSpec::required_unpinned(
            "fleet.script",
            RuntimeMaterialKind::Script,
            "apply.sh",
        ))
        .with_material(RuntimeMaterialSpec::required_unpinned(
            "fleet.config",
            RuntimeMaterialKind::Config,
            "fleet.toml",
        ));

    assert_eq!(manifest_a.digest(), manifest_b.digest());
    assert_ne!(manifest_a.digest(), manifest_reordered.digest());
}

#[test]
fn plan_material_contract_links_plan_and_manifest_digests() {
    let operation = RuntimeOperation::new(
        "fleet.apply_dry_run",
        OsString::from("fleetctl"),
        vec![OsString::from("apply"), OsString::from("--dry-run")],
        "fleetctl apply --dry-run",
    );
    let plan = RuntimeOperationPlan::new("fleet.bootstrap").then(operation);
    let manifest = RuntimeMaterialManifest::new("fleet.bootstrap.materials")
        .with_material(RuntimeMaterialSpec::required_unpinned(
            "fleet.bootstrap.config",
            RuntimeMaterialKind::Config,
            "fleet.bootstrap.toml",
        ));

    let contract = RuntimePlanMaterialContract::new(&plan, &manifest);

    assert_eq!(contract.plan_id, "fleet.bootstrap");
    assert_eq!(contract.plan_digest, plan.digest());
    assert_eq!(contract.manifest_id, "fleet.bootstrap.materials");
    assert_eq!(contract.manifest_digest, manifest.digest());
    assert!(contract.digest().as_str().len() == 64);
}
