# Regras

- Rust idiomático; clean architecture: `domain` ← `application` ← `infrastructure`/`presentation`. Dependências apontam só para dentro.
- Domain sem I/O nem crates de framework. Fronteiras via `trait` (ports); adapters na infraestrutura.
- SOLID: uma responsabilidade por tipo/módulo; depender de traits, não de tipos concretos; injeção de dependência no `main`.
- Clean Code: nomes que revelam intenção (sem abreviações), funções pequenas, sem comentários óbvios, sem números mágicos.
- Erros com `Result` e tipos próprios (`thiserror`); sem `unwrap`/`expect` fora de testes.
- Testes unitários no domain/application com fakes dos ports.
- Antes de concluir: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`.
- Ao final de cada tarefa, depois das checagens: `cargo clean`.
