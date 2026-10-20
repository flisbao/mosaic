# SDD ledger — plan: /home/flisbao/projetos/mosaic/docs/plans/2026-10-19/02-solid-dry/README.md

Execution workspace: native worktree `/home/flisbao/.codex/worktrees/7909/mosaic`; branch `refactor/solid-dry-2026-10-20`; base `fffbb3f15a538d6d9448f940da77f2166e2b9634`.

Baseline: `cargo test` passed on 2026-10-20 (539 lib + 7 bin + 28 integration = 574; doc tests 0).

Plan order follows README, not numeric order. Skips: SD-08 (covered by CC-04), SD-11 (covered by CC-06), SD-04 T1 (CC-13), SD-05 T2 (CC-05), SD-06 T1 (CC-01), SD-07 T1/T2 (CC-03), SD-14 T2 (CC-13) and T3 (CC-10), and CC-11 T3 is irrelevant to these SD plans. These are explicit reconciliations in README/individual plans.

Pre-flight shared interfaces:
- SD-13 → SD-03: `AppPaths`, constants/text separator and config path exports; inspect what SD-03 consumes after SD-13.
- SD-02 → SD-03: `WidgetLaunchSpec` replaces `WidgetIdentity`; config reload planner imports must follow the new type.
- SD-13 + SD-02 → SD-10: shared text constants and builders after config/settings type changes.
- SD-05 T1 → SD-04: `HasCancelFlag` generic `Cancellable` applies to the LLM runner trait object/factory.
- SD-06 T2 → SD-14 T1 → SD-15: pending action storage, explicit modal transitions, reserved widget ID must compose.
- SD-07 T3/T4 → SD-12: block parser modules and `Flag` parsing; apply enum mappings after shared parser refactor.
- SD-09 independent.
- SD-01 composition change follows README last among code plans; adapt to current `App` and CC-10 event shape.
- SD-03 → SD-10: config module exports and widget test support.

Ruling: run only the SD plans in the requested directory, honor their reconciliations for CC-covered work, and adapt their code to the already-merged CC branch. AR plans mentioned as context in the README are not part of the requested directory; do not silently implement separate AR scopes. Cost if wrong: a narrow integration adjustment or follow-up AR task may be needed.

Task sequence (todos): SD-07; SD-12; SD-09; SD-13; SD-02; SD-03; SD-05/T1; SD-04 delta; SD-06/T2; SD-14/T1; SD-15; SD-01; SD-10. SD-08 and SD-11 are fully covered and skipped.

Ruling: o extrator `task-start/task-brief` não reconhece cabeçalhos portugueses `## Tarefa N` (busca literalmente `## Task N`), e os planos estão fora do worktree por `docs/` estar ignorado; usar caminho absoluto aos planos originais e gerar briefs fiéis por extração entre cabeçalhos `## Tarefa N`/próximo `##`, mantendo os arquivos de origem intactos. Custo se errado: algum limite de tarefa pode exigir reextração, conferida contra o original.

SD-07 complete (commit fffbb3f..HEAD; tests: baseline characterization 55 protocol_report tests; after refactor: 55 protocol_report, protocol_blocks check passed; full cargo test 574/574; clippy clean; fmt; grep confirms no capped in protocol_blocks; cargo clean).
SD-07 Task 3 Ruling: pure loop extraction had existing characterization coverage and no behavior change was specified, so ran affected tests before and after instead of adding a test expected to fail against equivalent behavior. Cost if wrong: a subtle behavior difference could have lacked a new targeted assertion.
SD-07 Task 3 Ruling: CC-03's parse_each was private despite SD-07 requiring sibling protocol-block modules to call it; widened visibility to pub(super), the narrowest shared scope. Cost if wrong: a larger internal API surface, still confined to infrastructure.

SD-12 complete: tone/flag/block/phase names now have a single enum/table source as specified; commit recorded in per-plan ledger; 578 tests, clippy/fmt passed.
Ruling: SD-12 Task 1 declaration-order test already passed on old implementation; used it as characterization, not a synthetic failure. Cost if wrong: doesn't independently prove internal table relation, but runtime parse tests and block-kind relation cover mapping behavior.
SD-09 complete: centralized whitespace collapsing, character truncation, and ellipsis in domain/widget_content; recommendation and text_fit reuse them. Tests: 545 lib + 7 bin + 28 integration = 580 passed; clippy/fmt/diff-check passed.
SD-13 complete: centralized config/state/app paths and domain text/time constants; full cargo test 548 lib + 7 bin + 28 integration = 583 passed; cargo build/clippy/fmt/checks passed. Its documented recommended order names SD-02 first, while the aggregate ledger listed SD-13 before SD-02 to satisfy the SD-13→SD-03 config-path dependency; SD-02 remains independent of SD-13 and is next.
SD-02 complete: created ProviderCommand/WidgetSchedule/WidgetLaunchSpec, projected WidgetSettings, made reload comparison operate on launch specs, and changed PluginRunner/runtime/check/tests to use the execution contract. TDD projection test observed expected missing-method RED then passed; full suite 586 passed; clippy/fmt/checks passed.
SD-03 complete: split config DTOs, validation and path APIs into config/{raw,validation,paths}.rs; moved RestartSections into application and reload notice into presentation. config/paths delegates the directory source to SD-13 app_paths. Full tests 589 passed; build/clippy/fmt/checks passed.
SD-05 delta complete: added HasCancelFlag and the blanket Cancellable implementation, replacing concrete implementations on PluginRunner, CommandNotifier and LlmCli. New generic delegation test observed missing-trait RED then passed; full suite 590 passed; clippy/fmt/checks clean. SD-05 Task 2 remains skipped as reconciled: CC-05 already implemented the generic CLI runner.
SD-04 delta complete: retained CC-13 harness names, added CancellableLlmRunner and build_llm_runner, and simplified main to use the factory. Factory test for each harness passed; full suite 591 passed; clippy/fmt/checks clean. SD-04 Task 1 was skipped per its CC-13 reconciliation.
SD-06 delta complete: CC-01 already supplied AlertQueue; extracted the pending-action HashSet behind PendingActions and preserved the AppState facade. Both PendingActions tests failed against the provisional no-op then passed. Full suite 593 passed; clippy/fmt/checks clean.
SD-14 delta complete: centralized confirmation key interpretation in confirmation_answer; adapted CC-01 Transition handlers and SD-06 PendingActions. Task 2 is covered by CC-13; Task 3 is covered by CC-10. Confirmation classifier test observed expected missing-function RED then passed; full suite 594 passed; clippy/fmt/checks clean.
SD-15 complete: recommendation widget identity now lives in domain/WidgetId, AppState exposes recommendation_record, and config rejects the reserved `ia` id. Domain and config tests observed their expected RED states then passed; full suite 597 passed; clippy/fmt/checks clean.
