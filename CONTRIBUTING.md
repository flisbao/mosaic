# Contribuindo com o Mosaic

## Plugin novo

Um plugin é uma pasta em `plugins/` com o código, o README, os testes e a **skill** que ensina um agente (ou uma pessoa) a configurá-lo e a alterá-lo. A skill é pré-requisito: sem ela o plugin não é aceito.

1. Leia o [`PROTOCOL.md`](PROTOCOL.md) e a skill [`mosaic-widget-authoring`](.claude/skills/mosaic-widget-authoring/SKILL.md). Ela reúne os requisitos do Mosaic e do protocolo (saída única em JSON, timeout, limite de 1 MiB, segredo fora de `args`, erro só daquele widget) e o passo a passo, em ordem de TDD.
2. Crie `plugins/<pasta>/<nome>.py` (só biblioteca padrão), `plugins/<pasta>/README.md` e `plugins/tests/test_<nome>.py`. Teste primeiro.
3. Escreva `.claude/skills/mosaic-plugin-<pasta>/SKILL.md` a partir do [template](.claude/skills/mosaic-widget-authoring/plugin-skill-template.md). A skill é específica do plugin: a entrevista de configuração (que pergunta fazer e o que vira `mosaic.toml`), a verificação com `mosaic check`, as invariantes do código e as armadilhas. As skills de [Jira](.claude/skills/mosaic-plugin-jira/SKILL.md) e [GitHub](.claude/skills/mosaic-plugin-github/SKILL.md) servem de exemplo.
4. Registre o plugin na instalação padrão (`src/infrastructure/default_files.rs`).
5. Antes de abrir o pedido:

```sh
python3 -m unittest discover -s plugins/tests   # inclui a checagem das skills
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

A checagem das skills (`plugins/tests/test_skills.py`) falha se um plugin não tiver skill, se a skill estiver mal formada, se citar um arquivo que não existe ou uma variável que o plugin não conhece.

As skills são arquivos Markdown comuns. O Claude Code as carrega sozinho a partir de `.claude/skills/`; com outros agentes, aponte-os para essa pasta.

## Segredos

Nunca coloque token, link secreto ou senha em `mosaic.toml`, em argumento de comando, em issue ou em pedido de contribuição. O plugin lê o **nome** de uma variável de ambiente ou o caminho de um arquivo.
