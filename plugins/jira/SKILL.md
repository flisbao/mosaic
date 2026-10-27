---
name: mosaic-plugin-jira
description: Use quando for configurar, depurar ou alterar o widget do Jira (plugins/jira/jira.py), ou ao mexer em JQL, --group-by, --view, token de API do Jira, JIRA_SITE ou o cache offline do Jira.
---

# Jira

## Visão geral

`plugins/jira/jira.py` é um provider genérico do **Jira Cloud**, só leitura (sem ações): cada widget traz o próprio JQL em `args` e escolhe como mostrar o resultado. Autenticação Basic com e-mail e token de API clássico. Detalhes e exemplos: `plugins/jira/README.md`. Requisitos gerais de widget: `mosaic-widget-authoring`.

> Instalada por `mosaic plugins install` (junto com o plugin, em `~/.config/mosaic/plugins/jira/`), os caminhos `plugins/...` são os de `~/.config/mosaic/`. A pasta `plugins/tests/` (servidores falsos e testes) e a seção "Mexendo no código" só valem com o repositório clonado.

## Instalar e configurar com o mosaic plugins

Passos que a pessoa roda no terminal dela (agente: não rode o `setup` por ela, ele é interativo):

1. `mosaic plugins list`: lista os plugins disponíveis no pacote (`/usr/share/mosaic/plugins`).
2. `mosaic plugins install jira`: copia a pasta para `~/.config/mosaic/plugins/jira/` (script, `README.md`, esta `SKILL.md` e o `setup.toml`) e liga esta skill ao Claude Code em `~/.claude/skills/mosaic-plugin-jira`, se `~/.claude` existir. Não sobrescreve nada: depois de instalar um pacote novo, use `--force`. `--no-skill` pula a skill; `--from DIR`, `--to DIR` e `--skills-dir DIR` apontam para outras pastas (tarball, testes).
3. `mosaic plugins setup jira`: entrevista no terminal, só com o que o plugin já lê (veja "Todas as opções"). Mostra o `[[widgets]]` pronto e só grava no `mosaic.toml` depois de um "S". Nunca pede o valor do segredo: só o nome da variável ou o caminho do arquivo. `--print` só mostra o bloco, `--advanced` inclui as opções avançadas e `--config-dir DIR` usa outra pasta de configuração. Para um segundo widget (outra conta, outro filtro), rode de novo com outro id: cada execução acrescenta um bloco.
4. Aperte `R` no mosaic para recarregar; `mosaic check --config` valida o arquivo.

Dentro do Claude Code, a pessoa pode rodar `! mosaic plugins setup jira` sem sair da conversa. Se ela preferir conversar, siga a entrevista abaixo e entregue o bloco para colar.

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

## Todas as opções

O que o `mosaic plugins setup` pergunta, na ordem. Vem do `setup.toml` desta pasta, que um teste confere com o README e com o código do plugin.

| Opção | O que é | Padrão | Situação |
| --- | --- | --- | --- |
| `JIRA_SITE` | Host do site do Jira, sem https:// (ex.: empresa.atlassian.net) | — | obrigatória |
| `JIRA_EMAIL` | E-mail da conta dona do token | — | obrigatória |
| `JIRA_TOKEN_ENV` ou `JIRA_TOKEN_FILE` | Token de API do Jira (crie em id.atlassian.com) (nome da variável ou caminho do arquivo; o valor nunca é pedido) | — | obrigatória |
| `--jql` | Consulta JQL do widget | — | obrigatória |
| `--title` | Título do widget | `Jira` | opcional |
| `--group-by` | Agrupar o detalhe por: `none`, `assignee`, `status`, `priority` | `none` | opcional |
| `--view` | Como mostrar os issues: `lines`, `table`, `stat` | `lines` | opcional |
| `--columns` | Colunas da tabela: key, summary, status, assignee, priority, updated (separados por vírgula) | `key,summary,status,assignee` | opcional; perguntada se `--view` = `table` |
| `--max-issues` | Máximo de issues buscados (1 a 500) | `100` | opcional |
| `JIRA_STATE_DIR` | Pasta do cache offline | `~/.local/state/mosaic/jira` | opcional; só com `--advanced` |
| `JIRA_BUDGET_SECONDS` | Orçamento interno de tempo, em segundos (menor que o tempo máximo do widget) (1 a 3600) | `40` | opcional; só com `--advanced` |

- Só para testes locais, o `setup` não pergunta: `JIRA_BASE_URL`.
- Valores de partida do widget: `interval_seconds = 120` e `timeout_seconds = 45`; o id padrão é `jira`.

Modelos prontos que o `setup` oferece antes das perguntas (vêm do README):

- `minhas-tarefas`: o que está atribuído a você e ainda não terminou.
- `time-em-andamento`: o que cada pessoa do projeto está pegando agora, agrupado por responsável, em tabela.
- `time`: issues abertos de uma lista fixa de pessoas, agrupados por responsável.
- `sem-dono`: issues a fazer sem responsável há mais de duas horas, só a contagem.
- `sprint`: sprint aberto de um projeto, agrupado por status.
- `filtro-salvo`: um filtro salvo no Jira (o número está na URL do filtro).

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
