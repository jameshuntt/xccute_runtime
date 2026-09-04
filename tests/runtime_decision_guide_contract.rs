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
    RuntimeOperation,
    RuntimeConnectorExecutionIntent,
    RuntimeConnectorExecutionReceipt,
) {
    let inspect_operation = RuntimeOperation::new(
        "fleet.inspect_logs",
        OsString::from("grep"),
        vec![OsString::from("-n"), OsString::from("ERROR"), OsString::from("fleet.log")],
        "grep -n ERROR fleet.log",
    );
    let commit_operation = RuntimeOperation::new(
        "fleet.commit_receipt",
        OsString::from("fleetctl"),
        vec![OsString::from("receipt"), OsString::from("commit")],
        "fleetctl receipt commit",
    );
    let plan = RuntimeOperationPlan::new("fleet.bootstrap")
        .then(inspect_operation.clone())
        .then(commit_operation.clone());

    let script_bytes = b"grep -n ERROR fleet.log\n";
    let script_path = unique_temp_file("decision_guide_script", script_bytes);
    let manifest = RuntimeMaterialManifest::new("fleet.bootstrap.materials").with_material(
        RuntimeMaterialSpec::required_file(
            "scripts.inspect_logs",
            RuntimeMaterialKind::Script,
            script_path,
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
        &inspect_operation,
        "inspect logs before choosing next step",
    )
    .expect("operation should be in plan");
    let intent = RuntimeConnectorExecutionIntent::prepare(call.clone(), &material_contract, &manifest)
        .expect("verified material should prepare execution intent");

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::Continue,
            "pattern search completed: continue to receipt commit",
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
        &inspect_operation,
        decision,
    )
    .expect("connector receipt should form");
    let execution_receipt = RuntimeConnectorExecutionReceipt::new(&intent, connector_receipt);

    (plan, inspect_operation, commit_operation, intent, execution_receipt)
}

fn build_guide(plan: &RuntimeOperationPlan, operation: &RuntimeOperation) -> RuntimeDecisionGuide {
    RuntimeDecisionGuide::new(
        "fleet.bootstrap.guide",
        "Decide whether fleet can safely move from log inspection to receipt commit.",
        plan,
    )
    .ask(
        RuntimeDecisionQuestion::required(
            "q.error_pattern",
            &operation.logical_id,
            RuntimeObservationKind::PatternSearch,
            "Did grep find the ERROR pattern in fleet.log?",
            "Find blocking error evidence before deciding whether to continue.",
        )
        .with_context_budget_hint(256),
    )
}

#[test]
fn decision_guide_builds_observation_plan_from_required_questions() {
    let (plan, operation, _next, _intent, _execution_receipt) = build_verified_execution();
    let guide = build_guide(&plan, &operation);
    let observation_plan = guide.to_observation_plan("fleet.bootstrap.observations");

    assert_eq!(observation_plan.plan_id, "fleet.bootstrap");
    assert_eq!(observation_plan.plan_digest, plan.digest());
    assert_eq!(observation_plan.requirements.len(), 1);
    assert_eq!(observation_plan.requirements[0].logical_id, "q.error_pattern");
    assert_eq!(observation_plan.requirements[0].question, "Did grep find the ERROR pattern in fleet.log?");
    assert_eq!(observation_plan.requirements[0].context_budget_hint, 256);
}

#[test]
fn decision_context_requires_declared_questions_before_solution_path_is_accepted() {
    let (plan, operation, _next, _intent, _execution_receipt) = build_verified_execution();
    let guide = build_guide(&plan, &operation);
    let observation_plan = guide.to_observation_plan("fleet.bootstrap.observations");
    let empty_evidence = RuntimeObservationEvidenceSet::new(&plan);

    let result = RuntimeGuidedDecisionContext::from_evidence(
        &guide,
        &observation_plan,
        &empty_evidence,
    );

    assert!(matches!(
        result,
        Err(RuntimeDecisionGuideError::MissingRequiredQuestions { missing_question_ids })
            if missing_question_ids == vec!["q.error_pattern".to_string()]
    ));
}

#[test]
fn guided_decision_context_keeps_only_declared_compact_evidence() {
    let (plan, operation, _next, intent, _execution_receipt) = build_verified_execution();
    let guide = build_guide(&plan, &operation);
    let observation_plan = guide.to_observation_plan("fleet.bootstrap.observations");
    let requirement = guide.questions[0].to_observation_requirement();
    let call = RuntimeObservationCall::for_intent(&requirement, &intent, "inspect errors")
        .expect("observation call should form");
    let fact = RuntimeObservationFact::from_text(
        &call,
        "fact.error_pattern",
        false,
        0,
        "",
        "grep found 0 ERROR lines",
        "no blocking error evidence, so continue",
    );

    let unrelated_requirement = RuntimeObservationRequirement::optional(
        "q.unrelated",
        &operation.logical_id,
        RuntimeObservationKind::TextTransform,
        "Normalize unrelated text.",
    );
    let unrelated_call = RuntimeObservationCall::for_intent(
        &unrelated_requirement,
        &intent,
        "not part of the guide",
    )
    .expect("optional unrelated call should form");
    let unrelated_fact = RuntimeObservationFact::from_text(
        &unrelated_call,
        "fact.unrelated",
        true,
        9000,
        "very large raw output that should never be copied into the guided context",
        "unrelated giant result",
        "not useful to this decision",
    );

    let evidence = RuntimeObservationEvidenceSet::new(&plan)
        .with_fact(fact)
        .with_fact(unrelated_fact);
    let context = RuntimeGuidedDecisionContext::from_evidence(
        &guide,
        &observation_plan,
        &evidence,
    )
    .expect("declared evidence should form guided context");

    assert_eq!(context.answered_question_ids, vec!["q.error_pattern".to_string()]);
    assert!(context.compact_context.contains("grep found 0 ERROR lines"));
    assert!(context.compact_context.contains("no blocking error evidence"));
    assert!(!context.compact_context.contains("unrelated giant result"));
    assert!(!context.compact_context.contains("very large raw output"));
    assert_eq!(context.context_budget_hint, 256);
}

#[test]
fn acknowledged_decision_path_links_guided_context_to_next_plan_transition() {
    let (plan, operation, next_operation, intent, _execution_receipt) = build_verified_execution();
    let guide = build_guide(&plan, &operation);
    let observation_plan = guide.to_observation_plan("fleet.bootstrap.observations");
    let requirement = guide.questions[0].to_observation_requirement();
    let call = RuntimeObservationCall::for_intent(
        &requirement,
        &intent,
        "check whether errors block receipt commit",
    )
    .expect("observation call should form");
    let fact = RuntimeObservationFact::from_text(
        &call,
        "fact.error_pattern",
        false,
        0,
        "",
        "grep found 0 ERROR lines",
        "no blocking error evidence, so continue to receipt commit",
    );
    let evidence = RuntimeObservationEvidenceSet::new(&plan).with_fact(fact);
    let context = RuntimeGuidedDecisionContext::from_evidence(
        &guide,
        &observation_plan,
        &evidence,
    )
    .expect("context should form");

    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::Continue,
            "required observation found no blocking errors",
        ))
        .decide(RuntimeExitStatus::new(Some(0), true));
    let transition = plan
        .transition_after(&operation.logical_id, &decision)
        .expect("continue should resolve to next operation");
    let path = RuntimeAcknowledgedDecisionPath::new(
        &context,
        &transition,
        "operator accepted the guided evidence path",
    );

    assert_eq!(path.current_logical_id, "fleet.inspect_logs");
    assert_eq!(path.next_logical_id, Some(next_operation.logical_id));
    assert_eq!(path.context_digest, context.digest());
    assert_ne!(path.digest(), context.digest());
    assert!(path
        .stable_material()
        .contains("operator accepted the guided evidence path"));
}
