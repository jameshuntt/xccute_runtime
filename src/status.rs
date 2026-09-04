use std::borrow::Cow;
use std::process::Output;

pub trait OutputExt {
    fn code(&self) -> Option<i32>;
    fn stdout_text(&self) -> Cow<'_, str>;
    fn stderr_text(&self) -> Cow<'_, str>;
}

impl OutputExt for Output {
    fn code(&self) -> Option<i32> {
        self.status.code()
    }

    fn stdout_text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }

    fn stderr_text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.stderr)
    }
}
