//! Plan transition helpers for deterministic operation chains.
//!
//! This module connects verified operation receipts to the next operation in a
//! plan. It is the runtime seam NodePlan/denv-style connectors can use to say:
//! "this operation exited with this known status, for this known reason, so the
//! next operation is this known plan step."

use crate::verified_operation::{
    ExitDisposition, ExitStatusDecision, RuntimeOperation, RuntimeOperationPlan, StableDigest,
};

/// A deterministic link between neighboring operations in a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOperationPlanLink {
    pub index: usize,
    pub logical_id: String,
    pub operation_digest: StableDigest,
    pub previous_operation_digest: Option<StableDigest>,
    pub next_operation_digest: Option<StableDigest>,
}

/// The resolved next-step view after applying an exit-status decision to an
/// operation inside a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePlanTransition {
    pub current_logical_id: String,
    pub current_operation_digest: StableDigest,
    pub disposition: ExitDisposition,
    pub reason: String,
    pub next_logical_id: Option<String>,
    pub next_operation_digest: Option<StableDigest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimePlanTransitionError {
    UnknownOperation { logical_id: String },
    UnknownJumpTarget { logical_id: String },
}

pub type RuntimePlanTransitionResult = Result<RuntimePlanTransition, RuntimePlanTransitionError>;

impl RuntimeOperationPlan {
    /// Return deterministic adjacency links for every operation in this plan.
    pub fn operation_links(&self) -> Vec<RuntimeOperationPlanLink> {
        self.operations
            .iter()
            .enumerate()
            .map(|(index, operation)| RuntimeOperationPlanLink {
                index,
                logical_id: operation.logical_id.clone(),
                operation_digest: operation.digest(),
                previous_operation_digest: index
                    .checked_sub(1)
                    .and_then(|previous| self.operations.get(previous))
                    .map(RuntimeOperation::digest),
                next_operation_digest: self.operations.get(index + 1).map(RuntimeOperation::digest),
            })
            .collect()
    }

    pub fn operation_index_by_logical_id(&self, logical_id: &str) -> Option<usize> {
        self.operations
            .iter()
            .position(|operation| operation.logical_id == logical_id)
    }

    pub fn operation_by_logical_id(&self, logical_id: &str) -> Option<&RuntimeOperation> {
        self.operation_index_by_logical_id(logical_id)
            .and_then(|index| self.operations.get(index))
    }

    pub fn next_operation_after(&self, logical_id: &str) -> Option<&RuntimeOperation> {
        self.operation_index_by_logical_id(logical_id)
            .and_then(|index| self.operations.get(index + 1))
    }

    /// Resolve the next known step implied by an exit-status decision.
    pub fn transition_after(
        &self,
        current_logical_id: &str,
        decision: &ExitStatusDecision,
    ) -> RuntimePlanTransitionResult {
        let current = self
            .operation_by_logical_id(current_logical_id)
            .ok_or_else(|| RuntimePlanTransitionError::UnknownOperation {
                logical_id: current_logical_id.to_string(),
            })?;

        let next = match &decision.disposition {
            ExitDisposition::Continue => self.next_operation_after(current_logical_id),
            ExitDisposition::Stop => None,
            ExitDisposition::JumpTo(target) => Some(
                self.operation_by_logical_id(target)
                    .ok_or_else(|| RuntimePlanTransitionError::UnknownJumpTarget {
                        logical_id: target.clone(),
                    })?,
            ),
        };

        Ok(RuntimePlanTransition {
            current_logical_id: current.logical_id.clone(),
            current_operation_digest: current.digest(),
            disposition: decision.disposition.clone(),
            reason: decision.reason.clone(),
            next_logical_id: next.map(|operation| operation.logical_id.clone()),
            next_operation_digest: next.map(RuntimeOperation::digest),
        })
    }
}
