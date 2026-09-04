use std::{io, process::Output};

#[derive(Debug)]
pub enum CommandError {
    Io(io::Error),

    /// Non-success exit (or not accepted by policy), with captured output (if available).
    NonZeroExit {
        cmd: String,
        code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },

    /// Kept for compatibility with your existing API surface.
    ExitFailure(Option<i32>),

    /// Kept for compatibility (string-only failure).
    ExecutionFailed(String),
}

impl CommandError {
    pub fn from_output(cmd: String, output: &Output) -> Self {
        Self::NonZeroExit {
            cmd,
            code: output.status.code(),
            stdout: output.stdout.clone(),
            stderr: output.stderr.clone(),
        }
    }

    fn preview(bytes: &[u8], max: usize) -> String {
        let s = String::from_utf8_lossy(bytes);
        if s.len() <= max {
            s.to_string()
        } else {
            format!("{}…", &s[..max])
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Io(e) => write!(f, "IO error: {}", e),
            CommandError::NonZeroExit {
                cmd,
                code,
                stdout,
                stderr,
            } => {
                write!(
                    f,
                    "Command failed (code={code:?}): {cmd}\nstdout: {}\nstderr: {}",
                    Self::preview(stdout, 8_192),
                    Self::preview(stderr, 8_192),
                )
            },
            // CommandError::ExitFailure(code) => write!(f, "Command exited with code: {:?}", code),
            // CommandError::ExecutionFailed(cmd) => write!(f, "Command failed: {}", cmd),
            CommandError::ExitFailure(code) => write!(f, "Command exited with code: {code:?}"),
            CommandError::ExecutionFailed(cmd) => write!(f, "Command failed: {cmd}"),

        }
    }
}

impl std::error::Error for CommandError {}

impl From<io::Error> for CommandError {
    fn from(err: io::Error) -> Self {
        CommandError::Io(err)
    }
}
