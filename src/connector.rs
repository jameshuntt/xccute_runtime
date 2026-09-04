//! Runtime connector contracts for external systems such as a fleet supervisor, an environment daemon, or
//! any other orchestrator that wants to feed verified operations into xccute.
//!
//! This module intentionally stays generic. A connector is not the command
//! itself; it is the contract boundary saying: this service/function requested
//! this known operation in this known plan for this known reason, observed this
//! exit status, and produced this transition/receipt.

use crate::plan_transition::{RuntimePlanTransition, RuntimePlanTransitionError};
use crate::verified_operation::{
    ExitStatusDecision,
    RuntimeExitStatus,
    RuntimeOperation,
    RuntimeOperationPlan,
    RuntimeOperationReceipt,
    StableDigest,
};

/// Stable identity for a runtime connector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConnectorIdentity {
    pub connector_id: String,
    pub connector_kind: String,
}

impl RuntimeConnectorIdentity {
    pub fn new(connector_id: impl Into<String>, connector_kind: impl Into<String>) -> Self {
        Self {
            connector_id: connector_id.into(),
            connector_kind: connector_kind.into(),
        }
    }
}

/// Errors raised while forming connector contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeConnectorError {
    OperationNotInPlan { logical_id: String },
    ObservationCallMismatch {
        expected_call_digest: StableDigest,
        actual_call_digest: StableDigest,
    },
    Transition(RuntimePlanTransitionError),
}

pub type RuntimeConnectorResult<T> = Result<T, RuntimeConnectorError>;

impl From<RuntimePlanTransitionError> for RuntimeConnectorError {
    fn from(value: RuntimePlanTransitionError) -> Self {
        Self::Transition(value)
    }
}

/// Deterministic request from an external connector into the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConnectorCall {
    pub connector: RuntimeConnectorIdentity,
    pub service: String,
    pub function: String,
    pub plan_id: String,
    pub plan_digest: StableDigest,
    pub operation_logical_id: String,
    pub operation_digest: StableDigest,
    pub requested_reason: String,
}

impl RuntimeConnectorCall {
    pub fn for_operation(
        connector: RuntimeConnectorIdentity,
        service: impl Into<String>,
        function: impl Into<String>,
        plan: &RuntimeOperationPlan,
        operation: &RuntimeOperation,
        requested_reason: impl Into<String>,
    ) -> RuntimeConnectorResult<Self> {
        let operation_digest = operation.digest();
        if !plan.contains_operation_digest(&operation_digest) {
            return Err(RuntimeConnectorError::OperationNotInPlan {
                logical_id: operation.logical_id.clone(),
            });
        }

        Ok(Self {
            connector,
            service: service.into(),
            function: function.into(),
            plan_id: plan.plan_id.clone(),
            plan_digest: plan.digest(),
            operation_logical_id: operation.logical_id.clone(),
            operation_digest,
            requested_reason: requested_reason.into(),
        })
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.connector.call.v1\n");
        push_stable_field(&mut material, "connector_id", &self.connector.connector_id);
        push_stable_field(&mut material, "connector_kind", &self.connector.connector_kind);
        push_stable_field(&mut material, "service", &self.service);
        push_stable_field(&mut material, "function", &self.function);
        push_stable_field(&mut material, "plan_id", &self.plan_id);
        push_stable_field(&mut material, "plan_digest", self.plan_digest.as_str());
        push_stable_field(&mut material, "operation_logical_id", &self.operation_logical_id);
        push_stable_field(&mut material, "operation_digest", self.operation_digest.as_str());
        push_stable_field(&mut material, "requested_reason", &self.requested_reason);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Deterministic observation made after the connector's service/function ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConnectorObservation {
    pub call_digest: StableDigest,
    pub exit_status: RuntimeExitStatus,
    pub observed_reason: String,
}

impl RuntimeConnectorObservation {
    pub fn new(
        call: &RuntimeConnectorCall,
        exit_status: RuntimeExitStatus,
        observed_reason: impl Into<String>,
    ) -> Self {
        Self {
            call_digest: call.digest(),
            exit_status,
            observed_reason: observed_reason.into(),
        }
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.connector.observation.v1\n");
        push_stable_field(&mut material, "call_digest", self.call_digest.as_str());
        push_stable_field(&mut material, "exit_code", &format!("{:?}", self.exit_status.code));
        push_stable_field(&mut material, "accepted", &self.exit_status.accepted.to_string());
        push_stable_field(&mut material, "observed_reason", &self.observed_reason);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Connector-level receipt linking the external function call, observed status,
/// runtime decision, and resolved next-step transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConnectorReceipt {
    pub call: RuntimeConnectorCall,
    pub observation: RuntimeConnectorObservation,
    pub operation_receipt: RuntimeOperationReceipt,
    pub transition: RuntimePlanTransition,
}

impl RuntimeConnectorReceipt {
    pub fn from_decision(
        call: RuntimeConnectorCall,
        observation: RuntimeConnectorObservation,
        plan: &RuntimeOperationPlan,
        operation: &RuntimeOperation,
        decision: ExitStatusDecision,
    ) -> RuntimeConnectorResult<Self> {
        let expected_call_digest = call.digest();
        if observation.call_digest != expected_call_digest {
            return Err(RuntimeConnectorError::ObservationCallMismatch {
                expected_call_digest,
                actual_call_digest: observation.call_digest,
            });
        }

        let operation_receipt = RuntimeOperationReceipt::new(plan, operation, decision.clone());
        let transition = plan.transition_after(&operation.logical_id, &decision)?;

        Ok(Self {
            call,
            observation,
            operation_receipt,
            transition,
        })
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.connector.receipt.v1\n");
        push_stable_field(&mut material, "call_digest", self.call.digest().as_str());
        push_stable_field(&mut material, "observation_digest", self.observation.digest().as_str());
        push_stable_field(&mut material, "operation_receipt_digest", self.operation_receipt.digest().as_str());
        push_stable_field(&mut material, "transition_current", &self.transition.current_logical_id);
        push_stable_field(&mut material, "transition_reason", &self.transition.reason);
        push_stable_field(
            &mut material,
            "transition_next",
            self.transition.next_logical_id.as_deref().unwrap_or(""),
        );
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
