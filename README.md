# Mosaic

Mosaic é um painel de terminal (TUI) feito de peças independentes. Cada widget é um programa, em Python, Node, shell ou qualquer linguagem, que imprime um JSON seguindo um protocolo simples e versionado. O Mosaic desenha o resultado em cards (linhas, agenda, indicadores, tabelas), avisa quando algo muda e traz um pomodoro e uma integração opcional com LLMs.

> A terminal dashboard built from pluggable widgets. Any program that prints JSON becomes a card.

## Uso

```sh
cargo run --release
```

Na primeira execução o Mosaic cria `~/.config/mosaic/` com a configuração, os widgets de exemplo (kanban, Jira, galeria) e o prompt de prioridade. Se você já usava a versão anterior, copie `~/.config/panel-tui/` para `~/.config/mosaic/`.

## Escrevendo um widget

Um widget responde ao argumento `snapshot` imprimindo um JSON:

```python
import json
print(json.dumps({"summary": {"title": "Olá", "blocks": [
    {"type": "lines", "lines": [{"text": "tudo certo", "tone": "ok"}]}
]}}))
```

Valide com `mosaic check python3 meu_widget.py`. O esquema JSON do protocolo está em [`schema/protocol.schema.json`](schema/protocol.schema.json). Os widgets `plugins/gallery.py` e `plugins/gallery.js` mostram todos os blocos.

## Licença

MIT
