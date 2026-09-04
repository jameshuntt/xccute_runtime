use std::ffi::OsString;

use xccute_runtime::{
    ExitDisposition,
    ExitStatusPolicy,
    ExitStatusRule,
    RuntimeExitStatus,
    RuntimeOperation,
    RuntimeOperationPlan,
    RuntimePlanTransitionError,
};

fn operation(logical_id: &str, args: &[&str]) -> RuntimeOperation {
    RuntimeOperation::new(
        logical_id,
        OsString::from("nodeplanctl"),
        args.iter().map(OsString::from).collect(),
        format!("nodeplanctl {}", args.join(" ")),
    )
}

#[test]
fn runtime_plan_exposes_deterministic_operation_links() {
    let load = operation("nodeplan.load", &["load", "plan.json"]);
    let dry_run = operation("nodeplan.apply_dry_run", &["apply", "--dry-run"]);
    let commit = operation("nodeplan.commit_receipt", &["receipt", "commit"]);

    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(load.clone())
        .then(dry_run.clone())
        .then(commit.clone());

    let links = plan.operation_links();

    assert_eq!(links.len(), 3);
    assert_eq!(links[0].logical_id, "nodeplan.load");
    assert_eq!(links[0].previous_operation_digest, None);
    assert_eq!(links[0].next_operation_digest, Some(dry_run.digest()));

    assert_eq!(links[1].previous_operation_digest, Some(load.digest()));
    assert_eq!(links[1].next_operation_digest, Some(commit.digest()));

    assert_eq!(links[2].operation_digest, commit.digest());
    assert_eq!(links[2].next_operation_digest, None);
}

#[test]
fn continue_decision_resolves_to_next_ordered_operation() {
    let load = operation("nodeplan.load", &["load", "plan.json"]);
    let dry_run = operation("nodeplan.apply_dry_run", &["apply", "--dry-run"]);

    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(load)
        .then(dry_run.clone());

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::Continue,
            "load accepted: continue to dry-run",
        ))
        .decide(RuntimeExitStatus::new(Some(0), true));

    let transition = plan
        .transition_after("nodeplan.load", &decision)
        .expect("known current operation should transition");

    assert_eq!(transition.current_logical_id, "nodeplan.load");
    assert_eq!(transition.next_logical_id, Some("nodeplan.apply_dry_run".to_string()));
    assert_eq!(transition.next_operation_digest, Some(dry_run.digest()));
    assert_eq!(transition.reason, "load accepted: continue to dry-run");
}

#[test]
fn jump_decision_resolves_to_named_operation() {
    let load = operation("nodeplan.load", &["load", "plan.json"]);
    let dry_run = operation("nodeplan.apply_dry_run", &["apply", "--dry-run"]);
    let commit = operation("nodeplan.commit_receipt", &["receipt", "commit"]);

    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(load)
        .then(dry_run)
        .then(commit.clone());

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::JumpTo("nodeplan.commit_receipt".to_string()),
            "dry-run accepted: commit receipt",
        ))
        .decide(RuntimeExitStatus::new(Some(0), true));

    let transition = plan
        .transition_after("nodeplan.apply_dry_run", &decision)
        .expect("jump target should exist");

    assert_eq!(transition.next_logical_id, Some("nodeplan.commit_receipt".to_string()));
    assert_eq!(transition.next_operation_digest, Some(commit.digest()));
    assert_eq!(transition.reason, "dry-run accepted: commit receipt");
}

#[test]
fn stop_decision_resolves_to_no_next_operation() {
    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(operation("nodeplan.apply_dry_run", &["apply", "--dry-run"]))
        .then(operation("nodeplan.commit_receipt", &["receipt", "commit"]));

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::rejected_code(
            42,
            ExitDisposition::Stop,
            "nodeplan rejected contract: stop chain",
        ))
        .decide(RuntimeExitStatus::new(Some(42), false));

    let transition = plan
        .transition_after("nodeplan.apply_dry_run", &decision)
        .expect("known current operation can stop");

    assert_eq!(transition.disposition, ExitDisposition::Stop);
    assert_eq!(transition.next_logical_id, None);
    assert_eq!(transition.next_operation_digest, None);
}

#[test]
fn unknown_jump_target_is_a_structural_transition_error() {
    let plan = RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(operation("nodeplan.apply_dry_run", &["apply", "--dry-run"]));

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::JumpTo("nodeplan.missing".to_string()),
            "bad jump should be rejected",
        ))
        .decide(RuntimeExitStatus::new(Some(0), true));

    let error = plan
        .transition_after("nodeplan.apply_dry_run", &decision)
        .expect_err("unknown jump target should fail structurally");

    assert_eq!(
        error,
        RuntimePlanTransitionError::UnknownJumpTarget {
            logical_id: "nodeplan.missing".to_string()
        }
    );
}
