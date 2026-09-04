use std::process::Command;

use crate::command::ShellCommand;

/// argv-based command with a subcommand *path* (multi-part) + args.
///
/// Example:
///   CommandSpec::new("git")
///     .path(["remote", "add"])
///     .arg("origin")
///     .arg("https://…");
#[derive(Debug, Clone, Default)]
pub struct CommandSpec {
    pub program: String,
    pub path: Vec<String>,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Default::default()
        }
    }

    /// Append one path segment (a subcommand word).
    #[allow(clippy::should_implement_trait)]
    pub fn sub(mut self, part: impl Into<String>) -> Self {
        self.path.push(part.into());
        self
    }

    pub fn path<I, S>(mut self, parts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.path.extend(parts.into_iter().map(Into::into));
        self
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
}

impl ShellCommand for CommandSpec {
    fn build(&self) -> String {
        let mut parts = vec![self.program.clone()];
        parts.extend(self.path.iter().cloned());
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }

    /// Crucially: no `sh -c`.
    fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.path);
        cmd.args(&self.args);
        cmd
    }
}
