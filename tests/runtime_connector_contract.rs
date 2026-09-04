use std::ffi::OsString;

use xccute_runtime::{
    ExitDisposition,
    ExitStatusPolicy,
    ExitStatusRule,
    RuntimeConnectorCall,
    RuntimeConnectorError,
    RuntimeConnectorIdentity,
    RuntimeConnectorObservation,
    RuntimeConnectorReceipt,
    RuntimeExitStatus,
    RuntimeOperation,
    RuntimeOperationPlan,
};

fn operation(program: &str, logical_id: &str, args: &[&str]) -> RuntimeOperation {
    RuntimeOperation::new(
        logical_id,
        OsString::from(program),
        args.iter().map(OsString::from).collect(),
        format!("{} {}", program, args.join(" ")),
    )
}

#[test]
fn connector_call_hashes_connector_function_plan_and_operation_identity() {
    let dry_run = operation("fleetctl", "fleet.apply_dry_run", &["apply", "--dry-run"]);
    let plan = RuntimeOperationPlan::new("fleet.bootstrap").then(dry_run.clone());
    let connector = RuntimeConnectorIdentity::new("fleet.local", "fleet");

    let first = RuntimeConnectorCall::for_operation(
        connector.clone(),
        "fleet-control",
        "apply_dry_run",
        &plan,
        &dry_run,
        "operator requested dry-run apply",
    )
    .expect("operation belongs to plan");

    let second = RuntimeConnectorCall::for_operation(
        connector.clone(),
        "fleet-control",
        "apply_dry_run",
        &plan,
        &dry_run,
        "operator requested dry-run apply",
    )
    .expect("same call should form");

    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.plan_digest, plan.digest());
    assert_eq!(first.operation_digest, dry_run.digest());

    let changed_function = RuntimeConnectorCall::for_operation(
        connector,
        "fleet-control",
        "apply_live",
        &plan,
        &dry_run,
        "operator requested dry-run apply",
    )
    .expect("same operation can be requested by a different function contract");

    assert_ne!(first.digest(), changed_function.digest());
}

#[test]
fn connector_call_rejects_operation_that_is_not_in_the_verified_plan() {
    let load = operation("fleetctl", "fleet.load", &["load", "plan.json"]);
    let missing = operation("fleetctl", "fleet.apply_dry_run", &["apply", "--dry-run"]);
    let plan = RuntimeOperationPlan::new("fleet.bootstrap").then(load);

    let error = RuntimeConnectorCall::for_operation(
        RuntimeConnectorIdentity::new("fleet.local", "fleet"),
        "fleet-control",
        "apply_dry_run",
        &plan,
        &missing,
        "should not form because this operation is outside the plan",
    )
    .expect_err("connector calls must be plan-bound");

    assert_eq!(
        error,
        RuntimeConnectorError::OperationNotInPlan {
            logical_id: "fleet.apply_dry_run".to_string(),
        }
    );
}

#[test]
fn connector_receipt_links_function_exit_status_decision_and_next_transition() {
    let load = operation("fleetctl", "fleet.load", &["load", "plan.json"]);
    let dry_run = operation("fleetctl", "fleet.apply_dry_run", &["apply", "--dry-run"]);
    let commit = operation("fleetctl", "fleet.commit_receipt", &["receipt", "commit"]);
    let plan = RuntimeOperationPlan::new("fleet.bootstrap")
        .then(load)
        .then(dry_run.clone())
        .then(commit.clone());

    let call = RuntimeConnectorCall::for_operation(
        RuntimeConnectorIdentity::new("fleet.local", "fleet"),
        "fleet-control",
        "apply_dry_run",
        &plan,
        &dry_run,
        "dry-run before committing receipt",
    )
    .expect("connector call should be plan-bound");

    let observation = RuntimeConnectorObservation::new(
        &call,
        RuntimeExitStatus::new(Some(0), true),
        "fleetctl returned success",
    );

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::JumpTo("fleet.commit_receipt".to_string()),
            "dry-run accepted: commit receipt",
        ))
        .decide(observation.exit_status.clone());

    let receipt = RuntimeConnectorReceipt::from_decision(
        call.clone(),
        observation.clone(),
        &plan,
        &dry_run,
        decision,
    )
    .expect("receipt should resolve transition");

    assert_eq!(receipt.call.digest(), call.digest());
    assert_eq!(receipt.observation.digest(), observation.digest());
    assert_eq!(receipt.operation_receipt.plan_digest, plan.digest());
    assert_eq!(receipt.transition.next_logical_id, Some("fleet.commit_receipt".to_string()));
    assert_eq!(receipt.transition.next_operation_digest, Some(commit.digest()));
    assert_eq!(receipt.digest(), receipt.clone().digest());
}

#[test]
fn connector_contract_is_multipurpose_and_not_fleet_specific() {
    let materialize = operation("envdctl", "envd.materialize_env", &["materialize", "dev"]);
    let plan = RuntimeOperationPlan::new("envd.bootstrap").then(materialize.clone());

    let call = RuntimeConnectorCall::for_operation(
        RuntimeConnectorIdentity::new("envd.local", "envd"),
        "envd-runtime",
        "materialize_env",
        &plan,
        &materialize,
        "prepare deterministic developer environment",
    )
    .expect("envd should use the same connector contract");

    assert_eq!(call.connector.connector_kind, "envd");
    assert_eq!(call.service, "envd-runtime");
    assert_eq!(call.function, "materialize_env");
    assert_eq!(call.plan_digest, plan.digest());
    assert_eq!(call.operation_digest, materialize.digest());
}
