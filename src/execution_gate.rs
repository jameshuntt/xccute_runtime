//! Runtime execution gates that combine verified plans, material manifests, and
//! connector calls before anything is executed.
//!
//! This module is the pre-run contract seam for NodePlan, denv, local scripts,
//! CI, and future remote runners. It answers the question: is this connector
//! allowed to run this known operation in this known plan with these known local
//! materials?

use crate::connector::{RuntimeConnectorCall, RuntimeConnectorReceipt};
use crate::material::{
    RuntimeMaterialManifest,
    RuntimeMaterialManifestReport,
    RuntimeMaterialObservation,
    RuntimeMaterialVerificationError,
    RuntimePlanMaterialContract,
};
use crate::verified_operation::StableDigest;

/// Errors raised while preparing a verified runtime execution intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeExecutionGateError {
    PlanDigestMismatch {
        call_plan_digest: StableDigest,
        material_plan_digest: StableDigest,
    },
    ManifestDigestMismatch {
        contract_manifest_digest: StableDigest,
        actual_manifest_digest: StableDigest,
    },
    MaterialVerification(RuntimeMaterialVerificationError),
    MaterialContractNotSatisfied {
        report_digest: StableDigest,
        blocking_logical_ids: Vec<String>,
    },
}

pub type RuntimeExecutionGateResult<T> = Result<T, RuntimeExecutionGateError>;

impl From<RuntimeMaterialVerificationError> for RuntimeExecutionGateError {
    fn from(value: RuntimeMaterialVerificationError) -> Self {
        Self::MaterialVerification(value)
    }
}

/// Report produced by checking a plan/material contract against the current
/// local material state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecutionGateReport {
    pub material_contract: RuntimePlanMaterialContract,
    pub material_report: RuntimeMaterialManifestReport,
    pub ready: bool,
}

impl RuntimeExecutionGateReport {
    pub fn blocking_observations(&self) -> Vec<&RuntimeMaterialObservation> {
        self.material_report.blocking_observations()
    }

    pub fn blocking_logical_ids(&self) -> Vec<String> {
        self.blocking_observations()
            .into_iter()
            .map(|observation| observation.logical_id.clone())
            .collect()
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.execution.gate.report.v1\n");
        push_stable_field(&mut material, "material_contract_digest", self.material_contract.digest().as_str());
        push_stable_field(&mut material, "material_report_digest", self.material_report.digest().as_str());
        push_stable_field(&mut material, "ready", &self.ready.to_string());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

impl RuntimePlanMaterialContract {
    /// Verify a manifest against this contract and return a deterministic gate
    /// report. Digest mismatches are structural errors. Material mismatches are
    /// represented in the report so callers can decide whether to block.
    pub fn verify_manifest(
        &self,
        manifest: &RuntimeMaterialManifest,
    ) -> RuntimeExecutionGateResult<RuntimeExecutionGateReport> {
        let actual_manifest_digest = manifest.digest();
        if self.manifest_digest != actual_manifest_digest {
            return Err(RuntimeExecutionGateError::ManifestDigestMismatch {
                contract_manifest_digest: self.manifest_digest.clone(),
                actual_manifest_digest,
            });
        }

        let material_report = manifest.verify()?;
        let ready = material_report.is_fully_verified();
        Ok(RuntimeExecutionGateReport {
            material_contract: self.clone(),
            material_report,
            ready,
        })
    }
}

/// Prepared intent proving a connector call is linked to a verified plan and a
/// fully satisfied material contract before the service/function is run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConnectorExecutionIntent {
    pub call: RuntimeConnectorCall,
    pub gate_report: RuntimeExecutionGateReport,
}

impl RuntimeConnectorExecutionIntent {
    pub fn prepare(
        call: RuntimeConnectorCall,
        material_contract: &RuntimePlanMaterialContract,
        manifest: &RuntimeMaterialManifest,
    ) -> RuntimeExecutionGateResult<Self> {
        if call.plan_digest != material_contract.plan_digest {
            return Err(RuntimeExecutionGateError::PlanDigestMismatch {
                call_plan_digest: call.plan_digest.clone(),
                material_plan_digest: material_contract.plan_digest.clone(),
            });
        }

        let gate_report = material_contract.verify_manifest(manifest)?;
        if !gate_report.ready {
            return Err(RuntimeExecutionGateError::MaterialContractNotSatisfied {
                report_digest: gate_report.material_report.digest(),
                blocking_logical_ids: gate_report.blocking_logical_ids(),
            });
        }

        Ok(Self { call, gate_report })
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.connector.execution.intent.v1\n");
        push_stable_field(&mut material, "call_digest", self.call.digest().as_str());
        push_stable_field(&mut material, "gate_report_digest", self.gate_report.digest().as_str());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Final execution receipt envelope that proves the connector receipt came from
/// a prepared intent whose materials were verified first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConnectorExecutionReceipt {
    pub intent_digest: StableDigest,
    pub material_contract_digest: StableDigest,
    pub material_report_digest: StableDigest,
    pub connector_receipt: RuntimeConnectorReceipt,
}

impl RuntimeConnectorExecutionReceipt {
    pub fn new(
        intent: &RuntimeConnectorExecutionIntent,
        connector_receipt: RuntimeConnectorReceipt,
    ) -> Self {
        Self {
            intent_digest: intent.digest(),
            material_contract_digest: intent.gate_report.material_contract.digest(),
            material_report_digest: intent.gate_report.material_report.digest(),
            connector_receipt,
        }
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.connector.execution.receipt.v1\n");
        push_stable_field(&mut material, "intent_digest", self.intent_digest.as_str());
        push_stable_field(&mut material, "material_contract_digest", self.material_contract_digest.as_str());
        push_stable_field(&mut material, "material_report_digest", self.material_report_digest.as_str());
        push_stable_field(&mut material, "connector_receipt_digest", self.connector_receipt.digest().as_str());
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
