---
name: mosaic-plugin-gitlab
description: Use quando for configurar, depurar ou alterar o widget de merge requests do GitLab (plugins/gitlab/gitlab_mrs.py), ou ao mexer em GITLAB_TOKEN_ENV, GITLAB_URL, as tags novo/push novo/aprovado ou o estado de revisões do GitLab.
---

# GitLab (merge requests)

## Visão geral

`plugins/gitlab/gitlab_mrs.py` mostra os merge requests do **GitLab.com** que esperam por você: "Aguardando minha revisão" e "Meus PRs" (o nome da seção é "PRs" mesmo para MRs, igual nos outros plugins de pull request). Só leitura (GraphQL sem mutation em `/api/graphql`), sem ações. Detalhes e roteiro manual: `plugins/gitlab/README.md`. Requisitos gerais: `mosaic-widget-authoring`.

## Configurar com a pessoa

1. **Token**: PAT em `https://gitlab.com/-/user_settings/personal_access_tokens`, escopo mínimo `read_api`. **Nunca peça o valor no chat**: ela o guarda numa variável (`GITLAB_TOKEN_MRS`) ou num arquivo `chmod 600` (`GITLAB_TOKEN_FILE`). O PAT vence e se troca à mão.
2. **Título** (opcional, `GITLAB_TITLE`).
3. É GitLab.com? Self-hosted não é testado: avise e não prometa funcionamento (`GITLAB_URL` existe só para teste local).
4. Duas contas são dois widgets, cada um com `id` e `[widgets.env]` próprios.

## Bloco do mosaic.toml

```toml
[[widgets]]
id = "gitlab-mrs"
command = "python3"
args = ["plugins/gitlab/gitlab_mrs.py"]
interval_seconds = 120
timeout_seconds = 30
inherit_env = ["GITLAB_TOKEN_MRS"]

[widgets.env]
GITLAB_TOKEN_ENV = "GITLAB_TOKEN_MRS"
```

## Verificar

```sh
mosaic check --cwd ~/.config/mosaic \
  --env GITLAB_TOKEN_ENV=GITLAB_TOKEN_MRS \
  --env GITLAB_STATE_DIR="$(mktemp -d)" \
  python3 plugins/gitlab/gitlab_mrs.py
```

Sem conta: `FAKE_PR_TOKEN=segredo python3 plugins/tests/fake_pull_request_server.py` (porta na primeira linha), depois `--env GITLAB_URL=http://127.0.0.1:PORTA --env GITLAB_TOKEN_ENV=FAKE_TOKEN` com `FAKE_TOKEN=segredo`. `GITLAB_STATE_DIR` temporário no `check`, para não sujar o estado do widget real. `check` não aplica `inherit_env`.

## Mexendo no código

- Testes: `python3 -m unittest discover -s plugins/tests -p test_gitlab_mrs.py`; fixtures e servidor falso são compartilhados com GitHub e Azure DevOps.
- "Aguardando minha revisão": MR aberto em que você é revisor e que **não** é seu; o que você aprovou sai. Um MR seu com autorrevisão fica só em "Meus PRs" (teste `test_own_merge_request_with_self_review_stays_only_in_my_section`).
- "Meus PRs": `aprovado` vem só da aprovação de **revisores designados**; projeto sem regra de aprovação não ganha `aprovado` por estar pronto para merge.
- `first: 100` por lista, sem paginação; draft nunca aparece; autor apagado vira `desconhecido`.
- `push novo` depende da memória local: só some quando a sua revisão muda de valor ou o MR sai da lista. Revisar de novo sem mudar de estado mantém a tag (limitação conhecida).
- Arquivo de estado = hash de URL base, origem do token e `GITLAB_TITLE`. O token nunca entra.
- Permanentes (token inválido, 400/404/410, consulta recusada): `erro:` sem cache. Rede, resposta inesperada e limite de requisições: cache com `dados de HH:MM (offline)`.
- `read_token`, `decode_response` e a classe de estado são iguais nos três plugins de PRs: corrija nos três.

## Armadilhas

| Sintoma | Causa | Correção |
| --- | --- | --- |
| Erro de permissão | PAT sem `read_api` | Criar outro com o escopo |
| Lista vazia | Você não é revisor designado dos MRs | O widget só mostra revisor pedido; peça a revisão no GitLab |
| `push novo` não some | Revisão repetida sem mudar de estado | Limitação conhecida |
| Self-hosted não funciona | Só GitLab.com é suportado | Backlog: self-hosted |
