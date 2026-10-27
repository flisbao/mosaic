# Jira

> Instalação: `mosaic plugins install jira` (o plugin não vem no binário; veja o README da raiz). Configuração guiada, sem editar o `mosaic.toml` à mão: `mosaic plugins setup jira` (pergunta só o que este plugin lê, mostra o bloco e grava depois que você confirmar).

`plugins/jira/jira.py` é um provider genérico para o Jira Cloud: cada widget define o próprio JQL e como mostrar o resultado. O Mosaic acrescenta `snapshot` aos `args`, então as opções do widget vêm antes dele e a conexão (site, e-mail, token) vem por variáveis de ambiente em `[widgets.env]`. O token nunca vai em argumento: `JIRA_TOKEN_ENV` guarda o **nome** da variável que contém o token (ou use `JIRA_TOKEN_FILE`).

## Testando de graça

1. Crie um site Jira Cloud Free em [atlassian.com](https://www.atlassian.com/software/jira/free) (o plano gratuito cobre até 10 usuários: confira na página oficial da Atlassian).
2. Gere um token de API em [id.atlassian.com](https://id.atlassian.com/manage-profile/security/api-tokens). Ele aparece uma única vez: copie na hora (confira na página oficial da Atlassian).
3. Crie alguns issues no site e exporte o token:

```sh
export JIRA_TOKEN_EMPRESA="seu-token-de-api"
mosaic check --cwd ~/.config/mosaic \
  --env JIRA_SITE=meusite.atlassian.net \
  --env JIRA_EMAIL=voce@exemplo.com \
  --env JIRA_TOKEN_ENV=JIRA_TOKEN_EMPRESA \
  python3 plugins/jira/jira.py -- --jql "assignee = currentUser()"
```

Sem rede e sem conta, use o servidor falso dos testes. Ele imprime a porta na primeira linha e aceita as credenciais definidas em `FAKE_JIRA_EMAIL` e `FAKE_JIRA_TOKEN`:

```sh
FAKE_JIRA_EMAIL=voce@exemplo.com FAKE_JIRA_TOKEN=segredo python3 plugins/tests/fake_jira_server.py
# em outro terminal, com a porta impressa:
export FAKE_TOKEN=segredo
mosaic check --cwd ~/.config/mosaic \
  --env JIRA_SITE=meusite.atlassian.net \
  --env JIRA_BASE_URL=http://127.0.0.1:PORTA \
  --env JIRA_EMAIL=voce@exemplo.com \
  --env JIRA_TOKEN_ENV=FAKE_TOKEN \
  python3 plugins/jira/jira.py -- --jql "assignee = currentUser()"
```

## Minhas tarefas

O widget principal: o que está atribuído a você e ainda não terminou.

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

O resumo lista `CHAVE resumo` e o detalhe (widget expandido) agrupa os issues sob o título do widget. Com `--group-by`, cada grupo vira uma seção do detalhe.

## Outros exemplos

Todos usam o mesmo bloco `[widgets.env]` do exemplo acima; abaixo aparecem só `id` e `args`. Lembre de repetir `command`, `interval_seconds`, `timeout_seconds`, `inherit_env` e `[widgets.env]`.

O que cada pessoa está pegando agora, agrupado por responsável e em tabela:

```toml
id = "jira-time-em-andamento"
args = ["plugins/jira/jira.py", "--jql", "project = VHPJ AND statusCategory = \"In Progress\"", "--title", "Time em andamento", "--group-by", "assignee", "--view", "table", "--columns", "key,summary,status"]
```

Issues do time (lista fixa de pessoas):

```toml
id = "jira-time"
args = ["plugins/jira/jira.py", "--jql", "assignee in (\"ana@empresa.com\", \"bruno@empresa.com\") AND statusCategory != Done", "--title", "Time", "--group-by", "assignee"]
```

Sem dono há mais de duas horas, só a contagem:

```toml
id = "jira-sem-dono"
args = ["plugins/jira/jira.py", "--jql", "assignee is EMPTY AND statusCategory = \"To Do\" AND created <= -2h", "--title", "Sem dono", "--view", "stat"]
```

Sprint aberto de um projeto:

```toml
id = "jira-sprint"
args = ["plugins/jira/jira.py", "--jql", "sprint in openSprints() AND project = VHPJ", "--title", "Sprint", "--group-by", "status"]
```

Um filtro salvo no Jira (o número está na URL do filtro):

```toml
id = "jira-filtro"
args = ["plugins/jira/jira.py", "--jql", "filter = 12345", "--title", "Meu filtro"]
```

Dois Jira ao mesmo tempo: são dois widgets, cada um com o seu `[widgets.env]` e o seu `inherit_env`:

```toml
[[widgets]]
id = "jira-empresa-tarefas"
command = "python3"
args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done", "--title", "Empresa"]
inherit_env = ["JIRA_TOKEN_EMPRESA"]

[widgets.env]
JIRA_SITE = "empresa.atlassian.net"
JIRA_EMAIL = "voce@empresa.com"
JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"

[[widgets]]
id = "jira-cliente-tarefas"
command = "python3"
args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done", "--title", "Cliente"]
inherit_env = ["JIRA_TOKEN_CLIENTE"]

[widgets.env]
JIRA_SITE = "cliente.atlassian.net"
JIRA_EMAIL = "voce@consultoria.com"
JIRA_TOKEN_ENV = "JIRA_TOKEN_CLIENTE"
```

Os títulos das seções do detalhe vêm dos nomes dos grupos (ou do `--title`, sem agrupamento); é a eles que `llm_sections` se refere, por exemplo `llm_sections = ["Sem dono"]`. Sem responsável, o grupo de `--group-by assignee` se chama `ninguem`.

## Opções do widget

Vão em `args`, antes do `snapshot` (que o Mosaic acrescenta).

| Opção | Padrão | O que faz |
| --- | --- | --- |
| `--jql` | obrigatória | Consulta JQL do widget |
| `--title` | `Jira` | Título do widget (e nome da seção sem agrupamento) |
| `--group-by` | `none` | `none`, `assignee`, `status` ou `priority`: agrupa o detalhe (e as contagens no `stat`) |
| `--view` | `lines` | `lines` (uma linha por issue), `table` ou `stat` (só contagens) |
| `--columns` | `key,summary,status,assignee` | Colunas do `table`: `key`, `summary`, `status`, `assignee`, `priority`, `updated` |
| `--max-issues` | `100` | Máximo de issues buscados, de 1 a 500; o resumo mostra no máximo 200 linhas |

## Variáveis de ambiente

Vão em `[widgets.env]` (o token, por `inherit_env` ou `JIRA_TOKEN_FILE`).

| Variável | Padrão | O que faz |
| --- | --- | --- |
| `JIRA_SITE` | obrigatória | Só o host do site, ex.: `empresa.atlassian.net` |
| `JIRA_EMAIL` | obrigatória | E-mail da conta dona do token |
| `JIRA_TOKEN_ENV` | | Nome da variável de ambiente que contém o token |
| `JIRA_TOKEN_FILE` | | Caminho de um arquivo com o token (alternativa a `JIRA_TOKEN_ENV`) |
| `JIRA_STATE_DIR` | `~/.local/state/mosaic/jira` | Pasta do cache offline |
| `JIRA_BUDGET_SECONDS` | `40` | Orçamento interno de tempo; mantenha abaixo do `timeout_seconds` do widget |
| `JIRA_BASE_URL` | | Só para testes locais: `http://127.0.0.1` ou `localhost` (ou https) no lugar de `https://<JIRA_SITE>` |

## Limitações

- Só Jira Cloud, só leitura (o widget não declara ações) e só token de API clássico (autenticação Basic com e-mail e token).
- O único estado entre execuções é o cache offline: se a rede falhar, o widget mostra o último resultado com o aviso `dados de HH:MM (offline)`. Credenciais inválidas e JQL inválido não usam o cache.
- As variáveis antigas `JIRA_MODE`, `JIRA_PROJECT`, `JIRA_BOARD`, `JIRA_TEAM` e `JIRA_UNASSIGNED_HOURS` foram removidas e dão erro: use `--jql`.

Próximos passos: um coletor com SQLite, relatórios sobre o histórico e uma ação de abrir no navegador.
