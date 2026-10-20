# Protocolo de widgets do Mosaic (v1)

Um widget é qualquer programa que o Mosaic executa: Python, Node, shell, o que você preferir. Ele responde ao argumento `snapshot` imprimindo um JSON, e o Mosaic desenha o resultado como um widget. Este documento é o contrato; o [esquema JSON](schema/protocol.schema.json) é equivalente e os testes garantem que os dois não divergem do código.

Um widget completo, em Python:

```python
import json
print(json.dumps({"summary": {"title": "Olá", "blocks": [
    {"type": "lines", "lines": [{"text": "tudo certo", "tone": "ok"}]}
]}}))
```

## 1. Chamada

O Mosaic roda `command` + `args` do widget com um argumento a mais:

| Chamada | Quando | Saída esperada |
| --- | --- | --- |
| `<comando> snapshot` | a cada `interval_seconds` e quando o usuário aperta `r` | um JSON na saída padrão |
| `<comando> action <id>` | quando o usuário confirma uma ação (parâmetros em JSON na entrada padrão) | uma mensagem curta de texto |

O processo recebe o ambiente do Mosaic mais `[widgets.env]` e roda no diretório de configuração do Mosaic (a pasta que contém o `config.toml`). A saída vai até **1 MiB** e o widget tem `timeout_seconds` (padrão 5 s) para responder; código de saída diferente de zero, tempo esgotado ou saída grande demais viram um erro no widget. `interval_seconds` abaixo de **30** sobe para 30 (o widget avisa); a tecla `r` não tem esse limite.

## 2. Snapshot

```json
{
  "protocol": 1,
  "summary": { "title": "Agenda", "blocks": [ ... ] },
  "detail": { "sections": [ { "heading": "Hoje", "blocks": [ ... ], "items": [ ... ] } ] },
  "actions": [ { "id": "refresh", "label": "Atualizar" } ],
  "alerts": [ { "id": "deploy-42", "title": "Deploy falhou", "level": "warn" } ],
  "mini": { "value": "12", "text": "tarefas", "tone": "warn" }
}
```

- **`protocol`** (opcional, padrão 1): número inteiro positivo. Se for maior que a versão suportada, o widget pede para atualizar o Mosaic; outro valor é erro.
- **`summary.title`** (obrigatório) e **`summary.blocks`**: o conteúdo da caixa na Home (seção 3). Atalho: no lugar de `blocks` você pode usar `summary.lines`, uma lista de `{ "text", "flag"? }`.
- **`detail`**: a aba do widget. A configuração decide a aba e o atalho para ela, com `[[tabs]]` e `tab = "Nome"`. Cada seção tem `heading`, e opcionalmente `blocks` (desenhados primeiro) e `items`, a lista selecionável de `{ "id"?, "text", "flag"?, "meta"? }`.
- **`actions`**: `{ "id", "label", "description"?, "applies_to"?: "widget" | "item", "params"? }`.
- **`alerts`**: seção 5.
- **`mini`** (opcional): a coluna do modo foco, `{ "value", "text", "tone"? }`. `value` e `text` são obrigatórios; `tone` tem o padrão `normal`. Um `mini` inválido é descartado sem afetar o resto do widget. O `mosaic check` mostra a linha `mini: <value> <text>`.

Campos desconhecidos são ignorados, então um widget novo continua funcionando em um Mosaic mais antigo.

## 3. Blocos

Um bloco é um objeto com `type`, empilhado na ordem em que aparece.

| `type` | Campos | Descrição |
| --- | --- | --- |
| `lines` | `lines`: `{ "text", "tone"?, "flag"? }` | linhas de texto |
| `rows` | `rows`: `{ "text", "time"?, "marker"?, "tags"?, "tone"?, "flag"? }` | agenda e listas; `marker` é **um** caractere; `tags` são `{ "text", "tone"? }` alinhadas à direita |
| `stat` | `items`: `{ "value", "label", "tone"? }`, `hint`? | indicadores lado a lado; quebram de linha se não couberem |
| `table` | `columns`: `{ "title", "align"?: "left" \| "right" }`, `rows`: lista de listas | célula é um texto ou `{ "text", "tone"?, "flag"? }` |

```json
{ "type": "rows", "rows": [
  { "time": "10:30", "text": "Revisão", "tone": "current", "marker": "▶",
    "tags": [ { "text": "agora", "tone": "ok" } ] }
] }
{ "type": "table",
  "columns": [ { "title": "Pessoa" }, { "title": "Em andamento", "align": "right" } ],
  "rows": [ [ "Ana", "3" ], [ { "text": "Bia", "tone": "danger", "flag": "warning" }, "5" ] ] }
```

**Tons:** `normal` (padrão do terminal), `muted` (cinza), `ok` (verde), `warn` (amarelo), `danger` (vermelho), `current` (ciano em negrito). As cores são fixas.
**Flags:** `updated` (`●`) e `warning` (`⚠`).

## 4. Limites, texto e erros

| Limite | Valor |
| --- | --- |
| blocos por widget | 20 |
| linhas por bloco (`lines`, `rows` e linhas de `table`) | 200 |
| colunas de `table` | 10 |
| indicadores de `stat` | 8 |
| etiquetas por linha de `rows` | 3 |
| caracteres por texto | 500 |
| caracteres de `mini.value` | 8 |
| caracteres de `mini.text` | 24 |
| avisos novos por leitura | 5 |

O excesso é cortado e o `mosaic check` avisa.

**Texto:** antes de ser usado, todo texto do widget perde os caracteres de controle e de formatação do Unicode (escape, NUL, direção de texto), quebras de linha e tabulações viram um espaço e os espaços repetidos são colapsados.

**Erros:**
- JSON inválido, raiz que não é objeto ou `summary.title` ausente: o widget inteiro mostra o erro.
- Bloco malformado (não é objeto, sem `type`, lista que não é lista, linha sem `text`, tipo de valor errado): só esse bloco é descartado e o widget ganha a linha `⚠ bloco inválido: <caminho>: <motivo>`. Veja o caminho em `mosaic check`, por exemplo `summary.blocks[2].rows[1].text`.
- Tipo de bloco, tom, flag ou campo desconhecido: ignorado, com aviso no `mosaic check`.

## 5. Avisos (`alerts`)

`{ "id", "title", "body"?, "level"?: "info" | "warn" }`, com `id` de 1 a 100 caracteres.

- A primeira leitura de cada widget depois que o Mosaic abre só memoriza os ids; ela não avisa. Depois disso, cada `id` novo avisa **uma vez**.
- Gere ids estáveis: o mesmo evento, o mesmo id. Reutilizar um id esconde o evento seguinte.
- `info` mostra uma caixa. `warn` também toca o sinal sonoro e roda o `notify_command` do pomodoro.
- Avisos simultâneos entram numa fila; qualquer tecla fecha o da frente.

## 6. Testando o widget: `mosaic check`

```sh
mosaic check <comando> [args...] [--env NOME=valor]... [--timeout SEGUNDOS] [--cwd DIRETORIO] [-- argumentos-do-provider...]
```

Roda `snapshot` com as mesmas regras de saída, tempo e protocolo do Mosaic (`--timeout` tem o mesmo padrão de `timeout_seconds`: 5 s), sem abrir o terminal. Por padrão o provider roda no diretório **atual**; use `--cwd ~/.config/mosaic` para reproduzir o diretório em que o Mosaic o executa. Para encaminhar opções próprias do provider, coloque `--` antes delas; a partir desse separador, todos os argumentos são repassados sem interpretação pelo Mosaic. O comando imprime os problemas e a prévia do widget em 60 colunas:

```text
ERRO summary.blocks[0].rows[2].text: campo obrigatório ausente
AVISO summary.extra: campo desconhecido

Agenda
  09:00  Daily
```

Saída `0` sem erros (avisos são permitidos), `1` com erros e `2` para uso incorreto, o que serve para testar o widget em integração contínua. Os widgets `plugins/gallery.py` e `plugins/gallery.js` mostram todos os blocos e servem de ponto de partida.
