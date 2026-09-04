use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use xccute_runtime::{
    ExitDisposition,
    ExitStatusPolicy,
    ExitStatusRule,
    RuntimeConnectorCall,
    RuntimeConnectorExecutionIntent,
    RuntimeConnectorExecutionReceipt,
    RuntimeConnectorIdentity,
    RuntimeConnectorObservation,
    RuntimeConnectorReceipt,
    RuntimeExecutionGateError,
    RuntimeExitStatus,
    RuntimeMaterialKind,
    RuntimeMaterialManifest,
    RuntimeMaterialSpec,
    RuntimeOperation,
    RuntimeOperationPlan,
    RuntimePlanMaterialContract,
    StableDigest,
};

fn operation(program: &str, logical_id: &str, args: &[&str]) -> RuntimeOperation {
    RuntimeOperation::new(
        logical_id,
        OsString::from(program),
        args.iter().map(OsString::from).collect(),
        format!("{} {}", program, args.join(" ")),
    )
}

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xccute_runtime_gate_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");
    dir
}

fn verified_manifest(name: &str) -> (PathBuf, RuntimeMaterialManifest) {
    let dir = test_dir(name);
    let script = dir.join("apply.sh");
    let config = dir.join("nodeplan.toml");
    let script_bytes = b"#!/usr/bin/env bash\necho apply\n";
    let config_bytes = b"[nodeplan]\nmode = \"dry_run\"\n";
    fs::write(&script, script_bytes).expect("write script");
    fs::write(&config, config_bytes).expect("write config");

    let manifest = RuntimeMaterialManifest::new("nodeplan.bootstrap.materials")
        .with_material(RuntimeMaterialSpec::required_file(
            "nodeplan.apply.script",
            RuntimeMaterialKind::Script,
            &script,
            StableDigest::sha256(script_bytes),
        ))
        .with_material(RuntimeMaterialSpec::required_file(
            "nodeplan.bootstrap.config",
            RuntimeMaterialKind::Config,
            &config,
            StableDigest::sha256(config_bytes),
        ));

    (dir, manifest)
}

#[test]
fn execution_gate_report_links_material_contract_and_report_digest() {
    let (dir, manifest) = verified_manifest("report_links");
    let dry_run = operation("nodeplanctl", "nodeplan.apply_dry_run", &["apply", "--dry-run"]);
    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap").then(dry_run);
    let contract = RuntimePlanMaterialContract::new(&plan, &manifest);

    let report = contract.verify_manifest(&manifest).expect("manifest should verify");

    assert!(report.ready);
    assert_eq!(report.material_contract.digest(), contract.digest());
    assert_eq!(report.material_report.manifest_digest, manifest.digest());
    assert_eq!(report.blocking_logical_ids(), Vec::<String>::new());
    assert_eq!(report.digest(), report.clone().digest());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn execution_intent_requires_connector_call_and_material_contract_to_share_plan_digest() {
    let (dir, manifest) = verified_manifest("plan_digest_mismatch");
    let dry_run = operation("nodeplanctl", "nodeplan.apply_dry_run", &["apply", "--dry-run"]);
    let plan_for_call = RuntimeOperationPlan::new("nodeplan.bootstrap").then(dry_run.clone());
    let different_plan = RuntimeOperationPlan::new("nodeplan.other").then(dry_run.clone());
    let contract = RuntimePlanMaterialContract::new(&different_plan, &manifest);

    let call = RuntimeConnectorCall::for_operation(
        RuntimeConnectorIdentity::new("nodeplan.local", "nodeplan"),
        "nodeplan-control",
        "apply_dry_run",
        &plan_for_call,
        &dry_run,
        "dry-run before committing receipt",
    )
    .expect("operation belongs to call plan");

    let error = RuntimeConnectorExecutionIntent::prepare(call, &contract, &manifest)
        .expect_err("mismatched plan/material contracts must not prepare");

    assert!(matches!(error, RuntimeExecutionGateError::PlanDigestMismatch { .. }));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn execution_intent_blocks_missing_required_material_before_connector_runs() {
    let dir = test_dir("missing_blocks");
    let missing_script = dir.join("missing_apply.sh");
    let manifest = RuntimeMaterialManifest::new("nodeplan.bootstrap.materials")
        .with_material(RuntimeMaterialSpec::required_file(
            "nodeplan.apply.script",
            RuntimeMaterialKind::Script,
            &missing_script,
            StableDigest::sha256(b"expected script bytes"),
        ));

    let dry_run = operation("nodeplanctl", "nodeplan.apply_dry_run", &["apply", "--dry-run"]);
    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap").then(dry_run.clone());
    let contract = RuntimePlanMaterialContract::new(&plan, &manifest);

    let call = RuntimeConnectorCall::for_operation(
        RuntimeConnectorIdentity::new("nodeplan.local", "nodeplan"),
        "nodeplan-control",
        "apply_dry_run",
        &plan,
        &dry_run,
        "dry-run before committing receipt",
    )
    .expect("operation belongs to plan");

    let error = RuntimeConnectorExecutionIntent::prepare(call, &contract, &manifest)
        .expect_err("missing required material must block execution intent");

    match error {
        RuntimeExecutionGateError::MaterialContractNotSatisfied { blocking_logical_ids, .. } => {
            assert_eq!(blocking_logical_ids, vec!["nodeplan.apply.script".to_string()]);
        }
        other => panic!("expected material contract error, got {other:?}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn execution_receipt_links_verified_material_gate_to_connector_receipt() {
    let (dir, manifest) = verified_manifest("receipt_links");
    let dry_run = operation("nodeplanctl", "nodeplan.apply_dry_run", &["apply", "--dry-run"]);
    let commit = operation("nodeplanctl", "nodeplan.commit_receipt", &["receipt", "commit"]);
    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(dry_run.clone())
        .then(commit.clone());
    let contract = RuntimePlanMaterialContract::new(&plan, &manifest);

    let call = RuntimeConnectorCall::for_operation(
        RuntimeConnectorIdentity::new("nodeplan.local", "nodeplan"),
        "nodeplan-control",
        "apply_dry_run",
        &plan,
        &dry_run,
        "dry-run before committing receipt",
    )
    .expect("operation belongs to plan");

    let intent = RuntimeConnectorExecutionIntent::prepare(call.clone(), &contract, &manifest)
        .expect("verified materials should prepare execution intent");

    let observation = RuntimeConnectorObservation::new(
        &call,
        RuntimeExitStatus::new(Some(0), true),
        "nodeplanctl returned success",
    );
    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::JumpTo("nodeplan.commit_receipt".to_string()),
            "dry-run accepted: commit receipt",
        ))
        .decide(observation.exit_status.clone());
    let connector_receipt = RuntimeConnectorReceipt::from_decision(
        call,
        observation,
        &plan,
        &dry_run,
        decision,
    )
    .expect("connector receipt should resolve transition");

    let execution_receipt = RuntimeConnectorExecutionReceipt::new(&intent, connector_receipt.clone());

    assert_eq!(execution_receipt.intent_digest, intent.digest());
    assert_eq!(execution_receipt.material_contract_digest, contract.digest());
    assert_eq!(execution_receipt.material_report_digest, intent.gate_report.material_report.digest());
    assert_eq!(execution_receipt.connector_receipt.digest(), connector_receipt.digest());
    assert_eq!(execution_receipt.connector_receipt.transition.next_logical_id, Some("nodeplan.commit_receipt".to_string()));
    assert_eq!(execution_receipt.connector_receipt.transition.next_operation_digest, Some(commit.digest()));
    assert_eq!(execution_receipt.digest(), execution_receipt.clone().digest());

    let _ = fs::remove_dir_all(&dir);
}
