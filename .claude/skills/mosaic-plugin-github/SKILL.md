---
name: mosaic-plugin-github
description: Use quando for configurar, depurar ou alterar o widget de pull requests do GitHub (plugins/github/github_prs.py), ou ao mexer em GITHUB_TOKEN_ENV, GITHUB_OWNERS, as tags novo/push novo/aprovado ou o estado de revisões do GitHub.
---

# GitHub (pull requests)

## Visão geral

`plugins/github/github_prs.py` mostra os PRs que esperam por você no **GitHub.com**: a seção "Aguardando minha revisão" e a seção "Meus PRs". Só leitura (GraphQL sem mutation), só `https://api.github.com`, sem ações. Detalhes, tabela de tags e roteiro manual: `plugins/github/README.md`. Requisitos gerais: `mosaic-widget-authoring`.

## Configurar com a pessoa

1. **Token**: PAT fine-grained (recomendado: `Pull requests: Read` e `Metadata: Read` nos repositórios acompanhados) ou classic com escopo `repo` (o único que alcança repositórios privados). A pessoa o cria em github.com/settings/tokens. **Nunca peça o valor no chat**; ela o guarda numa variável (`GITHUB_TOKEN_PRS`, por exemplo) ou num arquivo `chmod 600` (`GITHUB_TOKEN_FILE`). O token vence e se troca à mão.
2. **Donos**: todos os repositórios do token ou só alguns usuários e organizações? → `GITHUB_OWNERS` (separados por vírgula; vazio = todos).
3. **Título** (opcional, `GITHUB_TITLE`). Dois conjuntos de repositórios ou duas contas são dois widgets, cada um com `id` e `[widgets.env]` próprios.

## Bloco do mosaic.toml

```toml
[[widgets]]
id = "github-prs"
command = "python3"
args = ["plugins/github/github_prs.py"]
interval_seconds = 120
timeout_seconds = 30
inherit_env = ["GITHUB_TOKEN_PRS"]

[widgets.env]
GITHUB_TOKEN_ENV = "GITHUB_TOKEN_PRS"
GITHUB_OWNERS = "minha-org,minha-conta"
```

## Verificar

```sh
mosaic check --cwd ~/.config/mosaic \
  --env GITHUB_TOKEN_ENV=GITHUB_TOKEN_PRS \
  --env GITHUB_STATE_DIR="$(mktemp -d)" \
  python3 plugins/github/github_prs.py
```

Sem conta, suba `FAKE_PR_TOKEN=segredo python3 plugins/tests/fake_pull_request_server.py` (porta na primeira linha) e use `--env GITHUB_API_URL=http://127.0.0.1:PORTA --env GITHUB_TOKEN_ENV=FAKE_TOKEN` com `FAKE_TOKEN=segredo`; o roteiro completo está no README. Use sempre um `GITHUB_STATE_DIR` temporário no `check`, para não sujar a memória de revisões do widget real. `check` não aplica `inherit_env`.

## Mexendo no código

- Testes: `python3 -m unittest discover -s plugins/tests -p test_github_prs.py`. Fixtures e servidor falso são compartilhados com GitLab e Azure DevOps (`pull_request_fixtures.py`, `fake_pull_request_server.py`).
- Uma consulta GraphQL traz três buscas (revisão pedida, já revisados por você e PRs meus), unidas por `_keep_first` sem duplicar PR; `first: 100` sem paginação; draft nunca aparece; autor apagado vira `desconhecido`.
- "Meus PRs": o estado vem do `reviewDecision`; em repositório sem revisão obrigatória, `_state_from_reviews` deriva `aprovado` e `mudança pedida` das revisões de outras pessoas.
- `push novo` depende da **memória local** (`apply_review_memory`): guarda o último estado de revisão por PR. Sem o estado, não há `push novo`.
- Arquivo de estado = hash de URL da API, `GITHUB_OWNERS`, origem do token (nome da variável ou caminho) e `GITHUB_TITLE`. O token nunca entra. Mudar qualquer um desses zera memória e cache.
- Erros permanentes (token inválido ou sem permissão, 400/404/410, consulta recusada) mostram `erro:` e não usam cache; rede, resposta inesperada e limite de requisições (429, "rate limit", `RATE_LIMITED`) servem o cache com `dados de HH:MM (offline)`.
- Alerta `info` por PR novo a revisar, com id estável derivado do PR; o item leva `meta.url`.
- `read_token`, `decode_response` e a classe de estado são iguais nos três plugins de PRs: corrija nos três.
- Dívida: nunca validado contra conta real; GitHub Enterprise não é suportado.

## Armadilhas

| Sintoma | Causa | Correção |
| --- | --- | --- |
| Lista vazia com token válido | Token fine-grained sem acesso ao repositório, ou `GITHUB_OWNERS` errado | Dar acesso ao repositório; conferir donos |
| PR privado não aparece | Token classic sem `repo` | Escopo `repo` |
| `push novo` nunca aparece | `GITHUB_STATE_DIR` novo a cada execução | Deixar o padrão no widget real |
| Erro de token com caracteres inválidos | Espaço ou caractere não imprimível dentro do token (espaço nas pontas é aparado) | Salvar só o token, numa linha |
| Tag some após trocar `GITHUB_TITLE` | O título entra na chave do estado | Esperado: o estado recomeça |
