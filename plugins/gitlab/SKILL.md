---
name: mosaic-plugin-gitlab
description: Use quando for configurar, depurar ou alterar o widget de merge requests do GitLab (plugins/gitlab/gitlab_mrs.py), ou ao mexer em GITLAB_TOKEN_ENV, GITLAB_URL, as tags novo/push novo/aprovado ou o estado de revisões do GitLab.
---

# GitLab (merge requests)

## Visão geral

`plugins/gitlab/gitlab_mrs.py` mostra os merge requests do **GitLab.com** que esperam por você: "Aguardando minha revisão" e "Meus PRs" (o nome da seção é "PRs" mesmo para MRs, igual nos outros plugins de pull request). Só leitura (GraphQL sem mutation em `/api/graphql`), sem ações. Detalhes e roteiro manual: `plugins/gitlab/README.md`. Requisitos gerais: `mosaic-widget-authoring`.

> Instalada por `mosaic plugins install` (junto com o plugin, em `~/.config/mosaic/plugins/gitlab/`), os caminhos `plugins/...` são os de `~/.config/mosaic/`. A pasta `plugins/tests/` (servidores falsos e testes) e a seção "Mexendo no código" só valem com o repositório clonado.

## Instalar e configurar com o mosaic plugins

Passos que a pessoa roda no terminal dela (agente: não rode o `setup` por ela, ele é interativo):

1. `mosaic plugins list`: lista os plugins disponíveis no pacote (`/usr/share/mosaic/plugins`).
2. `mosaic plugins install gitlab`: copia a pasta para `~/.config/mosaic/plugins/gitlab/` (script, `README.md`, esta `SKILL.md` e o `setup.toml`) e liga esta skill ao Claude Code em `~/.claude/skills/mosaic-plugin-gitlab`, se `~/.claude` existir. Não sobrescreve nada: depois de instalar um pacote novo, use `--force`. `--no-skill` pula a skill; `--from DIR`, `--to DIR` e `--skills-dir DIR` apontam para outras pastas (tarball, testes).
3. `mosaic plugins setup gitlab`: entrevista no terminal, só com o que o plugin já lê (veja "Todas as opções"). Mostra o `[[widgets]]` pronto e só grava no `mosaic.toml` depois de um "S". Nunca pede o valor do segredo: só o nome da variável ou o caminho do arquivo. `--print` só mostra o bloco, `--advanced` inclui as opções avançadas e `--config-dir DIR` usa outra pasta de configuração. Para um segundo widget (outra conta, outro filtro), rode de novo com outro id: cada execução acrescenta um bloco.
4. Aperte `R` no mosaic para recarregar; `mosaic check --config` valida o arquivo.

Dentro do Claude Code, a pessoa pode rodar `! mosaic plugins setup gitlab` sem sair da conversa. Se ela preferir conversar, siga a entrevista abaixo e entregue o bloco para colar.

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

## Todas as opções

O que o `mosaic plugins setup` pergunta, na ordem. Vem do `setup.toml` desta pasta, que um teste confere com o README e com o código do plugin.

| Opção | O que é | Padrão | Situação |
| --- | --- | --- | --- |
| `GITLAB_TOKEN_ENV` ou `GITLAB_TOKEN_FILE` | Token pessoal do GitLab (PAT com o escopo read_api) (nome da variável ou caminho do arquivo; o valor nunca é pedido) | — | obrigatória |
| `GITLAB_TITLE` | Título do widget | `GitLab` | opcional |
| `GITLAB_STATE_DIR` | Pasta do estado do widget (memória de revisões e cache offline) | `~/.local/state/mosaic/gitlab` | opcional; só com `--advanced` |

- Só para testes locais, o `setup` não pergunta: `GITLAB_URL`.
- Valores de partida do widget: `interval_seconds = 120` e `timeout_seconds = 30`; o id padrão é `gitlab-mrs`.

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
