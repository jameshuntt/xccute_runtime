use std::ffi::{OsStr, OsString};

use xccute_contract::{CompositeShellCommand, ExitStatusWord};
use xccute_runtime::{ExitDisposition, ExitStatusPolicy, ExitStatusRule, RuntimeExitStatus};

/// `systemctl is-active UNIT`: 0 active goes on, 3 inactive stops.
struct IsActive;

impl CompositeShellCommand for IsActive {
    fn program(&self) -> &OsStr {
        OsStr::new("systemctl")
    }

    fn push_args(&self, out: &mut Vec<OsString>) {
        out.push("is-active".into());
    }

    fn exit_statuses(&self) -> &'static [ExitStatusWord] {
        const NAMED: &[ExitStatusWord] = &[ExitStatusWord::accepted(0, "active"), ExitStatusWord::rejected(3, "inactive")];
        NAMED
    }
}

/// What the command accepts decides `accepted`, as a runner reads it.
fn ran(command: &impl CompositeShellCommand, code: i32) -> RuntimeExitStatus {
    RuntimeExitStatus::new(Some(code), command.ok_codes().contains(&code))
}

#[test]
fn a_commands_named_statuses_are_its_policy() {
    let policy = ExitStatusPolicy::of(&IsActive);
    assert_eq!(
        policy.rules,
        [
            ExitStatusRule::accepted_code(0, ExitDisposition::Continue, "active"),
            ExitStatusRule::rejected_code(3, ExitDisposition::Stop, "inactive"),
        ]
    );
    let active = policy.decide(ran(&IsActive, 0));
    assert_eq!((active.disposition, active.reason.as_str()), (ExitDisposition::Continue, "active"));
    let inactive = policy.decide(ran(&IsActive, 3));
    assert_eq!((inactive.disposition, inactive.reason.as_str()), (ExitDisposition::Stop, "inactive"));
    // a status it does not name falls to the policy's defaults
    let other = policy.decide(ran(&IsActive, 1));
    assert_eq!((other.disposition, other.reason.as_str()), (ExitDisposition::Stop, "unaccepted status: stop"));
}

#[test]
fn nothing_named_is_the_default_policy() {
    assert_eq!(ExitStatusPolicy::named(&[]), ExitStatusPolicy::new());
}
