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

Valide com `mosaic check python3 meu_widget.py`. O contrato completo está em [`PROTOCOL.md`](PROTOCOL.md) e o esquema JSON em [`schema/protocol.schema.json`](schema/protocol.schema.json). Os widgets `plugins/gallery.py` e `plugins/gallery.js` mostram todos os blocos.

## Abas e atalhos

Sem `[[tabs]]` na configuração, todos os widgets ficam numa aba única. Para separar em abas, declare cada aba com `name` e aponte os widgets para ela com `tab`. `max_items` limita quantos itens o widget mostra:

```toml
[[tabs]]
name = "Trabalho"

[[tabs]]
name = "Pessoal"

[[widgets]]
id = "jira-meus"
command = "python3"
args = ["plugins/jira.py"]
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

Valide o arquivo sem abrir o painel:

```sh
mosaic check --config            # usa ~/.config/mosaic/config.toml
mosaic check --config caminho.toml
```

## Licença

MIT
