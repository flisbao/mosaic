# Protocolo de widgets do Mosaic (v1)

Um widget é qualquer programa que o painel executa. Ele não precisa ser escrito em Rust: Python, Node, shell, o que você preferir. Este documento é o contrato. O esquema JSON equivalente está em [`schema/protocol.schema.json`](schema/protocol.schema.json) e os testes de conformidade garantem que os dois não divergem do código.

## 1. Chamada

O painel roda o comando configurado no widget (`command` + `args`) com um argumento a mais:

| Chamada | Quando | Entrada | Saída esperada |
| --- | --- | --- | --- |
| `<comando> snapshot` | a cada `interval_seconds` e quando o usuário aperta `r` | nenhuma | um JSON na saída padrão (formato da seção 4) |
| `<comando> action <id>` | quando o usuário confirma uma ação | JSON com os parâmetros na entrada padrão | uma mensagem de texto curta |

Regras da execução:

- **Ambiente:** o processo recebe o ambiente do painel mais as variáveis do bloco `[widgets.env]`.
- **Diretório de trabalho:** o diretório onde o painel foi iniciado.
- **Limites:** a saída pode ter no máximo **1 MiB** e o widget tem `timeout_seconds` (padrão 5 s) para responder. Passou do limite, a execução é encerrada e o card mostra o erro.
- **Código de saída:** diferente de zero vira erro no card, com o que foi escrito na saída de erro.
- **Intervalo mínimo:** `interval_seconds` abaixo de **30** é elevado para 30 e o card mostra `intervalo ajustado para 30s`. A tecla `r` não tem esse limite. O padrão também é 30.

## 2. Versão

O campo `"protocol": 1` é opcional; sem ele vale 1. Um número inteiro maior que 1 faz o painel mostrar `este widget exige o protocolo N; atualize o mosaic`. Qualquer outro valor (texto, `0`, negativo, decimal, `null`) é um erro: `protocol: precisa ser um número inteiro positivo`.

Campos que o painel não conhece, em qualquer nível, são ignorados (o `mosaic check` avisa). Isso permite que um widget novo continue funcionando em um painel mais antigo.

## 3. Texto

Todo texto vindo do widget (títulos, linhas, etiquetas, células, rótulos, avisos) é limpo antes de ser usado:

- caracteres de controle e de formatação do Unicode (sequências de escape, NUL, controles de direção do texto) são removidos;
- quebras de linha e tabulações viram um espaço, espaços repetidos são colapsados e as pontas são aparadas;
- cada texto vale no máximo **500 caracteres**; o excesso é cortado com `…`.

## 4. Formato do snapshot

```json
{
  "protocol": 1,
  "summary": { "title": "Agenda", "blocks": [ ... ] },
  "detail": { "sections": [ { "heading": "Hoje", "blocks": [ ... ], "items": [ ... ] } ] },
  "actions": [ { "id": "refresh", "label": "Atualizar" } ],
  "alerts": [ { "id": "deploy-42", "title": "Deploy falhou", "level": "warn" } ]
}
```

- `summary.title` é obrigatório e aparece no título da caixa na Home.
- `summary.blocks` é o conteúdo da caixa (seção 5). Se `blocks` não existir, vale o formato legado `summary.lines`: `[{"text": "...", "flag"?: "..."}]`. Se os dois existirem, `blocks` vale.
- `detail` alimenta a aba do widget (só aparece se o widget tiver `tab = true`). Cada seção tem `heading`, e opcionalmente `blocks` (desenhados primeiro) e `items` (a lista selecionável: `{"id"?, "text", "flag"?, "meta"?}`).
- `actions` (inalteradas desde a versão anterior): `{"id", "label", "description"?, "applies_to"?: "widget" | "item", "params"?}`.

### Blocos

Um bloco é um objeto com `type`. Um card tem no máximo **20 blocos**. Blocos são empilhados na ordem em que aparecem.

**`lines`** — linhas de texto (máximo 200).

```json
{ "type": "lines", "lines": [
  { "text": "tudo certo", "tone": "ok" },
  { "text": "atenção", "tone": "warn", "flag": "warning" }
] }
```

**`rows`** — linhas com hora, marcador e etiquetas (máximo 200). `marker` é uma string de **um** caractere; `tags` aceita no máximo **3** etiquetas por linha, alinhadas à direita.

```json
{ "type": "rows", "rows": [
  { "time": "09:00", "text": "Daily", "tone": "muted" },
  { "time": "10:30", "text": "Revisão", "tone": "current", "marker": "▶",
    "tags": [ { "text": "agora", "tone": "ok" } ] }
] }
```

**`stat`** — indicadores lado a lado (máximo 8), com uma dica opcional abaixo. Indicadores que não cabem na largura quebram para a linha seguinte.

```json
{ "type": "stat", "items": [
  { "value": "3", "label": "reuniões", "tone": "warn" },
  { "value": "2h", "label": "livre", "tone": "ok" }
], "hint": "atualizado há 1 min" }
```

**`table`** — tabela com cabeçalho (máximo 10 colunas e 200 linhas). Cada célula é um texto ou um objeto `{ "text", "tone"?, "flag"? }`. `align` da coluna é `left` (padrão) ou `right`. Linha com mais células que colunas tem o excesso cortado; com menos, é completada com células vazias (ambos geram aviso no `mosaic check`). Em terminais estreitos a coluna mais larga encolhe e, se nada couber, as colunas da direita são omitidas.

```json
{ "type": "table",
  "columns": [ { "title": "Pessoa" }, { "title": "Em andamento", "align": "right" } ],
  "rows": [ [ "Ana", "3" ], [ { "text": "Bia", "tone": "danger", "flag": "warning" }, "5" ] ] }
```

Um `type` desconhecido é ignorado (aviso no `mosaic check`).

### Tons e flags

| Tom | Cor no tema padrão |
| --- | --- |
| `normal` | padrão do terminal |
| `muted` | cinza escuro |
| `ok` | verde |
| `warn` | amarelo |
| `danger` | vermelho |
| `current` | ciano em negrito |

Um tom desconhecido vira `normal` (aviso no `check`). As cores são fixas; ainda não há tema configurável.

| Flag | Símbolo |
| --- | --- |
| `updated` | `●` |
| `warning` | `⚠` |

Uma flag desconhecida é ignorada.

## 5. Erros

- **Falha do snapshot inteiro** (JSON inválido, raiz que não é um objeto, `summary.title` ausente, tempo esgotado, saída acima de 1 MiB, código de saída diferente de zero): o card mostra o erro.
- **Bloco malformado** (não é objeto, `type` ausente, lista que não é lista, linha sem `text`, valor obrigatório de tipo errado): o bloco é descartado, o resto do card é desenhado e o card ganha como primeira linha `⚠ bloco inválido: <caminho>: <motivo>`. No `mosaic check` isso é um **erro**.
- **Excessos e deslizes pequenos** (campo desconhecido, tom desconhecido, excesso de linhas, etiquetas ou colunas, texto longo): são cortados ou ignorados e aparecem como **aviso** no `mosaic check`.

O caminho segue o JSON, por exemplo `summary.blocks[2].rows[1].text`.

## 6. Avisos (`alerts`)

```json
"alerts": [ { "id": "deploy-42", "title": "Deploy falhou", "body": "serviço api", "level": "warn" } ]
```

- `id` é obrigatório (texto não vazio de até 100 caracteres) e identifica o aviso entre uma leitura e outra; `title` é obrigatório; `body` é opcional; `level` é `info` (padrão) ou `warn`. Avisos sem `id` válido ou sem `title` são descartados com aviso no `check`.
- **A primeira leitura de cada widget depois que o painel abre apenas memoriza os ids; ela não avisa.** Depois disso, cada `id` novo avisa **uma vez**. Reaparecer na leitura seguinte não repete o aviso.
- No máximo **5** avisos novos por leitura; o excesso é descartado. Por isso gere ids estáveis (o mesmo evento, o mesmo id) e não reutilize ids para eventos diferentes.
- `info` só mostra a caixa. `warn` mostra a caixa, toca o sinal sonoro e roda o comando externo do pomodoro (`[pomodoro] bell` e `notify_command`).
- Avisos simultâneos entram numa fila, mostrados um por vez; qualquer tecla fecha o da frente.

## 7. Testando seu widget: `mosaic check`

```sh
mosaic check <comando> [args...] [--env NOME=valor]...
```

Roda `snapshot` com as mesmas regras do painel (ambiente, diretório atual, limite de 1 MiB, tempo limite de 10 s), sem abrir o terminal. Imprime os problemas e depois o título e a prévia do card em 60 colunas:

```text
ERRO summary.blocks[0].rows[2].text: campo obrigatório ausente
AVISO summary.extra: campo desconhecido

Agenda
  09:00  Daily
```

Código de saída: `0` sem erros (avisos são permitidos), `1` com erros, `2` para uso incorreto. Serve também como teste automático do seu widget em integração contínua:

```sh
mosaic check python3 plugins/gallery.py
mosaic check node plugins/gallery.js
mosaic check python3 plugins/kanban.py --env KANBAN_DIR=~/kanban
```

Os widgets `plugins/gallery.py` e `plugins/gallery.js` mostram todos os blocos e um aviso, e servem de ponto de partida.
