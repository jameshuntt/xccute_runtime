//! Marker traits for commands that can participate in the runtime path.

use xccute_contract::{CompositeShellCommand, ValidatedCommand};

/// A command that can enter the xccute runtime pipeline.
///
/// This is intentionally a marker over the existing generated contract traits.
/// Later passes can hang runtime-specific adapters here without forcing command
/// catalog crates to depend on the facade crate.
pub trait RuntimeCommand: CompositeShellCommand + ValidatedCommand {}

impl<T> RuntimeCommand for T where T: CompositeShellCommand + ValidatedCommand {}
