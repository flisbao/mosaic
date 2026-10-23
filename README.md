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

O provider do Jira Cloud (`plugins/jira/jira.py`) é genérico: cada widget define o próprio JQL e como mostrar o resultado. Exemplos, opções, variáveis e como testar com uma conta gratuita estão em [`plugins/jira/README.md`](plugins/jira/README.md).

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
args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC", "--title", "Minhas tarefas"]
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
| `Shift+letra` ou `Alt+letra` (`A` a `Z`, exceto `R`) | Foca o widget com essa letra de atalho. `Alt` exige que o terminal envie `Esc` antes da letra (no macOS, ative "Option como Meta") |
| `a` | Abre o menu de ações do widget focado |
| `r` | Atualiza todos os widgets agora |
| `R` | Recarrega o `mosaic.toml` |
| `i` | Pede uma recomendação à IA (se `[llm]` estiver ligado) |
| `p` / `x` | Inicia ou pausa / reinicia o pomodoro |
| `q` ou `Ctrl+C` | Sai |
| `?` | Abre a ajuda com as teclas da tela atual (`Esc` ou `?` fecham) |

Com um widget expandido, as setas (ou `h` `j` `k` `l`) movem entre os itens, `PageUp`/`PageDown` pulam vários itens, `Home`/`End` vão ao primeiro e ao último, e `Esc` volta à grade. No menu de ações, `Enter` escolhe a ação, `s` confirma e `n` ou `Esc` cancela. A tecla `1` a `9` troca de aba só na grade. A última linha do terminal mostra as dicas de teclas do contexto (a partir de 21 linhas de altura) e cada aba exibe o número que a abre. `Enter` em um widget sem `detail` mostra um aviso em vez de expandir.

Após editar a configuração, `R` recarrega widgets, abas e intervalos. Mudanças em `[llm]`, `[pomodoro]` e notificações só valem após reiniciar o Mosaic, mesmo com a recarga.

`Ctrl+C` (ou `SIGTERM`/`SIGHUP`) encerra o Mosaic com calma: restaura o terminal e finaliza os providers em execução. Um **segundo** sinal enquanto ele ainda encerra força a saída imediata (código 1): o terminal pode ficar em modo bruto, bastando rodar `reset`, e providers que ainda estavam em execução podem continuar vivos até terminarem sozinhos.

Valide o arquivo sem abrir o painel:

```sh
mosaic check --config            # usa ~/.config/mosaic/mosaic.toml
mosaic check --config caminho.toml
```

Por padrão, todo widget herda o ambiente do Mosaic (compatível com versões anteriores). Para que um widget receba só o necessário, liste as variáveis em `inherit_env`: `PATH`, `HOME`, `LC_*`, `XDG_*` e algumas outras básicas sempre passam, o resto é removido, e `[widgets.env]` sempre vale. Exemplo para o Jira, que lê o token pela variável nomeada em `JIRA_TOKEN_ENV`:

```toml
[[widgets]]
id = "jira-empresa"
command = "python3"
args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done"]
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
