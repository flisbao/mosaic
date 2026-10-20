# SDD ledger — plan: docs/plans/2026-10-19/04-seguranca/README.md

Workspace: branch codex/04-seguranca inside the repository, per user constraint against other worktrees.

Ruling: Omit SEC-06 implementation — the plan explicitly says not to execute because it requires unsafe; the adjacent SEC-02 restricted environment provides only its stated partial mitigation — cost if wrong: providers may still inherit the controlling terminal.

Pre-flight: SEC-02 and SEC-08 both change config.rs imports; sequence SEC-02 before SEC-08 and merge imports when editing. SEC-02 and SEC-10 touch separate phrases on PROTOCOL.md line 23; preserve both. SEC-04 touches protocol_report.rs while SEC-08 touches config.rs, no overlap. Existing AR-01, AR-12, AR-07 commits satisfy SEC-01/07/10 delta preconditions.

Task SEC-02: Ruling: Adapt WidgetSettings/WidgetIdentity to WidgetLaunchSpec/ProviderCommand and split config modules — repository architecture already moved these contracts; cost if wrong: environment policy may be carried at an unintended boundary.
Task SEC-02: Ruling: Add ConfigError::InvalidValue — the plan assumes this variant exists but current code has no equivalent generic invalid value; use a clear validation message. Cost if wrong: downstream callers may expect a different configuration error variant.

Task SEC-02: complete (commit 57ad0c3; tests cargo test + cargo clippy --all-targets -- -D warnings passed; an initial suite run exposed malformed JSON in the new shell test, corrected before the green run).
Task SEC-08: complete (commit bd35d64; cargo test and clippy passed).
Task SEC-04: implementation complete (commit 035f45c; cargo test and clippy passed; repository now uses modular config and WidgetLaunchSpec, so the specified insertion points were adapted).
Task SEC-05: complete (commit 91661f4; cargo test and clippy passed).
Task SEC-03: complete (commit 8f1413d; cargo test and clippy passed).
Task SEC-09: complete (commit 0021e30; Python suite 150 tests, cargo test, and clippy passed).
Delta SEC-07: complete (commit 5b84b27; real process group test passed).
Delta SEC-10: complete (commit cf6f704; global Rust verification passed).
Delta SEC-01: complete (commit a587911; all 36 process tests, full Rust suite, and clippy passed).
Final review: self-review (no subagent tool allowed by collaboration policy). Global verification passed: cargo fmt --check, cargo clippy --all-targets -- -D warnings, cargo test, plugins Python suite (150 tests). `unsafe` grep only matches the existing test function name `safe_process_group_never_targets_unsafe_ids`.

SEC-06 review: confirmed plan status NÃO EXECUTAR and its explicit no-code-change acceptance criterion. `process_group(0)` preserves the controlling session; complete detachment needs `pre_exec`/unsafe on the pinned stable toolchain. No changes made per plan; retain the accepted residual risk, with SEC-02 environment variables as only partial mitigation.
