# Changelog

## 0.1.2 (2026-10-18)

`ExitStatusPolicy` takes its rules one at a time, so a program that names a command's statuses wrote the same fold every time: each named status a rule, accepted ones going on, the rest stopping, the word as the reason. xccute_contract 0.1.2 lets a command name its statuses; this makes them the policy.

### what's new

- the policy had only `new`, `with_rule` and `decide`: [src/verified_operation.rs L208-L234](https://github.com/jameshuntt/xccute_runtime/blob/aff94b2fd94e8454b1ea8c977d4e3c6009961d33/src/verified_operation.rs#L208-L234). `ExitStatusPolicy::named(&[ExitStatusWord])` makes the rules in order, an accepted status `accepted_code(code, Continue, word)` and any other `rejected_code(code, Stop, word)`, and `ExitStatusPolicy::of(&command)` does it for the statuses the command names: [src/verified_operation.rs L234-L250](https://github.com/jameshuntt/xccute_runtime/blob/61b23d49966603e8e3e3c697869596ee66e1ebc9/src/verified_operation.rs#L234-L250).
- a status the command does not name falls to the policy's defaults, as before. the codes a derived command accepts already reach this crate's `ShellCommand` through the blanket impl's `ok_codes`, so a runner's `accepted` and the rules agree.

### proof

- systemctl is-active's 0 active and 3 inactive, named on the command, are its policy's two rules; 0 goes on with "active", 3 stops with "inactive", 1 falls to the default stop; naming nothing is the default policy: [tests/exit_status_policy_contract.rs L29-L51](https://github.com/jameshuntt/xccute_runtime/blob/61b23d49966603e8e3e3c697869596ee66e1ebc9/tests/exit_status_policy_contract.rs#L29-L51).
- `cargo test`: 33 passed (stable and 1.78).

### Cargo

- version 0.1.1 to 0.1.2, xccute_contract 0.1.2, and CHANGELOG.md is in the package: [Cargo.toml L3](https://github.com/jameshuntt/xccute_runtime/blob/61b23d49966603e8e3e3c697869596ee66e1ebc9/Cargo.toml#L3-L3), [Cargo.toml L13](https://github.com/jameshuntt/xccute_runtime/blob/61b23d49966603e8e3e3c697869596ee66e1ebc9/Cargo.toml#L13-L13), [Cargo.toml L16](https://github.com/jameshuntt/xccute_runtime/blob/61b23d49966603e8e3e3c697869596ee66e1ebc9/Cargo.toml#L16-L16).
