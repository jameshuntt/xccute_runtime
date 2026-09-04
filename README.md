# xccute_runtime

The execution half of [xccute](https://crates.io/crates/xccute).

- `ShellCommand`: `build()` renders a command string; `to_command()` turns it
  into a `std::process::Command` (`sh -c` by default, argv for composite
  commands through the blanket impl over `xccute_contract`); `ok_codes()`,
  `accept_status()` and `handle_output()` decide what counts as success.
- `RawCommand`: a string that is already a command.
- `CommandSpec`: a command with an identity and a stable digest.
- `runner::{CommandChainExecutor, CaptureMode, CommandError}`: run several
  commands in order, stop on error or not, dry run, capture output or stream it.
- The seams a supervising system uses before and after a run: connector
  calls and receipts, material manifests and verification, the execution gate,
  plan transitions, verified operations, and the decision guide.

Command builders live in [`xccute_commands`](https://crates.io/crates/xccute_commands);
this crate does not know any particular program.

## License

MIT OR Apache-2.0.
