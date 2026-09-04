use std::ffi::OsString;

use xccute_contract::CommandPreview;
use xccute_runtime::{
    ExitDisposition,
    ExitStatusPolicy,
    ExitStatusRule,
    RuntimeExitStatus,
    RuntimeOperation,
    RuntimeOperationPlan,
    RuntimeOperationReceipt,
};

#[test]
fn runtime_operation_hash_is_deterministic_and_argv_sensitive() {
    let preview = CommandPreview::new(
        OsString::from("nodeplanctl"),
        vec![OsString::from("apply"), OsString::from("--plan"), OsString::from("plan.json")],
    );

    let first = RuntimeOperation::from_preview("nodeplan.apply.plan", &preview);
    let second = RuntimeOperation::from_preview("nodeplan.apply.plan", &preview);

    assert_eq!(first.digest(), second.digest());

    let changed = RuntimeOperation::new(
        "nodeplan.apply.plan",
        OsString::from("nodeplanctl"),
        vec![OsString::from("apply"), OsString::from("--plan"), OsString::from("other.json")],
        "nodeplanctl apply --plan other.json",
    );

    assert_ne!(first.digest(), changed.digest());
}

#[test]
fn runtime_operation_plan_hashes_ordered_repeatable_chain() {
    let load = RuntimeOperation::new(
        "nodeplan.load",
        OsString::from("nodeplanctl"),
        vec![OsString::from("load"), OsString::from("plan.json")],
        "nodeplanctl load plan.json",
    );
    let apply = RuntimeOperation::new(
        "nodeplan.apply",
        OsString::from("nodeplanctl"),
        vec![OsString::from("apply"), OsString::from("--dry-run")],
        "nodeplanctl apply --dry-run",
    );

    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(load.clone())
        .then(apply.clone());

    assert!(plan.contains_operation_digest(&load.digest()));
    assert!(plan.contains_operation_digest(&apply.digest()));
    assert_eq!(plan.digest(), plan.clone().digest());

    let reversed = RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(apply)
        .then(load);

    assert_ne!(plan.digest(), reversed.digest());
}

#[test]
fn exit_status_policy_maps_known_reasons_to_next_step_decisions() {
    let policy = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::JumpTo("nodeplan.commit_receipt".to_string()),
            "dry-run accepted: commit the receipt",
        ))
        .with_rule(ExitStatusRule::rejected_code(
            42,
            ExitDisposition::Stop,
            "nodeplan rejected the plan contract",
        ));

    let accepted = policy.decide(RuntimeExitStatus::new(Some(0), true));
    assert_eq!(accepted.disposition, ExitDisposition::JumpTo("nodeplan.commit_receipt".to_string()));
    assert_eq!(accepted.reason, "dry-run accepted: commit the receipt");

    let rejected = policy.decide(RuntimeExitStatus::new(Some(42), false));
    assert_eq!(rejected.disposition, ExitDisposition::Stop);
    assert_eq!(rejected.reason, "nodeplan rejected the plan contract");
}

#[test]
fn runtime_receipt_links_plan_operation_exit_status_and_decision() {
    let operation = RuntimeOperation::new(
        "nodeplan.apply",
        OsString::from("nodeplanctl"),
        vec![OsString::from("apply"), OsString::from("--dry-run")],
        "nodeplanctl apply --dry-run",
    );
    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap").then(operation.clone());

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::Continue,
            "operation accepted: continue chain",
        ))
        .decide(RuntimeExitStatus::new(Some(0), true));

    let receipt = RuntimeOperationReceipt::new(&plan, &operation, decision);

    assert_eq!(receipt.plan_digest, plan.digest());
    assert_eq!(receipt.operation_digest, operation.digest());
    assert_eq!(receipt.decision.reason, "operation accepted: continue chain");
    assert_eq!(receipt.digest(), receipt.clone().digest());
}
