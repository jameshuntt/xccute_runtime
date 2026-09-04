use std::ffi::OsString;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use xccute_runtime::*;

fn unique_temp_file(name: &str, contents: &[u8]) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("xccute_{name}_{nanos}.txt"));
    fs::write(&path, contents).expect("write temp material");
    path
}

fn build_verified_execution() -> (
    RuntimeOperationPlan,
    RuntimeOperation,
    RuntimeConnectorExecutionIntent,
    RuntimeConnectorExecutionReceipt,
) {
    let operation = RuntimeOperation::new(
        "fleet.inspect_logs",
        OsString::from("grep"),
        vec![OsString::from("-n"), OsString::from("ERROR"), OsString::from("fleet.log")],
        "grep -n ERROR fleet.log",
    );
    let plan = RuntimeOperationPlan::new("fleet.bootstrap").then(operation.clone());

    let script_bytes = b"grep -n ERROR fleet.log\n";
    let script_path = unique_temp_file("observation_script", script_bytes);
    let manifest = RuntimeMaterialManifest::new("fleet.bootstrap.materials").with_material(
        RuntimeMaterialSpec::required_file(
            "scripts.inspect_logs",
            RuntimeMaterialKind::Script,
            script_path.clone(),
            StableDigest::sha256(script_bytes),
        ),
    );
    let material_contract = RuntimePlanMaterialContract::new(&plan, &manifest);

    let connector = RuntimeConnectorIdentity::new("fleet.local", "fleet");
    let call = RuntimeConnectorCall::for_operation(
        connector,
        "fleet-control",
        "inspect_logs",
        &plan,
        &operation,
        "inspect logs before choosing next step",
    )
    .expect("operation should be in plan");
    let intent = RuntimeConnectorExecutionIntent::prepare(call.clone(), &material_contract, &manifest)
        .expect("verified material should prepare execution intent");

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::Stop,
            "pattern search completed: stop for operator review",
        ))
        .decide(RuntimeExitStatus::new(Some(0), true));
    let connector_observation = RuntimeConnectorObservation::new(
        &call,
        RuntimeExitStatus::new(Some(0), true),
        "grep completed successfully",
    );
    let connector_receipt = RuntimeConnectorReceipt::from_decision(
        call,
        connector_observation,
        &plan,
        &operation,
        decision,
    )
    .expect("connector receipt should form");
    let execution_receipt = RuntimeConnectorExecutionReceipt::new(&intent, connector_receipt);

    (plan, operation, intent, execution_receipt)
}

#[test]
fn observation_call_links_required_question_to_verified_execution_intent() {
    let (_plan, operation, intent, _execution_receipt) = build_verified_execution();
    let requirement = RuntimeObservationRequirement::required(
        "obs.error_pattern",
        &operation.logical_id,
        RuntimeObservationKind::PatternSearch,
        "Did grep find the ERROR pattern in fleet.log?",
    )
    .with_context_budget_hint(128);

    let call = RuntimeObservationCall::for_intent(
        &requirement,
        &intent,
        "need one focused check before deciding whether to continue",
    )
    .expect("observation call should link to intent");

    assert_eq!(call.requirement_logical_id, "obs.error_pattern");
    assert_eq!(call.operation_logical_id, "fleet.inspect_logs");
    assert_eq!(call.operation_digest, intent.call.operation_digest);
    assert_eq!(call.question, "Did grep find the ERROR pattern in fleet.log?");
}

#[test]
fn observation_fact_hashes_raw_output_but_keeps_compact_decision_context() {
    let (_plan, operation, intent, _execution_receipt) = build_verified_execution();
    let requirement = RuntimeObservationRequirement::required(
        "obs.error_pattern",
        &operation.logical_id,
        RuntimeObservationKind::PatternSearch,
        "Did grep find the ERROR pattern in fleet.log?",
    );
    let call = RuntimeObservationCall::for_intent(&requirement, &intent, "inspect errors")
        .expect("observation call should form");

    let fact = RuntimeObservationFact::from_text(
        &call,
        "fact.error_pattern",
        true,
        2,
        "123:ERROR failed to apply\n190:ERROR missing receipt\n",
        "grep found 2 ERROR lines",
        "errors exist, so stop and surface review context",
    );
    let changed_fact = RuntimeObservationFact::from_text(
        &call,
        "fact.error_pattern",
        true,
        3,
        "123:ERROR failed to apply\n190:ERROR missing receipt\n250:ERROR bad hash\n",
        "grep found 3 ERROR lines",
        "errors exist, so stop and surface review context",
    );

    assert!(fact.observed_anything());
    assert_eq!(fact.compact_summary, "grep found 2 ERROR lines");
    assert_ne!(fact.output_digest, changed_fact.output_digest);
}

#[test]
fn observation_receipt_requires_required_checks_before_decision_context_is_accepted() {
    let (plan, operation, _intent, execution_receipt) = build_verified_execution();
    let observation_plan = RuntimeObservationPlan::new("fleet.bootstrap.observations", &plan)
        .require(RuntimeObservationRequirement::required(
            "obs.error_pattern",
            &operation.logical_id,
            RuntimeObservationKind::PatternSearch,
            "Did grep find the ERROR pattern in fleet.log?",
        ));
    let empty_evidence = RuntimeObservationEvidenceSet::new(&plan);

    let result = RuntimeObservationReceipt::new(
        &execution_receipt,
        &observation_plan,
        &empty_evidence,
    );

    assert!(matches!(
        result,
        Err(RuntimeObservationError::MissingRequiredObservations { missing_logical_ids })
            if missing_logical_ids == vec!["obs.error_pattern".to_string()]
    ));
}

#[test]
fn observation_receipt_and_trace_link_evidence_back_to_verified_execution() {
    let (plan, operation, intent, execution_receipt) = build_verified_execution();
    let requirement = RuntimeObservationRequirement::required(
        "obs.error_pattern",
        &operation.logical_id,
        RuntimeObservationKind::PatternSearch,
        "Did grep find the ERROR pattern in fleet.log?",
    );
    let observation_plan = RuntimeObservationPlan::new("fleet.bootstrap.observations", &plan)
        .require(requirement.clone());
    let call = RuntimeObservationCall::for_intent(&requirement, &intent, "inspect errors")
        .expect("observation call should form");
    let fact = RuntimeObservationFact::from_text(
        &call,
        "fact.error_pattern",
        true,
        2,
        "123:ERROR failed to apply\n190:ERROR missing receipt\n",
        "grep found 2 ERROR lines",
        "errors exist, so stop and surface review context",
    );
    let evidence = RuntimeObservationEvidenceSet::new(&plan).with_fact(fact);

    let observation_receipt = RuntimeObservationReceipt::new(
        &execution_receipt,
        &observation_plan,
        &evidence,
    )
    .expect("required evidence should create observation receipt");

    assert_eq!(observation_receipt.execution_receipt_digest, execution_receipt.digest());
    assert_eq!(observation_receipt.evidence_set_digest, evidence.digest());
    assert!(evidence.compact_context().contains("grep found 2 ERROR lines"));

    let trace = RuntimeExecutionTrace::new("fleet.bootstrap.trace")
        .append(
            &execution_receipt,
            Some(&observation_receipt),
            "stopped because observation evidence found error lines",
        )
        .append(
            &execution_receipt,
            Some(&observation_receipt),
            "operator replayed same checked step",
        );

    assert_eq!(trace.entries.len(), 2);
    assert_eq!(trace.entries[0].sequence, 0);
    assert_eq!(trace.entries[1].sequence, 1);
    assert_eq!(
        trace.entries[1].previous_entry_digest,
        Some(trace.entries[0].digest())
    );
    assert_ne!(trace.entries[0].digest(), trace.entries[1].digest());
}

#[test]
fn observation_requirement_rejects_mismatched_intent_operation() {
    let (_plan, _operation, intent, _execution_receipt) = build_verified_execution();
    let requirement = RuntimeObservationRequirement::required(
        "obs.process_exists",
        "fleet.some_other_step",
        RuntimeObservationKind::ProcessSearch,
        "Is the expected fleet process running?",
    );

    let result = RuntimeObservationCall::for_intent(&requirement, &intent, "check process");

    assert!(matches!(
        result,
        Err(RuntimeObservationError::RequirementOperationMismatch {
            requirement_operation_logical_id,
            intent_operation_logical_id,
        }) if requirement_operation_logical_id == "fleet.some_other_step"
            && intent_operation_logical_id == "fleet.inspect_logs"
    ));
}
