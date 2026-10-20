# SDD ledger — plan: docs/plans/2026-10-19/05-performance-concorrencia/README.md

## Escopo e preflight
- Branch: `perf/performance-concorrencia`, criada a partir de `main` em `be6e73d`.
- Plano README seleciona PERF-11, PERF-03, PERF-02, PERF-10, PERF-04, PERF-07, PERF-06 e PERF-09. PERF-01/05/08 marcados como NÃO EXECUTAR serão ignorados.
- Spec indicada: `docs/reviews/2026-10-19/05-performance-concorrencia.md`; validar presença e ler antes da implementação.
- Dependências compartilhadas: PERF-03 produz `tests/perf_render.rs`, `box_header`, `render_blocks_limited` e `Cow`; PERF-02 e PERF-10 consomem esse teste/helpers. PERF-02 depende de PERF-03. PERF-04 e PERF-09 tocam event loop/canais e serão executados na ordem do README; conferir assinatura no código após cada alteração. PERF-11, PERF-07 e PERF-06 são independentes conforme o README.
- Regra de execução: um commit por plano concluído; executar as verificações do plano e `cargo clean` ao fim de cada tarefa.


Task 1: complete (PERF-11; benchmark RED 125.3 ms vs 60.8 ms, GREEN 70.1 ms vs 67.3 ms; cargo test protocol 69 passed; cargo clippy --all-targets -- -D warnings passed; cargo test passed: 584 unit tests plus integrations)

Task 2: complete (PERF-03; grid benchmark RED 27.891 ms/frame, GREEN 1.587 ms/frame; display_blocks 4 passed; widget_view 29 passed; cargo test 590 unit tests plus integrations passed; clippy passed)
Ruling: snippets de widget_box/render no PERF-03 referiam-se a imports e flags antigos (`domain::layout`, `selected: bool`); mantive as APIs atuais (`presentation::grid_layout`, `Emphasis`, `FailureDetail`) e adaptei as mudanças ao fluxo existente — a semântica e os limites do plano foram preservados — custo se errado: revisão manual dos helpers por mudança recente de arquitetura.

Task 3: complete (PERF-02; baseline 51.779 ms/100k, 5.231 ms/10k, 53.217 ms/selection at end; after 0.293 ms, 0.294 ms, 1.180 ms; render unit tests 48 passed; final cargo test 594 unit tests plus integrations passed; clippy passed)

Task 4: complete (PERF-10; key benchmark RED 210.93 us/key, GREEN 0.03 us/key; app_state 62 passed; cargo test 594 unit tests plus integrations passed; clippy and build passed)
Ruling: app_state já usa o helper `WidgetRecord::usable_snapshot` descrito no README como mudança de outra trilha; preservei esse helper e apliquei `detail_sections`, contagem por seção e referências às ações nele — a regra de falhas continua centralizada no snapshot utilizável — custo se errado: possível mudança no conjunto de itens/ações disponíveis durante falha.

Task 5: complete (PERF-04; redraw policy 4 passed; baseline 45 frames/10s, after 8; q exit=0 and no remaining process; release build, cargo test 598 unit tests plus integrations passed, clippy passed)
Ruling: plano PERF-04 descrevia o loop funcional antigo, mas `main.rs` já encapsula o estado em `App::run_event_loop`; integrei recv_timeout, fila de eventos e política de desenho nesse método e mantive o despacho existente em `handle_input` — preserva o comportamento de comandos atual — custo se errado: regressão em uma ramificação pouco exercitada do ciclo de eventos.
Ruling: a medição de terminal e sua configuração foram redirecionadas de `/tmp/perf04` para `.superpowers/sdd/05-performance-concorrencia/perf04` dentro do repositório, por causa da restrição do usuário — mesmos 12 widgets, duração e contagem de quadros — custo se errado: scripts/arquivos de medição permanecem apenas no workspace local.

Task 6: complete (PERF-07; baseline p50 50.565511 ms, after 3.581302 ms; process tests passed 3 consecutive runs at 36/36; cargo test 598 unit tests plus integrations passed; clippy passed)

Task 7: complete (PERF-06; concurrency gate 3 passed; peak without gate 188.6 MiB, with 3 parses 51.4 MiB; cargo test 601 unit tests plus integrations passed; clippy passed)

Task 8: complete (PERF-09; widget_runtime tests 5 passed; bounded-queue test failed as expected at capacity 1000; cargo build and cargo test 602 unit tests plus integrations passed; clippy passed; interactive q exit=0)
Ruling: o trecho de teste PERF-09 usa `runtime.start(..., 1)`, mas o repositório tipa gerações como `Generation`; usei o helper `generation()` existente — mantém o teste alinhado com a API real — custo se errado: o teste passaria a exercitar uma geração diferente da pretendida.

## Revisão final
- Important corrigido: falha em `event::read()` agora vira `AppEvent::TerminalReadFailed` e é propagada como erro de terminal pelo loop principal. Teste de regressão primeiro falhou por ausência do helper/variante, depois passou; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passaram.
- Minor corrigido: `propose` agora recebe `Option<ActionSpec>` e move o valor para `ProposedAction`, eliminando o clone adicional em Enter. Teste de transferência de propriedade falhou inicialmente por incompatibilidade do tipo de referência e passou após a mudança.
