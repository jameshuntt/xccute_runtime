//! Question-guided observation contracts for adaptive runtime decisions.
//!
//! This module models the "look before deciding" layer: grep/pgrep/sed-style
//! probes, file/process checks, and other observation calls can be represented as
//! required questions. The runtime can then prove which compact evidence was
//! gathered without stuffing large command output into a context window.

use crate::execution_gate::{RuntimeConnectorExecutionIntent, RuntimeConnectorExecutionReceipt};
use crate::verified_operation::{RuntimeOperationPlan, StableDigest};

/// Coarse kind of observation a plan can require before a decision is trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeObservationKind {
    PatternSearch,
    ProcessSearch,
    TextTransform,
    FileCheck,
    CommandProbe,
    Custom(String),
}

impl RuntimeObservationKind {
    pub fn stable_label(&self) -> String {
        match self {
            Self::PatternSearch => "pattern_search".to_string(),
            Self::ProcessSearch => "process_search".to_string(),
            Self::TextTransform => "text_transform".to_string(),
            Self::FileCheck => "file_check".to_string(),
            Self::CommandProbe => "command_probe".to_string(),
            Self::Custom(value) => format!("custom:{value}"),
        }
    }
}

/// A required or optional question/check that should be answered by a compact
/// observation call before the runtime explains why it continued, stopped, or
/// jumped to another operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeObservationRequirement {
    pub logical_id: String,
    pub operation_logical_id: String,
    pub kind: RuntimeObservationKind,
    pub question: String,
    pub required: bool,
    pub context_budget_hint: usize,
}

impl RuntimeObservationRequirement {
    pub fn required(
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        kind: RuntimeObservationKind,
        question: impl Into<String>,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            operation_logical_id: operation_logical_id.into(),
            kind,
            question: question.into(),
            required: true,
            context_budget_hint: 512,
        }
    }

    pub fn optional(
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        kind: RuntimeObservationKind,
        question: impl Into<String>,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            operation_logical_id: operation_logical_id.into(),
            kind,
            question: question.into(),
            required: false,
            context_budget_hint: 512,
        }
    }

    pub fn with_context_budget_hint(mut self, bytes: usize) -> Self {
        self.context_budget_hint = bytes;
        self
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.observation.requirement.v1\n");
        push_stable_field(&mut material, "logical_id", &self.logical_id);
        push_stable_field(&mut material, "operation_logical_id", &self.operation_logical_id);
        push_stable_field(&mut material, "kind", &self.kind.stable_label());
        push_stable_field(&mut material, "question", &self.question);
        push_stable_field(&mut material, "required", &self.required.to_string());
        push_stable_field(&mut material, "context_budget_hint", &self.context_budget_hint.to_string());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Observation requirements linked to a verified operation plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeObservationPlan {
    pub observation_plan_id: String,
    pub plan_id: String,
    pub plan_digest: StableDigest,
    pub requirements: Vec<RuntimeObservationRequirement>,
}

impl RuntimeObservationPlan {
    pub fn new(observation_plan_id: impl Into<String>, plan: &RuntimeOperationPlan) -> Self {
        Self {
            observation_plan_id: observation_plan_id.into(),
            plan_id: plan.plan_id.clone(),
            plan_digest: plan.digest(),
            requirements: Vec::new(),
        }
    }

    pub fn require(mut self, requirement: RuntimeObservationRequirement) -> Self {
        self.requirements.push(requirement);
        self
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.observation.plan.v1\n");
        push_stable_field(&mut material, "observation_plan_id", &self.observation_plan_id);
        push_stable_field(&mut material, "plan_id", &self.plan_id);
        push_stable_field(&mut material, "plan_digest", self.plan_digest.as_str());
        for (index, requirement) in self.requirements.iter().enumerate() {
            material.push_str("requirement[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(requirement.digest().as_str());
            material.push('\n');
            material.push_str(&requirement.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }

    pub fn required_requirements(&self) -> impl Iterator<Item = &RuntimeObservationRequirement> {
        self.requirements.iter().filter(|requirement| requirement.required)
    }
}

/// Errors raised while linking observations to verified runtime work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeObservationError {
    RequirementOperationMismatch {
        requirement_operation_logical_id: String,
        intent_operation_logical_id: String,
    },
    PlanDigestMismatch {
        observation_plan_digest: StableDigest,
        execution_plan_digest: StableDigest,
    },
    EvidencePlanDigestMismatch {
        observation_plan_digest: StableDigest,
        evidence_plan_digest: StableDigest,
    },
    MissingRequiredObservations {
        missing_logical_ids: Vec<String>,
    },
}

pub type RuntimeObservationResult<T> = Result<T, RuntimeObservationError>;

/// A concrete observation call requested from a verified execution intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeObservationCall {
    pub requirement_logical_id: String,
    pub requirement_digest: StableDigest,
    pub intent_digest: StableDigest,
    pub connector_call_digest: StableDigest,
    pub operation_logical_id: String,
    pub operation_digest: StableDigest,
    pub question: String,
    pub requested_reason: String,
}

impl RuntimeObservationCall {
    pub fn for_intent(
        requirement: &RuntimeObservationRequirement,
        intent: &RuntimeConnectorExecutionIntent,
        requested_reason: impl Into<String>,
    ) -> RuntimeObservationResult<Self> {
        if requirement.operation_logical_id != intent.call.operation_logical_id {
            return Err(RuntimeObservationError::RequirementOperationMismatch {
                requirement_operation_logical_id: requirement.operation_logical_id.clone(),
                intent_operation_logical_id: intent.call.operation_logical_id.clone(),
            });
        }

        Ok(Self {
            requirement_logical_id: requirement.logical_id.clone(),
            requirement_digest: requirement.digest(),
            intent_digest: intent.digest(),
            connector_call_digest: intent.call.digest(),
            operation_logical_id: intent.call.operation_logical_id.clone(),
            operation_digest: intent.call.operation_digest.clone(),
            question: requirement.question.clone(),
            requested_reason: requested_reason.into(),
        })
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.observation.call.v1\n");
        push_stable_field(&mut material, "requirement_logical_id", &self.requirement_logical_id);
        push_stable_field(&mut material, "requirement_digest", self.requirement_digest.as_str());
        push_stable_field(&mut material, "intent_digest", self.intent_digest.as_str());
        push_stable_field(&mut material, "connector_call_digest", self.connector_call_digest.as_str());
        push_stable_field(&mut material, "operation_logical_id", &self.operation_logical_id);
        push_stable_field(&mut material, "operation_digest", self.operation_digest.as_str());
        push_stable_field(&mut material, "question", &self.question);
        push_stable_field(&mut material, "requested_reason", &self.requested_reason);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Compact fact produced by an observation call. Large raw output is represented
/// by a stable digest; the stored summary stays small and context-budgeted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeObservationFact {
    pub fact_id: String,
    pub call_digest: StableDigest,
    pub requirement_digest: StableDigest,
    pub found: bool,
    pub collected_count: u64,
    pub output_digest: StableDigest,
    pub compact_summary: String,
    pub decision_relevance: String,
}

impl RuntimeObservationFact {
    pub fn from_text(
        call: &RuntimeObservationCall,
        fact_id: impl Into<String>,
        found: bool,
        collected_count: u64,
        raw_output: impl AsRef<[u8]>,
        compact_summary: impl Into<String>,
        decision_relevance: impl Into<String>,
    ) -> Self {
        Self {
            fact_id: fact_id.into(),
            call_digest: call.digest(),
            requirement_digest: call.requirement_digest.clone(),
            found,
            collected_count,
            output_digest: StableDigest::sha256(raw_output),
            compact_summary: compact_summary.into(),
            decision_relevance: decision_relevance.into(),
        }
    }

    pub fn observed_anything(&self) -> bool {
        self.found || self.collected_count > 0
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.observation.fact.v1\n");
        push_stable_field(&mut material, "fact_id", &self.fact_id);
        push_stable_field(&mut material, "call_digest", self.call_digest.as_str());
        push_stable_field(&mut material, "requirement_digest", self.requirement_digest.as_str());
        push_stable_field(&mut material, "found", &self.found.to_string());
        push_stable_field(&mut material, "collected_count", &self.collected_count.to_string());
        push_stable_field(&mut material, "output_digest", self.output_digest.as_str());
        push_stable_field(&mut material, "compact_summary", &self.compact_summary);
        push_stable_field(&mut material, "decision_relevance", &self.decision_relevance);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Ordered compact evidence collected for a runtime plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeObservationEvidenceSet {
    pub plan_digest: StableDigest,
    pub facts: Vec<RuntimeObservationFact>,
}

impl RuntimeObservationEvidenceSet {
    pub fn new(plan: &RuntimeOperationPlan) -> Self {
        Self {
            plan_digest: plan.digest(),
            facts: Vec::new(),
        }
    }

    pub fn with_fact(mut self, fact: RuntimeObservationFact) -> Self {
        self.facts.push(fact);
        self
    }

    pub fn facts_for_requirement(&self, requirement: &RuntimeObservationRequirement) -> Vec<&RuntimeObservationFact> {
        let digest = requirement.digest();
        self.facts
            .iter()
            .filter(|fact| fact.requirement_digest == digest)
            .collect()
    }

    pub fn missing_required_logical_ids(&self, observation_plan: &RuntimeObservationPlan) -> Vec<String> {
        observation_plan
            .required_requirements()
            .filter(|requirement| self.facts_for_requirement(requirement).is_empty())
            .map(|requirement| requirement.logical_id.clone())
            .collect()
    }

    pub fn satisfies_required_observations(&self, observation_plan: &RuntimeObservationPlan) -> bool {
        self.missing_required_logical_ids(observation_plan).is_empty()
    }

    pub fn compact_context(&self) -> String {
        let mut context = String::new();
        for fact in &self.facts {
            context.push_str("- ");
            context.push_str(&fact.fact_id);
            context.push_str(": ");
            context.push_str(&fact.compact_summary);
            context.push_str(" | relevance: ");
            context.push_str(&fact.decision_relevance);
            context.push('\n');
        }
        context
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.observation.evidence.v1\n");
        push_stable_field(&mut material, "plan_digest", self.plan_digest.as_str());
        for (index, fact) in self.facts.iter().enumerate() {
            material.push_str("fact[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(fact.digest().as_str());
            material.push('\n');
            material.push_str(&fact.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Receipt linking a verified execution receipt to the compact observations that
/// explain its decision path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeObservationReceipt {
    pub execution_receipt_digest: StableDigest,
    pub observation_plan_digest: StableDigest,
    pub evidence_set_digest: StableDigest,
    pub compact_context_digest: StableDigest,
}

impl RuntimeObservationReceipt {
    pub fn new(
        execution_receipt: &RuntimeConnectorExecutionReceipt,
        observation_plan: &RuntimeObservationPlan,
        evidence_set: &RuntimeObservationEvidenceSet,
    ) -> RuntimeObservationResult<Self> {
        let execution_plan_digest = execution_receipt.connector_receipt.call.plan_digest.clone();
        if observation_plan.plan_digest != execution_plan_digest {
            return Err(RuntimeObservationError::PlanDigestMismatch {
                observation_plan_digest: observation_plan.plan_digest.clone(),
                execution_plan_digest,
            });
        }

        if evidence_set.plan_digest != observation_plan.plan_digest {
            return Err(RuntimeObservationError::EvidencePlanDigestMismatch {
                observation_plan_digest: observation_plan.plan_digest.clone(),
                evidence_plan_digest: evidence_set.plan_digest.clone(),
            });
        }

        let missing_logical_ids = evidence_set.missing_required_logical_ids(observation_plan);
        if !missing_logical_ids.is_empty() {
            return Err(RuntimeObservationError::MissingRequiredObservations { missing_logical_ids });
        }

        Ok(Self {
            execution_receipt_digest: execution_receipt.digest(),
            observation_plan_digest: observation_plan.digest(),
            evidence_set_digest: evidence_set.digest(),
            compact_context_digest: {
                let compact_context = evidence_set.compact_context();
                StableDigest::sha256(compact_context.as_bytes())
            },
        })
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.observation.receipt.v1\n");
        push_stable_field(&mut material, "execution_receipt_digest", self.execution_receipt_digest.as_str());
        push_stable_field(&mut material, "observation_plan_digest", self.observation_plan_digest.as_str());
        push_stable_field(&mut material, "evidence_set_digest", self.evidence_set_digest.as_str());
        push_stable_field(&mut material, "compact_context_digest", self.compact_context_digest.as_str());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Append-only trace entry. Each entry links to the previous entry digest so a
/// chain of verified, evidence-backed operations can be replayed and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecutionTraceEntry {
    pub sequence: u64,
    pub previous_entry_digest: Option<StableDigest>,
    pub execution_receipt_digest: StableDigest,
    pub observation_receipt_digest: Option<StableDigest>,
    pub reason: String,
}

impl RuntimeExecutionTraceEntry {
    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.execution.trace.entry.v1\n");
        push_stable_field(&mut material, "sequence", &self.sequence.to_string());
        push_stable_field(
            &mut material,
            "previous_entry_digest",
            self.previous_entry_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(&mut material, "execution_receipt_digest", self.execution_receipt_digest.as_str());
        push_stable_field(
            &mut material,
            "observation_receipt_digest",
            self.observation_receipt_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(&mut material, "reason", &self.reason);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Append-only execution trace for verified runtime work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecutionTrace {
    pub trace_id: String,
    pub entries: Vec<RuntimeExecutionTraceEntry>,
}

impl RuntimeExecutionTrace {
    pub fn new(trace_id: impl Into<String>) -> Self {
        Self {
            trace_id: trace_id.into(),
            entries: Vec::new(),
        }
    }

    pub fn append(
        mut self,
        execution_receipt: &RuntimeConnectorExecutionReceipt,
        observation_receipt: Option<&RuntimeObservationReceipt>,
        reason: impl Into<String>,
    ) -> Self {
        let previous_entry_digest = self.entries.last().map(RuntimeExecutionTraceEntry::digest);
        let sequence = self.entries.len() as u64;
        self.entries.push(RuntimeExecutionTraceEntry {
            sequence,
            previous_entry_digest,
            execution_receipt_digest: execution_receipt.digest(),
            observation_receipt_digest: observation_receipt.map(RuntimeObservationReceipt::digest),
            reason: reason.into(),
        });
        self
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.execution.trace.v1\n");
        push_stable_field(&mut material, "trace_id", &self.trace_id);
        for (index, entry) in self.entries.iter().enumerate() {
            material.push_str("entry[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(entry.digest().as_str());
            material.push('\n');
            material.push_str(&entry.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

fn push_stable_field(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(".len=");
    out.push_str(&value.len().to_string());
    out.push('\n');
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}
