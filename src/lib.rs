//! Runtime-facing surface for xccute.
//!
//! `xccute_runtime` owns execution/runtime concerns: command execution shims,
//! status/error handling, deterministic operation identities, exit-status
//! decisions, and verified operation receipts. Concrete command catalogs feed
//! this crate; they should not own runtime policy or chain semantics.

pub mod command;
pub mod command_spec;
pub mod connector;
pub mod decision_guide;
pub mod execution_gate;
pub mod material;
pub mod observation;
pub mod plan_transition;
pub mod prelude;
pub mod runner;
pub mod runtime_command;
pub mod status;
pub mod verified_operation;

pub use command::*;
pub use command_spec::*;
pub use connector::*;
pub use decision_guide::*;
pub use execution_gate::*;
pub use material::*;
pub use observation::*;
pub use plan_transition::*;
pub use prelude::*;
pub use runner::*;
#[allow(unused_imports)]
pub use runtime_command::*;
#[allow(unused_imports)]
pub use status::*;
pub use verified_operation::*;
