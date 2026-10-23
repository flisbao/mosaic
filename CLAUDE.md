# Regras

- Rust idiomático; clean architecture: `domain` ← `application` ← `infrastructure`/`presentation`. Dependências apontam só para dentro.
- Domain sem I/O nem crates de framework. Fronteiras via `trait` (ports); adapters na infraestrutura.
- SOLID: uma responsabilidade por tipo/módulo; depender de traits, não de tipos concretos; injeção de dependência no `main`.
- Clean Code: nomes que revelam intenção (sem abreviações), funções pequenas, sem comentários óbvios, sem números mágicos.
- Erros com `Result` e tipos próprios (`thiserror`); sem `unwrap`/`expect` fora de testes.
- Testes unitários no domain/application com fakes dos ports.
- Termo oficial: **widget** (nunca "card") no código, no schema e na documentação. Um widget é um comando completo (script + argumentos) com intervalo próprio; `python3 jira.py my-tasks` e `python3 jira.py my-team` são dois widgets independentes, cada um com `id` único, `command` como lista de argumentos (sem shell) e `interval`. Cada execução devolve um único widget.
- Providers: processos externos executados pelo mosaic a cada intervalo do widget (poll). Cada execução de `snapshot` imprime um JSON no stdout, conforme o schema do protocolo, com stdin fechado, e termina ("imprime e sai"). Ações recebem os parâmetros JSON pelo stdin. Sem processo de longa duração e sem canal de volta para o provider nesta fase.
- Execução: cada widget tem uma thread própria que coleta, envia o resultado por `std::sync::mpsc` ao loop de eventos e só então aguarda o intervalo; não há tokio. O render nunca espera um provider, e as execuções de um mesmo widget não se sobrepõem. Cada execução tem timeout (`timeout_seconds`) e limite de 1 MiB (`1_048_576` bytes) na saída padrão. No timeout, no estouro de saída, no cancelamento (recarga que reinicia/remove o widget e saída da aplicação) o mosaic encerra o grupo de processos do provider com `SIGKILL`. Falha (JSON inválido, exit code diferente de zero, timeout, saída acima do limite) vira estado de erro só daquele widget, preservando o último dado bom.
- Ports em `application/ports.rs`: `WidgetSource` (coleta, com supertrait `Cancellable`) e `ActionExecutor` (ações). O adapter com `Command` (`PluginRunner`) fica na infraestrutura, e os testes usam fakes.
- Antes de concluir: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`.
- Ao final de cada tarefa, depois das checagens: `cargo clean`.
- Plugin novo só entra com a sua skill em `.claude/skills/mosaic-plugin-<pasta>/SKILL.md`, escrita pelo template de `mosaic-widget-authoring`. Use essa skill ao criar ou alterar qualquer widget; `python3 -m unittest discover -s plugins/tests` falha sem ela.
