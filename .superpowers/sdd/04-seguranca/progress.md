# SDD ledger — plan: docs/plans/2026-10-19/04-seguranca/README.md

Workspace: branch codex/04-seguranca inside the repository, per user constraint against other worktrees.

Ruling: Omit SEC-06 implementation — the plan explicitly says not to execute because it requires unsafe; the adjacent SEC-02 restricted environment provides only its stated partial mitigation — cost if wrong: providers may still inherit the controlling terminal.

Pre-flight: SEC-02 and SEC-08 both change config.rs imports; sequence SEC-02 before SEC-08 and merge imports when editing. SEC-02 and SEC-10 touch separate phrases on PROTOCOL.md line 23; preserve both. SEC-04 touches protocol_report.rs while SEC-08 touches config.rs, no overlap. Existing AR-01, AR-12, AR-07 commits satisfy SEC-01/07/10 delta preconditions.

Task SEC-02: Ruling: Adapt WidgetSettings/WidgetIdentity to WidgetLaunchSpec/ProviderCommand and split config modules — repository architecture already moved these contracts; cost if wrong: environment policy may be carried at an unintended boundary.
Task SEC-02: Ruling: Add ConfigError::InvalidValue — the plan assumes this variant exists but current code has no equivalent generic invalid value; use a clear validation message. Cost if wrong: downstream callers may expect a different configuration error variant.
