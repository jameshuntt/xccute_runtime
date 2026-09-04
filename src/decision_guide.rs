//! Decision guides for question-led, evidence-backed runtime paths.
//!
//! This module sits above observation evidence. It models a goal, the focused
//! questions that must be answered, and the compact context that is allowed to
//! explain a next-step decision. The intent is to make runtime chains adaptive:
//! ask for the smallest useful check, hash the raw evidence, keep a compact
//! summary, and acknowledge the path taken toward a solution.

use crate::observation::{
    RuntimeObservationEvidenceSet,
    RuntimeObservationKind,
    RuntimeObservationPlan,
    RuntimeObservationRequirement,
};
use crate::plan_transition::RuntimePlanTransition;
use crate::verified_operation::{RuntimeOperationPlan, StableDigest};

/// A question/check the runtime expects before trusting a decision.
///
/// Examples include:
/// - "Did grep find the ERROR pattern?"
/// - "Did pgrep find a running fleet worker?"
/// - "Did sed normalize the config field we need?"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDecisionQuestion {
    pub logical_id: String,
    pub operation_logical_id: String,
    pub kind: RuntimeObservationKind,
    pub question: String,
    pub responsibility: String,
    pub required: bool,
    pub minimum_facts: usize,
    pub context_budget_hint: usize,
}

impl RuntimeDecisionQuestion {
    pub fn required(
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        kind: RuntimeObservationKind,
        question: impl Into<String>,
        responsibility: impl Into<String>,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            operation_logical_id: operation_logical_id.into(),
            kind,
            question: question.into(),
            responsibility: responsibility.into(),
            required: true,
            minimum_facts: 1,
            context_budget_hint: 512,
        }
    }

    pub fn optional(
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        kind: RuntimeObservationKind,
        question: impl Into<String>,
        responsibility: impl Into<String>,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            operation_logical_id: operation_logical_id.into(),
            kind,
            question: question.into(),
            responsibility: responsibility.into(),
            required: false,
            minimum_facts: 1,
            context_budget_hint: 512,
        }
    }

    pub fn with_minimum_facts(mut self, minimum_facts: usize) -> Self {
        self.minimum_facts = minimum_facts;
        self
    }

    pub fn with_context_budget_hint(mut self, bytes: usize) -> Self {
        self.context_budget_hint = bytes;
        self
    }

    pub fn to_observation_requirement(&self) -> RuntimeObservationRequirement {
        let requirement = if self.required {
            RuntimeObservationRequirement::required(
                self.logical_id.clone(),
                self.operation_logical_id.clone(),
                self.kind.clone(),
                self.question.clone(),
            )
        } else {
            RuntimeObservationRequirement::optional(
                self.logical_id.clone(),
                self.operation_logical_id.clone(),
                self.kind.clone(),
                self.question.clone(),
            )
        };

        requirement.with_context_budget_hint(self.context_budget_hint)
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.decision.question.v1\n");
        push_stable_field(&mut material, "logical_id", &self.logical_id);
        push_stable_field(&mut material, "operation_logical_id", &self.operation_logical_id);
        push_stable_field(&mut material, "kind", &self.kind.stable_label());
        push_stable_field(&mut material, "question", &self.question);
        push_stable_field(&mut material, "responsibility", &self.responsibility);
        push_stable_field(&mut material, "required", &self.required.to_string());
        push_stable_field(&mut material, "minimum_facts", &self.minimum_facts.to_string());
        push_stable_field(&mut material, "context_budget_hint", &self.context_budget_hint.to_string());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// A guide for a solution path: goal + focused questions + bounded context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDecisionGuide {
    pub guide_id: String,
    pub goal: String,
    pub plan_id: String,
    pub plan_digest: StableDigest,
    pub questions: Vec<RuntimeDecisionQuestion>,
}

impl RuntimeDecisionGuide {
    pub fn new(
        guide_id: impl Into<String>,
        goal: impl Into<String>,
        plan: &RuntimeOperationPlan,
    ) -> Self {
        Self {
            guide_id: guide_id.into(),
            goal: goal.into(),
            plan_id: plan.plan_id.clone(),
            plan_digest: plan.digest(),
            questions: Vec::new(),
        }
    }

    pub fn ask(mut self, question: RuntimeDecisionQuestion) -> Self {
        self.questions.push(question);
        self
    }

    pub fn to_observation_plan(&self, observation_plan_id: impl Into<String>) -> RuntimeObservationPlan {
        RuntimeObservationPlan {
            observation_plan_id: observation_plan_id.into(),
            plan_id: self.plan_id.clone(),
            plan_digest: self.plan_digest.clone(),
            requirements: self
                .questions
                .iter()
                .map(RuntimeDecisionQuestion::to_observation_requirement)
                .collect(),
        }
    }

    pub fn required_questions(&self) -> impl Iterator<Item = &RuntimeDecisionQuestion> {
        self.questions.iter().filter(|question| question.required)
    }

    pub fn total_context_budget_hint(&self) -> usize {
        self.questions
            .iter()
            .map(|question| question.context_budget_hint)
            .sum()
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.decision.guide.v1\n");
        push_stable_field(&mut material, "guide_id", &self.guide_id);
        push_stable_field(&mut material, "goal", &self.goal);
        push_stable_field(&mut material, "plan_id", &self.plan_id);
        push_stable_field(&mut material, "plan_digest", self.plan_digest.as_str());
        for (index, question) in self.questions.iter().enumerate() {
            material.push_str("question[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(question.digest().as_str());
            material.push('\n');
            material.push_str(&question.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeDecisionGuideError {
    ObservationPlanDigestMismatch {
        guide_plan_digest: StableDigest,
        observation_plan_digest: StableDigest,
    },
    EvidencePlanDigestMismatch {
        guide_plan_digest: StableDigest,
        evidence_plan_digest: StableDigest,
    },
    MissingRequiredQuestions {
        missing_question_ids: Vec<String>,
    },
}

pub type RuntimeDecisionGuideResult<T> = Result<T, RuntimeDecisionGuideError>;

/// Compact decision context formed from only the questions declared by a guide.
///
/// Raw command output remains behind observation fact digests. This object is the
/// bounded context window a connector/agent can use to explain a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeGuidedDecisionContext {
    pub guide_digest: StableDigest,
    pub observation_plan_digest: StableDigest,
    pub evidence_set_digest: StableDigest,
    pub goal: String,
    pub answered_question_ids: Vec<String>,
    pub compact_context: String,
    pub context_budget_hint: usize,
    pub context_bytes: usize,
}

impl RuntimeGuidedDecisionContext {
    pub fn from_evidence(
        guide: &RuntimeDecisionGuide,
        observation_plan: &RuntimeObservationPlan,
        evidence_set: &RuntimeObservationEvidenceSet,
    ) -> RuntimeDecisionGuideResult<Self> {
        if guide.plan_digest != observation_plan.plan_digest {
            return Err(RuntimeDecisionGuideError::ObservationPlanDigestMismatch {
                guide_plan_digest: guide.plan_digest.clone(),
                observation_plan_digest: observation_plan.plan_digest.clone(),
            });
        }

        if guide.plan_digest != evidence_set.plan_digest {
            return Err(RuntimeDecisionGuideError::EvidencePlanDigestMismatch {
                guide_plan_digest: guide.plan_digest.clone(),
                evidence_plan_digest: evidence_set.plan_digest.clone(),
            });
        }

        let mut answered_question_ids = Vec::new();
        let mut missing_question_ids = Vec::new();
        let mut compact_context = String::new();

        for question in &guide.questions {
            let requirement = question.to_observation_requirement();
            let facts = evidence_set.facts_for_requirement(&requirement);
            if facts.len() >= question.minimum_facts {
                answered_question_ids.push(question.logical_id.clone());
                compact_context.push_str("question: ");
                compact_context.push_str(&question.question);
                compact_context.push('\n');
                compact_context.push_str("responsibility: ");
                compact_context.push_str(&question.responsibility);
                compact_context.push('\n');
                for fact in facts {
                    compact_context.push_str("- ");
                    compact_context.push_str(&fact.fact_id);
                    compact_context.push_str(": ");
                    compact_context.push_str(&fact.compact_summary);
                    compact_context.push_str(" | relevance: ");
                    compact_context.push_str(&fact.decision_relevance);
                    compact_context.push('\n');
                }
            } else if question.required {
                missing_question_ids.push(question.logical_id.clone());
            }
        }

        if !missing_question_ids.is_empty() {
            return Err(RuntimeDecisionGuideError::MissingRequiredQuestions { missing_question_ids });
        }

        let context_bytes = compact_context.len();
        Ok(Self {
            guide_digest: guide.digest(),
            observation_plan_digest: observation_plan.digest(),
            evidence_set_digest: evidence_set.digest(),
            goal: guide.goal.clone(),
            answered_question_ids,
            compact_context,
            context_budget_hint: guide.total_context_budget_hint(),
            context_bytes,
        })
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.guided.decision.context.v1\n");
        push_stable_field(&mut material, "guide_digest", self.guide_digest.as_str());
        push_stable_field(&mut material, "observation_plan_digest", self.observation_plan_digest.as_str());
        push_stable_field(&mut material, "evidence_set_digest", self.evidence_set_digest.as_str());
        push_stable_field(&mut material, "goal", &self.goal);
        for (index, question_id) in self.answered_question_ids.iter().enumerate() {
            push_stable_field(&mut material, &format!("answered_question_ids[{index}]"), question_id);
        }
        push_stable_field(&mut material, "compact_context", &self.compact_context);
        push_stable_field(&mut material, "context_budget_hint", &self.context_budget_hint.to_string());
        push_stable_field(&mut material, "context_bytes", &self.context_bytes.to_string());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Acknowledged path receipt that says: this compact guided context explains why
/// this deterministic transition was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAcknowledgedDecisionPath {
    pub context_digest: StableDigest,
    pub transition_digest: StableDigest,
    pub current_logical_id: String,
    pub next_logical_id: Option<String>,
    pub disposition_label: String,
    pub acknowledged_reason: String,
}

impl RuntimeAcknowledgedDecisionPath {
    pub fn new(
        context: &RuntimeGuidedDecisionContext,
        transition: &RuntimePlanTransition,
        acknowledged_reason: impl Into<String>,
    ) -> Self {
        Self {
            context_digest: context.digest(),
            transition_digest: StableDigest::sha256(transition_stable_material(transition)),
            current_logical_id: transition.current_logical_id.clone(),
            next_logical_id: transition.next_logical_id.clone(),
            disposition_label: format!("{:?}", transition.disposition),
            acknowledged_reason: acknowledged_reason.into(),
        }
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.acknowledged.decision.path.v1\n");
        push_stable_field(&mut material, "context_digest", self.context_digest.as_str());
        push_stable_field(&mut material, "transition_digest", self.transition_digest.as_str());
        push_stable_field(&mut material, "current_logical_id", &self.current_logical_id);
        push_stable_field(
            &mut material,
            "next_logical_id",
            self.next_logical_id.as_deref().unwrap_or(""),
        );
        push_stable_field(&mut material, "disposition_label", &self.disposition_label);
        push_stable_field(&mut material, "acknowledged_reason", &self.acknowledged_reason);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

fn transition_stable_material(transition: &RuntimePlanTransition) -> String {
    let mut material = String::new();
    material.push_str("xccute.runtime.decision.transition.view.v1\n");
    push_stable_field(&mut material, "current_logical_id", &transition.current_logical_id);
    push_stable_field(&mut material, "current_operation_digest", transition.current_operation_digest.as_str());
    push_stable_field(&mut material, "disposition", &format!("{:?}", transition.disposition));
    push_stable_field(&mut material, "reason", &transition.reason);
    push_stable_field(
        &mut material,
        "next_logical_id",
        transition.next_logical_id.as_deref().unwrap_or(""),
    );
    push_stable_field(
        &mut material,
        "next_operation_digest",
        transition
            .next_operation_digest
            .as_ref()
            .map(StableDigest::as_str)
            .unwrap_or(""),
    );
    material
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
