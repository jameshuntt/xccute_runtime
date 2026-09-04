//! Runtime prelude.
//!
//! Keep this as the import surface callers can use when they only care about
//! validated, previewable, policy-approved, deterministic command execution.

pub use crate::command::{RawCommand, ShellCommand};
pub use crate::command_spec::CommandSpec;
pub use crate::connector::{
    RuntimeConnectorCall,
    RuntimeConnectorError,
    RuntimeConnectorIdentity,
    RuntimeConnectorObservation,
    RuntimeConnectorReceipt,
    RuntimeConnectorResult,
};
pub use crate::execution_gate::{
    RuntimeConnectorExecutionIntent,
    RuntimeConnectorExecutionReceipt,
    RuntimeExecutionGateError,
    RuntimeExecutionGateReport,
    RuntimeExecutionGateResult,
};
pub use crate::material::{
    RuntimeMaterialKind,
    RuntimeMaterialManifest,
    RuntimeMaterialManifestReport,
    RuntimeMaterialObservation,
    RuntimeMaterialSpec,
    RuntimeMaterialStatus,
    RuntimeMaterialVerificationError,
    RuntimeMaterialVerificationResult,
    RuntimePlanMaterialContract,
};
pub use crate::observation::*;
pub use crate::plan_transition::{
    RuntimeOperationPlanLink,
    RuntimePlanTransition,
    RuntimePlanTransitionError,
    RuntimePlanTransitionResult,
};
pub use crate::runner::{CaptureMode, CommandChainExecutor, CommandError};
pub use crate::runtime_command::RuntimeCommand;
pub use crate::status::OutputExt;
pub use crate::verified_operation::{
    ExitDisposition,
    ExitStatusDecision,
    ExitStatusPolicy,
    ExitStatusRule,
    RuntimeExitStatus,
    RuntimeOperation,
    RuntimeOperationPlan,
    RuntimeOperationReceipt,
    StableDigest,
};

pub use xccute_contract::{
    CommandApproval,
    CommandApprovalError,
    CommandApprovalReceipt,
    CommandExecutionError,
    CommandPolicyError,
    CommandPolicyResult,
    CommandPreview,
    CommandPreviewPolicy,
    CommandValidationError,
    CommandValidationErrorKind,
    CommandValidationResult,
    CompositeArgvPart,
    CompositeShellCommand,
    CompositeShellRoot,
    RootableCompositeSurface,
    RootableSubCommand,
    ValidatedCommand,
    ValidatedCompositeShellCommand,
};
