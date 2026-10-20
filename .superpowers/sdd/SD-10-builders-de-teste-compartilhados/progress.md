# SDD ledger — plan: /home/flisbao/projetos/mosaic/docs/plans/2026-10-19/02-solid-dry/SD-10-builders-de-teste-compartilhados.md

Tasks 1-3 complete: added Summary/WidgetSnapshot titled builders, replaced duplicated empty snapshots, and centralized test clocks in src/test_support.rs and tests/common/mod.rs. `titled_snapshot` test saw expected missing-method RED then passed; date helper test passed. Full suite: 571 lib + 5 bin + 28 integration = 604 passed; clippy/fmt/diff-check clean. `fn now()` remains only in render.rs.

Ruling: SD-10's current branch also contains unique fixed test clocks in recommendation_request.rs (2026-10-19 09:00) and widget_box.rs (2026-10-17 09:41), absent from the plan's two named values. Kept their exact timestamps in named shared helpers so test behavior remains the same and the acceptance grep can still pass. Cost if wrong: two extra fixed-clock helpers may be unnecessary later.
