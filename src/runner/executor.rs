use crate::command::ShellCommand;
use crate::runner::error::CommandError;
use std::process::Output;

#[derive(Debug, Clone, Copy, Default)]
pub enum CaptureMode {
    #[default]
    /// Use `.status()` and inherit stdout/stderr (streams to console).
    StatusOnly,
    /// Use `.output()` and capture stdout/stderr (enables rich introspection).
    Output,
}

pub struct CommandChainExecutor {
    pub commands: Vec<Box<dyn ShellCommand>>,
    pub dry_run: bool,
    pub stop_on_error: bool,
    pub capture: CaptureMode,
}

impl Default for CommandChainExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandChainExecutor {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            dry_run: false,
            stop_on_error: true,
            capture: CaptureMode::default(),
        }
    }

    pub fn with_dry_run(mut self, dry: bool) -> Self {
        self.dry_run = dry;
        self
    }

    pub fn with_stop_on_error(mut self, stop: bool) -> Self {
        self.stop_on_error = stop;
        self
    }

    pub fn with_capture(mut self, capture: CaptureMode) -> Self {
        self.capture = capture;
        self
    }

    pub fn add_command<T: ShellCommand + 'static>(mut self, command: T) -> Self {
        self.commands.push(Box::new(command));
        self
    }

    pub fn run(&self) -> Result<(), CommandError> {
        for cmd in &self.commands {
            let command_str = cmd.build();

            if self.dry_run {
                println!("[dry-run] {command_str}");
                continue;
            }

            let mut proc = cmd.to_command();

            // Unify on Output so `handle_output()` always runs.
            let output: Output = match self.capture {
                CaptureMode::Output => proc.output()?,
                CaptureMode::StatusOnly => {
                    let status = proc.status()?;
                    Output {
                        status,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    }
                }
            };

            if let Err(e) = cmd.handle_output(&output) {
                if self.stop_on_error {
                    return Err(e);
                } else {
                    eprintln!("{e}");
                }
            }
        }

        Ok(())
    }
}
