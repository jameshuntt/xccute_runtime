use std::process::{Command, ExitStatus, Output};

use crate::runner::error::CommandError;

/// The minimal interface for a typed “command spec” in xccute.
///
/// The default execution is `sh -c <build()>`; a command can override
/// `to_command()` to become argv-based instead.
pub trait ShellCommand {
    /// A human-readable command string for logging, snapshotting, dry-run, etc.
    fn build(&self) -> String;

    /// Default execution strategy: `sh -c <build()>`.
    /// Override this for argv-based commands.
    fn to_command(&self) -> Command {
        let built = self.build();
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(built);
        cmd
    }

    /// Acceptable exit codes for this command.
    /// Default: only 0 is success.
    fn ok_codes(&self) -> &'static [i32] {
        &[0]
    }

    /// Default “is success?” logic. Override if you want signal-aware logic on unix, etc.
    fn accept_status(&self, status: &ExitStatus) -> bool {
        match status.code() {
            Some(code) => self.ok_codes().contains(&code),
            None => status.success(), // e.g., terminated by signal
        }
    }

    /// The hook: gets the full Output (status + stdout/stderr if captured).
    ///
    /// Default behavior:
    /// - if status accepted => Ok
    /// - else => CommandError::NonZeroExit { cmd, code, stdout, stderr }
    fn handle_output(&self, output: &Output) -> Result<(), CommandError> {
        if self.accept_status(&output.status) {
            Ok(())
        } else {
            Err(CommandError::from_output(self.build(), output))
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawCommand(pub String);

impl ShellCommand for RawCommand {
    fn build(&self) -> String {
        self.0.clone()
    }
}


/// Adapter: every argv-safe composite command can also be used wherever the
/// existing `ShellCommand` executor expects a command.
///
/// The string returned by `build()` is display/debug only. `to_command()` uses
/// the argv-safe composite command path.
impl<T> ShellCommand for T
where
    T: xccute_contract::CompositeShellCommand,
{
    fn build(&self) -> String {
        xccute_contract::CompositeShellCommand::build_display(self)
    }

    fn to_command(&self) -> Command {
        xccute_contract::CompositeShellCommand::to_std_command(self)
    }

    fn ok_codes(&self) -> &'static [i32] {
        xccute_contract::CompositeShellCommand::ok_codes(self)
    }
}
