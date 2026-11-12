# Contribuindo com o Mosaic

## Plugin novo

Um plugin é uma pasta em `plugins/` com o código, o README, os testes, a **skill** que ensina um agente (ou uma pessoa) a configurá-lo e a alterá-lo e o **roteiro** (`setup.toml`) do `mosaic plugins setup`. A skill e o roteiro são pré-requisito: sem eles o plugin não é aceito.

1. Leia o [`PROTOCOL.md`](PROTOCOL.md) e a skill [`mosaic-widget-authoring`](.claude/skills/mosaic-widget-authoring/SKILL.md). Ela reúne os requisitos do Mosaic e do protocolo (saída única em JSON, timeout, limite de 1 MiB, segredo fora de `args`, erro só daquele widget) e o passo a passo, em ordem de TDD.
2. Crie `plugins/<pasta>/<nome>.py` (só biblioteca padrão), `plugins/<pasta>/README.md` e `plugins/tests/test_<nome>.py`. Teste primeiro.
3. Escreva `plugins/<pasta>/SKILL.md` a partir do [template](.claude/skills/mosaic-widget-authoring/plugin-skill-template.md) e crie o link `.claude/skills/mosaic-plugin-<pasta>` → `../../plugins/<pasta>` (é por ele que o Claude Code a encontra no repositório). A skill é específica do plugin: como instalar e configurar com `mosaic plugins`, a entrevista de configuração (que pergunta fazer e o que vira `mosaic.toml`), a tabela de todas as opções, a verificação com `mosaic check`, as invariantes do código e as armadilhas. As skills de [Jira](plugins/jira/SKILL.md) e [GitHub](plugins/github/SKILL.md) servem de exemplo.
4. Escreva `plugins/<pasta>/setup.toml`, o roteiro do `mosaic plugins setup <pasta>`: só as variáveis e opções que o plugin **já** lê (a tabela do README é a fonte), com padrões, faixas, condições e modelos prontos do README. Nada de configuração que o plugin não implementa. `plugins/tests/test_setup_manifests.py` confere o arquivo com o README e com o código; `tests/plugin_manifests.rs` o executa.
5. Distribua pelo próprio GitHub: `mosaic plugins install owner/repo/subdir` instala uma pasta com script, skill e manifesto, sem pacote coletivo e sem embutir código no binário. Declare `version` e `min_mosaic_version` no `setup.toml` como versões estáveis de três números; incremente só a versão do plugin alterado. O mínimo acompanha recursos exigidos do núcleo. O campo `script` aponta ao caminho instalado `plugins/<pasta>/<arquivo>.py`; o arquivo fica diretamente na pasta selecionada no clone. Instalação por referência dispensa registro em Rust; para acrescentar um atalho oficial, altere `src/application/plugin_catalog.rs` e seus testes. Opcional: bloco comentado no `REFERENCE_CONFIG` e menção no README.
6. Antes de abrir o pedido:

```sh
python3 -m unittest discover -s plugins/tests   # inclui a checagem das skills
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

A checagem das skills (`plugins/tests/test_skills.py`) falha se um plugin não tiver skill, se a skill estiver mal formada, se citar um arquivo que não existe ou uma variável que o plugin não conhece, ou se deixar de fora uma opção do `setup.toml`.

As skills são arquivos Markdown comuns. O Claude Code as carrega sozinho a partir de `.claude/skills/` (os links das skills de plugin apontam para `plugins/<pasta>/`); com outros agentes, aponte-os para essa pasta.

## Publicar o tap do Homebrew (passo manual do dono)

A release anexa `mosaic.rb`, a fórmula gerada por `scripts/render-homebrew-formula.sh <versão> SHA256SUMS`. O tap é o repositório público `flisbao/homebrew-mosaic`, que o fluxo de release não cria nem atualiza sozinho. Em cada versão:

1. Baixe `mosaic.rb` da release (ou gere: `scripts/render-homebrew-formula.sh v0.0.11 SHA256SUMS > mosaic.rb`).
2. No clone do `homebrew-mosaic`, copie-o para `Formula/mosaic.rb`, faça o commit e o push.
3. Confira com `brew install flisbao/mosaic/mosaic` e `brew test mosaic` numa máquina com Homebrew.

Até o tap existir, o README descreve o Homebrew como "disponível quando o tap for publicado".

## Segredos

Nunca coloque token, link secreto ou senha em `mosaic.toml`, em argumento de comando, em issue ou em pedido de contribuição. O plugin lê o **nome** de uma variável de ambiente ou o caminho de um arquivo.
