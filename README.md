# Mosaic

Mosaic é um painel de terminal (TUI) feito de peças independentes. Cada widget é um programa, em Python, Node, shell ou qualquer linguagem, que imprime um JSON seguindo um protocolo simples e versionado. O Mosaic desenha o resultado em widgets (linhas, agenda, indicadores, tabelas), avisa quando algo muda e traz um pomodoro e uma integração opcional com LLMs.

> A terminal dashboard built from pluggable widgets. Any program that prints JSON becomes a widget.

## Uso

```sh
cargo run --release
```

Na primeira execução o Mosaic cria `~/.config/mosaic/` com a configuração, os widgets de exemplo (kanban, Jira, galeria) e o prompt de prioridade. Se você já usava a versão anterior, copie `~/.config/panel-tui/` para `~/.config/mosaic/`.

O prompt é enviado ao `claude` e ao `opencode` pela entrada padrão, nunca como argumento, para não aparecer em `ps`.

## Escrevendo um widget

Um widget responde ao argumento `snapshot` imprimindo um JSON:

```python
import json
print(json.dumps({"summary": {"title": "Olá", "blocks": [
    {"type": "lines", "lines": [{"text": "tudo certo", "tone": "ok"}]}
]}}))
```

Valide com `mosaic check python3 meu_widget.py` (o `check` executa o script a partir do diretório atual; só valide widgets em que você confia). O contrato completo está em [`PROTOCOL.md`](PROTOCOL.md) e o esquema JSON em [`schema/protocol.schema.json`](schema/protocol.schema.json). Os widgets `plugins/gallery.py` e `plugins/gallery.js` mostram todos os blocos.

## Jira

`plugins/jira.py` é um provider genérico para o Jira Cloud: cada widget define o próprio JQL e como mostrar o resultado. O Mosaic acrescenta `snapshot` aos `args`, então as opções do widget vêm antes dele e a conexão (site, e-mail, token) vem por variáveis de ambiente em `[widgets.env]`. O token nunca vai em argumento: `JIRA_TOKEN_ENV` guarda o **nome** da variável que contém o token (ou use `JIRA_TOKEN_FILE`).

### Testando de graça

1. Crie um site Jira Cloud Free em [atlassian.com](https://www.atlassian.com/software/jira/free) (o plano gratuito cobre até 10 usuários: confira na página oficial da Atlassian).
2. Gere um token de API em [id.atlassian.com](https://id.atlassian.com/manage-profile/security/api-tokens). Ele aparece uma única vez: copie na hora (confira na página oficial da Atlassian).
3. Crie alguns issues no site e exporte o token:

```sh
export JIRA_TOKEN_EMPRESA="seu-token-de-api"
mosaic check --cwd ~/.config/mosaic \
  --env JIRA_SITE=meusite.atlassian.net \
  --env JIRA_EMAIL=voce@exemplo.com \
  --env JIRA_TOKEN_ENV=JIRA_TOKEN_EMPRESA \
  python3 plugins/jira.py -- --jql "assignee = currentUser()"
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
  python3 plugins/jira.py -- --jql "assignee = currentUser()"
```

### Minhas tarefas

O widget principal: o que está atribuído a você e ainda não terminou.

```toml
[[widgets]]
id = "jira-minhas-tarefas"
command = "python3"
args = ["plugins/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC", "--title", "Minhas tarefas"]
interval_seconds = 120
timeout_seconds = 45
inherit_env = ["JIRA_TOKEN_EMPRESA"]

[widgets.env]
JIRA_SITE = "empresa.atlassian.net"
JIRA_EMAIL = "voce@empresa.com"
JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"
```

O resumo lista `CHAVE resumo` e o detalhe (widget expandido) agrupa os issues sob o título do widget. Com `--group-by`, cada grupo vira uma seção do detalhe.

### Outros exemplos

Todos usam o mesmo bloco `[widgets.env]` do exemplo acima; abaixo aparecem só `id` e `args`. Lembre de repetir `command`, `interval_seconds`, `timeout_seconds`, `inherit_env` e `[widgets.env]`.

O que cada pessoa está pegando agora, agrupado por responsável e em tabela:

```toml
id = "jira-time-em-andamento"
args = ["plugins/jira.py", "--jql", "project = VHPJ AND statusCategory = \"In Progress\"", "--title", "Time em andamento", "--group-by", "assignee", "--view", "table", "--columns", "key,summary,status"]
```

Issues do time (lista fixa de pessoas):

```toml
id = "jira-time"
args = ["plugins/jira.py", "--jql", "assignee in (\"ana@empresa.com\", \"bruno@empresa.com\") AND statusCategory != Done", "--title", "Time", "--group-by", "assignee"]
```

Sem dono há mais de duas horas, só a contagem:

```toml
id = "jira-sem-dono"
args = ["plugins/jira.py", "--jql", "assignee is EMPTY AND statusCategory = \"To Do\" AND created <= -2h", "--title", "Sem dono", "--view", "stat"]
```

Sprint aberto de um projeto:

```toml
id = "jira-sprint"
args = ["plugins/jira.py", "--jql", "sprint in openSprints() AND project = VHPJ", "--title", "Sprint", "--group-by", "status"]
```

Um filtro salvo no Jira (o número está na URL do filtro):

```toml
id = "jira-filtro"
args = ["plugins/jira.py", "--jql", "filter = 12345", "--title", "Meu filtro"]
```

Dois Jira ao mesmo tempo: são dois widgets, cada um com o seu `[widgets.env]` e o seu `inherit_env`:

```toml
[[widgets]]
id = "jira-empresa-tarefas"
command = "python3"
args = ["plugins/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done", "--title", "Empresa"]
inherit_env = ["JIRA_TOKEN_EMPRESA"]

[widgets.env]
JIRA_SITE = "empresa.atlassian.net"
JIRA_EMAIL = "voce@empresa.com"
JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"

[[widgets]]
id = "jira-cliente-tarefas"
command = "python3"
args = ["plugins/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done", "--title", "Cliente"]
inherit_env = ["JIRA_TOKEN_CLIENTE"]

[widgets.env]
JIRA_SITE = "cliente.atlassian.net"
JIRA_EMAIL = "voce@consultoria.com"
JIRA_TOKEN_ENV = "JIRA_TOKEN_CLIENTE"
```

Os títulos das seções do detalhe vêm dos nomes dos grupos (ou do `--title`, sem agrupamento); é a eles que `llm_sections` se refere, por exemplo `llm_sections = ["Sem dono"]`. Sem responsável, o grupo de `--group-by assignee` se chama `ninguem`.

### Opções do widget

Vão em `args`, antes do `snapshot` (que o Mosaic acrescenta).

| Opção | Padrão | O que faz |
| --- | --- | --- |
| `--jql` | obrigatória | Consulta JQL do widget |
| `--title` | `Jira` | Título do widget (e nome da seção sem agrupamento) |
| `--group-by` | `none` | `none`, `assignee`, `status` ou `priority`: agrupa o detalhe (e as contagens no `stat`) |
| `--view` | `lines` | `lines` (uma linha por issue), `table` ou `stat` (só contagens) |
| `--columns` | `key,summary,status,assignee` | Colunas do `table`: `key`, `summary`, `status`, `assignee`, `priority`, `updated` |
| `--max-issues` | `100` | Máximo de issues buscados, de 1 a 500; o resumo mostra no máximo 200 linhas |

### Variáveis de ambiente

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

### Limitações

- Só Jira Cloud, só leitura (o widget não declara ações) e só token de API clássico (autenticação Basic com e-mail e token).
- O único estado entre execuções é o cache offline: se a rede falhar, o widget mostra o último resultado com o aviso `dados de HH:MM (offline)`. Credenciais inválidas e JQL inválido não usam o cache.
- As variáveis antigas `JIRA_MODE`, `JIRA_PROJECT`, `JIRA_BOARD`, `JIRA_TEAM` e `JIRA_UNASSIGNED_HOURS` foram removidas e dão erro: use `--jql`.

Próximos passos: um coletor com SQLite, relatórios sobre o histórico e uma ação de abrir no navegador.

## Abas e atalhos

Sem `[[tabs]]` na configuração, todos os widgets ficam numa aba única. Para separar em abas, declare cada aba com `name` e aponte os widgets para ela com `tab`. `max_items` limita quantos itens o widget mostra:

```toml
[[tabs]]
name = "Trabalho"

[[tabs]]
name = "Pessoal"

[[widgets]]
id = "jira-minhas-tarefas"
command = "python3"
args = ["plugins/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC", "--title", "Minhas tarefas"]
interval_seconds = 120
tab = "Trabalho"
max_items = 5

[[widgets]]
id = "galeria"
command = "python3"
args = ["plugins/gallery.py"]
tab = "Pessoal"
```

Teclas (modo grade, com os widgets visíveis):

| Tecla | Ação |
| --- | --- |
| `1` a `9` | Vai para a aba de mesmo número |
| `Tab` / `BackTab` | Próxima / aba anterior |
| `←` `↑` `→` `↓` ou `h` `j` `k` `l` | Move o foco entre os widgets |
| `Enter` | Expande o widget focado |
| `A` a `Z` (Shift+letra, exceto `R`) | Foca o widget com essa letra de atalho |
| `a` | Abre o menu de ações do widget focado |
| `r` | Atualiza todos os widgets agora |
| `R` | Recarrega o `config.toml` |
| `i` | Pede uma recomendação à IA (se `[llm]` estiver ligado) |
| `p` / `x` | Inicia ou pausa / reinicia o pomodoro |
| `q` ou `Ctrl+C` | Sai |

Com um widget expandido, as setas (ou `h` `j` `k` `l`) movem entre os itens, `PageUp`/`PageDown` pulam vários itens, `Home`/`End` vão ao primeiro e ao último, e `Esc` volta à grade. No menu de ações, `Enter` escolhe a ação, `s` confirma e `n` ou `Esc` cancela. A tecla `1` a `9` troca de aba só na grade.

Após editar a configuração, `R` recarrega widgets, abas e intervalos. Mudanças em `[llm]`, `[pomodoro]` e notificações só valem após reiniciar o Mosaic, mesmo com a recarga.

`Ctrl+C` (ou `SIGTERM`/`SIGHUP`) encerra o Mosaic com calma: restaura o terminal e finaliza os providers em execução. Um **segundo** sinal enquanto ele ainda encerra força a saída imediata (código 1): o terminal pode ficar em modo bruto, bastando rodar `reset`, e providers que ainda estavam em execução podem continuar vivos até terminarem sozinhos.

Valide o arquivo sem abrir o painel:

```sh
mosaic check --config            # usa ~/.config/mosaic/config.toml
mosaic check --config caminho.toml
```

Por padrão, todo widget herda o ambiente do Mosaic (compatível com versões anteriores). Para que um widget receba só o necessário, liste as variáveis em `inherit_env`: `PATH`, `HOME`, `LC_*`, `XDG_*` e algumas outras básicas sempre passam, o resto é removido, e `[widgets.env]` sempre vale. Exemplo para o Jira, que lê o token pela variável nomeada em `JIRA_TOKEN_ENV`:

```toml
[[widgets]]
id = "jira-empresa"
command = "python3"
args = ["plugins/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done"]
inherit_env = ["JIRA_TOKEN_EMPRESA"]

[widgets.env]
JIRA_SITE = "empresa.atlassian.net"
JIRA_EMAIL = "voce@empresa.com"
JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"
```

O `notify_command` e a CLI da IA ainda herdam o ambiente inteiro.

Título e corpo vêm do provider: o Mosaic limita cada um a 200 caracteres, acrescenta um espaço na frente de textos que começam com `-` e escapa `&`, `<` e `>` no corpo. Ainda assim, prefira programas que não interpretem o texto de outras formas.

`[pomodoro] notify_command` é uma lista de argumentos (ex.: `["notify-send", "--urgency=low"]`); o Mosaic acrescenta título e corpo como os **dois últimos argumentos**, sem shell. Cada notificação tem limite de 5 s e no máximo 4 rodam ao mesmo tempo. Título e corpo vêm do provider: não use programas que interpretem argumentos começando com `-` de forma perigosa.

## Licença

MIT
