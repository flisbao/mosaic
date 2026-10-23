---
name: mosaic-plugin-jira
description: Use quando for configurar, depurar ou alterar o widget do Jira (plugins/jira/jira.py), ou ao mexer em JQL, --group-by, --view, token de API do Jira, JIRA_SITE ou o cache offline do Jira.
---

# Jira

## Visão geral

`plugins/jira/jira.py` é um provider genérico do **Jira Cloud**, só leitura (sem ações): cada widget traz o próprio JQL em `args` e escolhe como mostrar o resultado. Autenticação Basic com e-mail e token de API clássico. Detalhes e exemplos: `plugins/jira/README.md`. Requisitos gerais de widget: `mosaic-widget-authoring`.

## Configurar com a pessoa

Pergunte uma coisa por vez:

1. **Site**: só o host, sem `https://` (`empresa.atlassian.net`) → `JIRA_SITE`.
2. **E-mail** da conta dona do token → `JIRA_EMAIL`.
3. **Onde fica o token**: o **nome** de uma variável de ambiente (`JIRA_TOKEN_ENV`, ex.: `JIRA_TOKEN_EMPRESA`) ou o caminho de um arquivo (`JIRA_TOKEN_FILE`, `chmod 600`). O token se cria em id.atlassian.com e aparece uma vez. **Nunca peça o valor no chat** nem o grave em arquivo do repositório; a pessoa o guarda sozinha (`~/.profile` ou arquivo).
4. **O que ver**: ofereça os modelos do README (minhas tarefas, time por responsável, sem dono, sprint, filtro salvo) e monte o `--jql`. Pergunte `--group-by` (`none`, `assignee`, `status`, `priority`) e `--view` (`lines`, `table`, `stat`; `--columns` só vale em `table`).
5. **Mais de um Jira**: um widget por site, cada um com `id`, `inherit_env` e `[widgets.env]` próprios.

## Bloco do mosaic.toml

```toml
[[widgets]]
id = "jira-minhas-tarefas"
command = "python3"
args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC", "--title", "Minhas tarefas"]
interval_seconds = 120
timeout_seconds = 45
inherit_env = ["JIRA_TOKEN_EMPRESA"]

[widgets.env]
JIRA_SITE = "empresa.atlassian.net"
JIRA_EMAIL = "voce@empresa.com"
JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"
```

`timeout_seconds` precisa ficar acima de `JIRA_BUDGET_SECONDS` (40). Aspas dentro do JQL viram `\"` no TOML.

## Verificar

Com a conta real (a pessoa exporta o token no próprio shell antes de abrir o Claude, ou roda o comando com `!`):

```sh
mosaic check --cwd ~/.config/mosaic \
  --env JIRA_SITE=empresa.atlassian.net --env JIRA_EMAIL=voce@empresa.com \
  --env JIRA_TOKEN_ENV=JIRA_TOKEN_EMPRESA \
  python3 plugins/jira/jira.py -- --jql "assignee = currentUser()"
```

Sem conta: `FAKE_JIRA_EMAIL=voce@exemplo.com FAKE_JIRA_TOKEN=segredo python3 plugins/tests/fake_jira_server.py` imprime a porta na primeira linha; use `--env JIRA_BASE_URL=http://127.0.0.1:PORTA` e `FAKE_TOKEN=segredo` (o servidor falso ignora `statusCategory`). Depois `mosaic check --config` valida o `mosaic.toml`. `check` não aplica `inherit_env`: confira a lista à mão.

## Mexendo no código

- Testes: `python3 -m unittest discover -s plugins/tests -p test_jira.py` e `cargo test --test jira_end_to_end`.
- Erros: `AuthError` e `QueryError` (JQL inválido) são **permanentes**: `erro:` e nada de cache. `RateLimitError`, `NetworkError` e `UnexpectedResponseError` servem o cache com `dados de HH:MM (offline)`.
- A chave do cache é `(site, jql, max_issues)`: mudar o JQL começa com o cache vazio.
- `JIRA_BUDGET_SECONDS` (padrão 40) é o orçamento interno; estourá-lo devolve resultado parcial (dívida conhecida: o parcial também vira cache).
- Os títulos das seções do detalhe são os nomes dos grupos (ou o `--title` sem agrupamento); `llm_sections` casa por eles. Sem responsável, o grupo se chama `ninguem`.
- O resumo mostra no máximo 200 linhas; `--max-issues` vai de 1 a 500.
- `JIRA_MODE`, `JIRA_PROJECT`, `JIRA_BOARD`, `JIRA_TEAM` e `JIRA_UNASSIGNED_HOURS` foram removidas e dão erro: use `--jql`.
- `JIRA_BASE_URL` é só para teste local (`127.0.0.1` ou `localhost`).
- Dívida: o plugin nunca foi validado contra um Jira real; só o servidor falso.

## Armadilhas

| Sintoma | Causa | Correção |
| --- | --- | --- |
| Erro de autenticação | E-mail diferente do dono do token, ou token revogado | Conferir `JIRA_EMAIL`; gerar outro token |
| `JIRA_SITE` recusado | Veio com `https://` ou caminho | Só o host |
| Widget sem credenciais mesmo com `check` ok | Variável do token fora de `inherit_env` | Adicionar ao `inherit_env` |
| JQL com aspas quebra o TOML | Aspas sem escape | `\"` dentro da string do TOML |
| Resumo cheio de itens concluídos | O servidor falso não filtra `statusCategory` | Limitação do falso; no Jira real o JQL filtra |
