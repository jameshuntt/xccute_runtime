//! Runtime material manifests for scripts, configs, binaries, and other files a
//! verified operation plan depends on.
//!
//! This module is the seam for NodePlan/denv-style connectors that need to prove
//! a plan is not just a list of commands, but a list of commands backed by known
//! local materials. Scripts and configs can be checked for existence and stable
//! SHA-256 digests before a connector runs anything.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::verified_operation::{RuntimeOperationPlan, StableDigest};

/// Coarse material category. This is intentionally generic so NodePlan, denv,
/// local scripts, remote runners, and CI adapters can share one model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMaterialKind {
    Script,
    Config,
    Binary,
    Directory,
    Other(String),
}

impl RuntimeMaterialKind {
    pub fn stable_label(&self) -> String {
        match self {
            Self::Script => "script".to_string(),
            Self::Config => "config".to_string(),
            Self::Binary => "binary".to_string(),
            Self::Directory => "directory".to_string(),
            Self::Other(value) => format!("other:{value}"),
        }
    }
}

/// A material that a runtime plan expects to exist, optionally with a known
/// digest. Required files with no expected digest can be observed, but they are
/// not considered fully verified because the bytes are not pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMaterialSpec {
    pub logical_id: String,
    pub kind: RuntimeMaterialKind,
    pub path: PathBuf,
    pub expected_digest: Option<StableDigest>,
    pub required: bool,
}

impl RuntimeMaterialSpec {
    pub fn required_file(
        logical_id: impl Into<String>,
        kind: RuntimeMaterialKind,
        path: impl Into<PathBuf>,
        expected_digest: StableDigest,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            kind,
            path: path.into(),
            expected_digest: Some(expected_digest),
            required: true,
        }
    }

    pub fn required_unpinned(
        logical_id: impl Into<String>,
        kind: RuntimeMaterialKind,
        path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            kind,
            path: path.into(),
            expected_digest: None,
            required: true,
        }
    }

    pub fn optional_unpinned(
        logical_id: impl Into<String>,
        kind: RuntimeMaterialKind,
        path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            kind,
            path: path.into(),
            expected_digest: None,
            required: false,
        }
    }

    pub fn required_directory(logical_id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            logical_id: logical_id.into(),
            kind: RuntimeMaterialKind::Directory,
            path: path.into(),
            expected_digest: None,
            required: true,
        }
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.material.spec.v1\n");
        push_stable_field(&mut material, "logical_id", &self.logical_id);
        push_stable_field(&mut material, "kind", &self.kind.stable_label());
        push_stable_field(&mut material, "path", &stable_path(&self.path));
        push_stable_field(
            &mut material,
            "expected_digest",
            self.expected_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(&mut material, "required", &self.required.to_string());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }

    pub fn observe(&self) -> RuntimeMaterialVerificationResult<RuntimeMaterialObservation> {
        if !self.path.exists() {
            return Ok(RuntimeMaterialObservation {
                spec_digest: self.digest(),
                logical_id: self.logical_id.clone(),
                path: self.path.clone(),
                exists: false,
                observed_digest: None,
                expected_digest: self.expected_digest.clone(),
                status: if self.required {
                    RuntimeMaterialStatus::MissingRequired
                } else {
                    RuntimeMaterialStatus::OptionalMissing
                },
            });
        }

        let metadata = fs::metadata(&self.path).map_err(|error| RuntimeMaterialVerificationError::Io {
            logical_id: self.logical_id.clone(),
            path: self.path.clone(),
            message: error.to_string(),
        })?;

        if self.kind == RuntimeMaterialKind::Directory {
            return Ok(RuntimeMaterialObservation {
                spec_digest: self.digest(),
                logical_id: self.logical_id.clone(),
                path: self.path.clone(),
                exists: true,
                observed_digest: None,
                expected_digest: self.expected_digest.clone(),
                status: if metadata.is_dir() {
                    RuntimeMaterialStatus::Verified
                } else {
                    RuntimeMaterialStatus::WrongKind
                },
            });
        }

        if !metadata.is_file() {
            return Ok(RuntimeMaterialObservation {
                spec_digest: self.digest(),
                logical_id: self.logical_id.clone(),
                path: self.path.clone(),
                exists: true,
                observed_digest: None,
                expected_digest: self.expected_digest.clone(),
                status: RuntimeMaterialStatus::WrongKind,
            });
        }

        let bytes = fs::read(&self.path).map_err(|error| RuntimeMaterialVerificationError::Io {
            logical_id: self.logical_id.clone(),
            path: self.path.clone(),
            message: error.to_string(),
        })?;
        let observed_digest = StableDigest::sha256(bytes);
        let status = match &self.expected_digest {
            Some(expected) if expected == &observed_digest => RuntimeMaterialStatus::Verified,
            Some(_) => RuntimeMaterialStatus::DigestMismatch,
            None => RuntimeMaterialStatus::PresentUnhashed,
        };

        Ok(RuntimeMaterialObservation {
            spec_digest: self.digest(),
            logical_id: self.logical_id.clone(),
            path: self.path.clone(),
            exists: true,
            observed_digest: Some(observed_digest),
            expected_digest: self.expected_digest.clone(),
            status,
        })
    }
}

/// The observed contract state of a material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMaterialStatus {
    Verified,
    OptionalMissing,
    MissingRequired,
    DigestMismatch,
    PresentUnhashed,
    WrongKind,
}

impl RuntimeMaterialStatus {
    /// True when the material observation satisfies the spec. Optional missing
    /// materials satisfy the spec because absence is explicitly allowed.
    pub fn satisfies_contract(&self) -> bool {
        matches!(self, Self::Verified | Self::OptionalMissing)
    }
}

/// Observation produced by checking a material spec against local state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMaterialObservation {
    pub spec_digest: StableDigest,
    pub logical_id: String,
    pub path: PathBuf,
    pub exists: bool,
    pub observed_digest: Option<StableDigest>,
    pub expected_digest: Option<StableDigest>,
    pub status: RuntimeMaterialStatus,
}

impl RuntimeMaterialObservation {
    pub fn satisfies_contract(&self) -> bool {
        self.status.satisfies_contract()
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.material.observation.v1\n");
        push_stable_field(&mut material, "spec_digest", self.spec_digest.as_str());
        push_stable_field(&mut material, "logical_id", &self.logical_id);
        push_stable_field(&mut material, "path", &stable_path(&self.path));
        push_stable_field(&mut material, "exists", &self.exists.to_string());
        push_stable_field(
            &mut material,
            "observed_digest",
            self.observed_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "expected_digest",
            self.expected_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(&mut material, "status", &format!("{:?}", self.status));
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMaterialVerificationError {
    Io {
        logical_id: String,
        path: PathBuf,
        message: String,
    },
}

impl From<io::Error> for RuntimeMaterialVerificationError {
    fn from(value: io::Error) -> Self {
        Self::Io {
            logical_id: "unknown".to_string(),
            path: PathBuf::new(),
            message: value.to_string(),
        }
    }
}

pub type RuntimeMaterialVerificationResult<T> = Result<T, RuntimeMaterialVerificationError>;

/// Ordered material manifest for a verified runtime plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMaterialManifest {
    pub manifest_id: String,
    pub materials: Vec<RuntimeMaterialSpec>,
}

impl RuntimeMaterialManifest {
    pub fn new(manifest_id: impl Into<String>) -> Self {
        Self {
            manifest_id: manifest_id.into(),
            materials: Vec::new(),
        }
    }

    pub fn with_material(mut self, material: RuntimeMaterialSpec) -> Self {
        self.materials.push(material);
        self
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.material.manifest.v1\n");
        push_stable_field(&mut material, "manifest_id", &self.manifest_id);
        for (index, spec) in self.materials.iter().enumerate() {
            material.push_str("material[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(spec.digest().as_str());
            material.push('\n');
            material.push_str(&spec.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }

    pub fn verify(&self) -> RuntimeMaterialVerificationResult<RuntimeMaterialManifestReport> {
        let mut observations = Vec::with_capacity(self.materials.len());
        for material in &self.materials {
            observations.push(material.observe()?);
        }
        Ok(RuntimeMaterialManifestReport {
            manifest_digest: self.digest(),
            observations,
        })
    }
}

/// Verification report for a material manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMaterialManifestReport {
    pub manifest_digest: StableDigest,
    pub observations: Vec<RuntimeMaterialObservation>,
}

impl RuntimeMaterialManifestReport {
    pub fn is_fully_verified(&self) -> bool {
        self.observations
            .iter()
            .all(RuntimeMaterialObservation::satisfies_contract)
    }

    pub fn blocking_observations(&self) -> Vec<&RuntimeMaterialObservation> {
        self.observations
            .iter()
            .filter(|observation| !observation.satisfies_contract())
            .collect()
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.material.report.v1\n");
        push_stable_field(&mut material, "manifest_digest", self.manifest_digest.as_str());
        for (index, observation) in self.observations.iter().enumerate() {
            material.push_str("observation[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(observation.digest().as_str());
            material.push('\n');
            material.push_str(&observation.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

/// Link between a runtime operation plan and the material manifest that must be
/// verified before the plan is run by a connector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePlanMaterialContract {
    pub plan_id: String,
    pub plan_digest: StableDigest,
    pub manifest_id: String,
    pub manifest_digest: StableDigest,
}

impl RuntimePlanMaterialContract {
    pub fn new(plan: &RuntimeOperationPlan, manifest: &RuntimeMaterialManifest) -> Self {
        Self {
            plan_id: plan.plan_id.clone(),
            plan_digest: plan.digest(),
            manifest_id: manifest.manifest_id.clone(),
            manifest_digest: manifest.digest(),
        }
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.runtime.plan.material.contract.v1\n");
        push_stable_field(&mut material, "plan_id", &self.plan_id);
        push_stable_field(&mut material, "plan_digest", self.plan_digest.as_str());
        push_stable_field(&mut material, "manifest_id", &self.manifest_id);
        push_stable_field(&mut material, "manifest_digest", self.manifest_digest.as_str());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

fn stable_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
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
