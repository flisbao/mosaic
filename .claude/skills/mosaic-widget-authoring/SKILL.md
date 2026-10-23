---
name: mosaic-widget-authoring
description: Use quando for criar, alterar ou revisar um widget ou plugin do Mosaic (programa que imprime JSON para o painel), ou antes de contribuir com um plugin novo.
---

# Criando widgets e plugins do Mosaic

## Visão geral

Um widget é um comando completo (script + argumentos) que o Mosaic executa em intervalos, lê um JSON da saída padrão e desenha. Um plugin é uma pasta em `plugins/` com o código, o README, os testes e a skill desse widget. O contrato está em `PROTOCOL.md` e em `schema/protocol.schema.json`: leia o `PROTOCOL.md` antes de escrever o código e não copie o contrato para outros arquivos.

> Instalada pelo pacote (`mosaic skills install`), esta skill serve a quem cria um widget próprio em `~/.config/mosaic/`. Os passos de registro (`default_files.rs`), `CONTRIBUTING.md` e o teste de skills só existem no repositório.

## Pré-requisito de contribuição

**Plugin novo só é aceito com a sua skill** `.claude/skills/mosaic-plugin-<pasta>/SKILL.md`, escrita a partir de [plugin-skill-template.md](plugin-skill-template.md). `python3 -m unittest discover -s plugins/tests` falha se uma pasta de plugin não tiver skill, se a skill estiver mal formada ou se citar uma variável que o código do plugin não conhece.

Skills existentes: `mosaic-plugin-jira`, `-github`, `-gitlab`, `-azure-devops`, `-calendar`. Ao mexer em um deles, use a skill dele junto com esta.

## Requisitos do Mosaic (não negociáveis)

- `<comando> snapshot`: stdin fechado, **um** JSON na saída padrão, processo termina. Sem daemon, sem processo filho que sobreviva: no timeout, no cancelamento e na saída do Mosaic o grupo de processos recebe `SIGKILL`.
- Saída até 1 MiB (`1_048_576` bytes); `timeout_seconds` do widget (padrão 5 s). Se o plugin faz rede, o widget declara um `timeout_seconds` maior e o plugin tem orçamento interno menor que ele.
- Limites do protocolo: 20 blocos, 200 linhas por bloco, 500 caracteres por texto, `id` de item de 1 a 200 e de alerta de 1 a 100. O excesso é cortado, não rejeitado: respeite os limites no plugin.
- `interval_seconds` abaixo de 30 sobe para 30.
- Uma execução devolve **um** widget. Dois conjuntos de dados são dois widgets, cada um com `id` único.
- Falha (exit ≠ 0, JSON inválido, timeout, saída grande) vira erro só desse widget e o último dado bom fica. O plugin sai com código 1 e escreve `erro: <mensagem>` em stderr; uso incorreto sai com 2.
- `command` é lista de argumentos, sem shell. O Mosaic acrescenta `snapshot` depois de `args`, então opções do plugin vão antes.
- Segredo **nunca** em `args` nem em `mosaic.toml`: o plugin lê o **nome** de uma variável (`<PREFIXO>_TOKEN_ENV`) ou um caminho de arquivo (`<PREFIXO>_TOKEN_FILE`), e o widget declara `inherit_env` com o nome da variável. Nenhuma mensagem de erro, aviso ou arquivo de estado pode conter o segredo.
- O widget roda na pasta do `mosaic.toml`; caminhos de plugin são `plugins/<pasta>/<arquivo>.py`.
- Avisos (`alerts`): `id` estável (mesmo evento, mesmo id). A primeira leitura só memoriza. Títulos de seção estáveis, porque `llm_sections` casa por igualdade exata.
- Termo **widget**; nunca "card" no código, no schema ou na documentação. Siga o `CLAUDE.md`: nomes que revelam intenção, constantes nomeadas no lugar de números mágicos, sem comentários óbvios.

## Anatomia (o padrão dos plugins existentes)

```text
plugins/<pasta>/<nome>.py      só biblioteca padrão do Python 3, um arquivo
plugins/<pasta>/README.md      o que mostra, variáveis, token, testar, limitações
plugins/tests/test_<nome>.py   unittest; servidor falso em plugins/tests/fake_*.py se houver rede
.claude/skills/mosaic-plugin-<pasta>/SKILL.md
```

Forma do código (copie de `plugins/github/github_prs.py`):

- `load_config(environ)` devolve uma dataclass e levanta `ConfigError` com mensagem acionável.
- Cliente HTTP com `http` injetável (`urllib_http(url, method, headers, body, timeout)`), sem seguir redirecionamento, com limite de bytes de resposta; a URL base de teste só aceita `127.0.0.1` ou `localhost` (ou https).
- `build_snapshot(...)` é função pura (recebe `now`); `main(arguments, environ=None, http=..., now=None, stdout=None, stderr=None) -> int` injeta tudo, para testar sem rede nem relógio.
- Erros tipados: autenticação, limite de requisições, rede, resposta inesperada, requisição recusada. **Permanentes** (token inválido, 400/404/410) mostram `erro:` e não usam cache. **Transitórios** servem o último resultado com o bloco `dados de HH:MM (offline)`.
- Estado entre execuções (cache offline, memória de revisões) em `<PREFIXO>_STATE_DIR`, pasta `0o700`, escrita atômica (arquivo temporário e `rename`), nome do arquivo = hash de tudo que identifica o widget, **sem** o segredo.
- Os plugins de PRs repetem de propósito os mesmos auxiliares (`read_token`, `decode_response`, `StateStore`): cada plugin é um arquivo autônomo. Correção num deles se repete nos outros dois, com teste.

## Fluxo

1. Ler `PROTOCOL.md`; decidir `summary`, `detail`, `actions`, `alerts` e `mini` antes de codar.
2. **REQUIRED SUB-SKILL:** `superpowers:test-driven-development`. Teste primeiro (`plugins/tests/test_<nome>.py`), veja falhar, implemente.
3. `python3 -m unittest discover -s plugins/tests` (suíte inteira, não só o arquivo novo).
4. `mosaic check --cwd ~/.config/mosaic ... python3 plugins/<pasta>/<nome>.py` (ou `--config` para validar o `mosaic.toml`). Atenção: `check` passa o ambiente inteiro do shell e **não aplica `inherit_env`**; confira `inherit_env` à mão.
5. Registrar para a instalação padrão em `src/infrastructure/default_files.rs`: entrada em `BUNDLED_PLUGINS` (e o número no tipo do array `[(&str, &str); N]`) e bloco **comentado** no `DEFAULT_CONFIG`. Mencionar o plugin no `README.md` da raiz.
6. Escrever a skill do plugin pelo template e conferir que o teste de skills passa.
7. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`; ao final `cargo clean`.
8. Revisão do plugin pronto por um modelo mais forte que o implementador (a equipe usa Sonnet 5.5, ou gpt-6-sol-1 em harness OpenAI). Critical e Important voltam com teste que falha primeiro.

## Erros comuns

| Erro | Correção |
| --- | --- |
| Token em `args`, `[widgets.env]` com o valor, ou em `--env` no `mosaic check` | Variável com o nome (`*_TOKEN_ENV`) ou arquivo `chmod 600`; o valor fica fora do repositório e do histórico do shell |
| `mosaic check` passou, widget real falha por variável ausente | `check` ignora `inherit_env`; liste a variável no widget |
| Erro permanente servindo cache offline | Autenticação e requisição recusada nunca usam o cache |
| `interval_seconds = 5` | Mínimo efetivo é 30; use `r` para forçar |
| Opção do plugin depois de `snapshot` | O Mosaic acrescenta `snapshot` no fim; opções vão em `args` |
| Id de alerta com data ou contador | Mesmo evento, mesmo id; senão o aviso repete ou esconde o próximo |
| Esquecer o número do array `BUNDLED_PLUGINS` | O compilador acusa; acrescente também o teste de instalação do plugin (veja os de Jira e GitLab em `default_files.rs`) |
| Plugin sem skill | O teste de skills falha; crie a skill pelo template |
