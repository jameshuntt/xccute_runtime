//! Deterministic operation identity, exit-status interpretation, and verified
//! plan receipts.
//!
//! This module is intentionally runtime-level: hashing, exit-status mapping, and
//! chain decisions describe what happened and what should happen next. Concrete
//! command catalogs should feed this layer; they should not own it.

use std::borrow::Cow;
use std::ffi::OsString;
use std::process::{ExitStatus, Output};

use sha2::{Digest, Sha256};
use xccute_contract::CommandPreview;

/// Stable SHA-256 digest rendered as lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableDigest(String);

impl StableDigest {
    pub fn sha256(material: impl AsRef<[u8]>) -> Self {
        let digest = Sha256::digest(material.as_ref());
        let mut out = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write;
            let _ = write!(&mut out, "{byte:02x}");
        }
        Self(out)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for StableDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A deterministic, argv-safe operation description that can be hashed before
/// execution and referred to inside NodePlan/denv connectors later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOperation {
    pub logical_id: String,
    pub program: OsString,
    pub argv: Vec<OsString>,
    pub display: String,
}

impl RuntimeOperation {
    pub fn new(logical_id: impl Into<String>, program: OsString, argv: Vec<OsString>, display: impl Into<String>) -> Self {
        Self {
            logical_id: logical_id.into(),
            program,
            argv,
            display: display.into(),
        }
    }

    pub fn from_preview(logical_id: impl Into<String>, preview: &CommandPreview) -> Self {
        Self::new(
            logical_id,
            preview.program().to_os_string(),
            preview.argv().to_vec(),
            preview.display().to_string(),
        )
    }

    /// Stable material used for hashing. The format is deliberately plain text
    /// and versioned so future connectors can reproduce it exactly.
    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.operation.v1\n");
        push_stable_field(&mut material, "logical_id", &self.logical_id);
        push_stable_field(&mut material, "program", &lossy(&self.program));
        for (index, arg) in self.argv.iter().enumerate() {
            push_stable_field(&mut material, &format!("argv[{index}]"), &lossy(arg));
        }
        push_stable_field(&mut material, "display", &self.display);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// A deterministic sequence of operations. This is the runtime seam for
/// verified chains of repeatable work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOperationPlan {
    pub plan_id: String,
    pub operations: Vec<RuntimeOperation>,
}

impl RuntimeOperationPlan {
    pub fn new(plan_id: impl Into<String>) -> Self {
        Self {
            plan_id: plan_id.into(),
            operations: Vec::new(),
        }
    }

    pub fn then(mut self, operation: RuntimeOperation) -> Self {
        self.operations.push(operation);
        self
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.plan.v1\n");
        push_stable_field(&mut material, "plan_id", &self.plan_id);
        for (index, operation) in self.operations.iter().enumerate() {
            material.push_str("operation[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(operation.digest().as_str());
            material.push('\n');
            material.push_str(operation.stable_material().as_str());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }

    pub fn contains_operation_digest(&self, digest: &StableDigest) -> bool {
        self.operations.iter().any(|operation| operation.digest().as_str() == digest.as_str())
    }
}

/// Runtime view of a process exit status. This keeps raw status facts separate
/// from the policy decision that interprets them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExitStatus {
    pub code: Option<i32>,
    pub accepted: bool,
}

impl RuntimeExitStatus {
    pub fn new(code: Option<i32>, accepted: bool) -> Self {
        Self { code, accepted }
    }

    pub fn from_exit_status(status: &ExitStatus, accepted: bool) -> Self {
        Self::new(status.code(), accepted)
    }

    pub fn from_output(output: &Output, accepted: bool) -> Self {
        Self::from_exit_status(&output.status, accepted)
    }
}

/// The next-step interpretation of an exit status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitDisposition {
    Continue,
    Stop,
    JumpTo(String),
}

/// A known reason for what to do after a status is observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitStatusDecision {
    pub status: RuntimeExitStatus,
    pub disposition: ExitDisposition,
    pub reason: String,
}

/// Rule mapping one status shape to a known runtime decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitStatusRule {
    pub code: Option<i32>,
    pub accepted: Option<bool>,
    pub disposition: ExitDisposition,
    pub reason: String,
}

impl ExitStatusRule {
    pub fn accepted_code(code: i32, disposition: ExitDisposition, reason: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            accepted: Some(true),
            disposition,
            reason: reason.into(),
        }
    }

    pub fn rejected_code(code: i32, disposition: ExitDisposition, reason: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            accepted: Some(false),
            disposition,
            reason: reason.into(),
        }
    }

    fn matches(&self, status: &RuntimeExitStatus) -> bool {
        self.code.map_or(true, |code| status.code == Some(code))
            && self.accepted.map_or(true, |accepted| status.accepted == accepted)
    }
}

/// Ordered exit-status policy. First matching rule wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitStatusPolicy {
    pub rules: Vec<ExitStatusRule>,
    pub default_continue_reason: String,
    pub default_stop_reason: String,
}

impl Default for ExitStatusPolicy {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            default_continue_reason: "accepted status: continue".to_string(),
            default_stop_reason: "unaccepted status: stop".to_string(),
        }
    }
}

impl ExitStatusPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rule(mut self, rule: ExitStatusRule) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn decide(&self, status: RuntimeExitStatus) -> ExitStatusDecision {
        if let Some(rule) = self.rules.iter().find(|rule| rule.matches(&status)) {
            return ExitStatusDecision {
                status,
                disposition: rule.disposition.clone(),
                reason: rule.reason.clone(),
            };
        }

        if status.accepted {
            ExitStatusDecision {
                status,
                disposition: ExitDisposition::Continue,
                reason: self.default_continue_reason.clone(),
            }
        } else {
            ExitStatusDecision {
                status,
                disposition: ExitDisposition::Stop,
                reason: self.default_stop_reason.clone(),
            }
        }
    }
}

/// Receipt that links a plan hash, operation hash, observed status, and the
/// reasoned next-step decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOperationReceipt {
    pub plan_digest: StableDigest,
    pub operation_digest: StableDigest,
    pub decision: ExitStatusDecision,
}

impl RuntimeOperationReceipt {
    pub fn new(
        plan: &RuntimeOperationPlan,
        operation: &RuntimeOperation,
        decision: ExitStatusDecision,
    ) -> Self {
        Self {
            plan_digest: plan.digest(),
            operation_digest: operation.digest(),
            decision,
        }
    }

    pub fn stable_material(&self) -> String {
        format!(
            "xccute.runtime.receipt.v1\nplan_digest={}\noperation_digest={}\ncode={:?}\naccepted={}\ndisposition={:?}\nreason={}\n",
            self.plan_digest,
            self.operation_digest,
            self.decision.status.code,
            self.decision.status.accepted,
            self.decision.disposition,
            self.decision.reason,
        )
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

fn lossy(value: &OsString) -> Cow<'_, str> {
    value.to_string_lossy()
}
