---
name: mosaic-plugin-azure-devops
description: Use quando for configurar, depurar ou alterar o widget de pull requests do Azure DevOps (plugins/azure-devops/azure_devops.py), ou ao mexer em AZDO_ORGANIZATION, AZDO_PROJECTS, AZDO_TOKEN_ENV, votos de revisores ou o estado de revisões do Azure.
---

# Azure DevOps (pull requests)

## Visão geral

`plugins/azure-devops/azure_devops.py` mostra os PRs ativos do **Azure DevOps Services** (`dev.azure.com`) que esperam por você: "Aguardando minha revisão" e "Meus PRs". Só leitura (`GET` na API REST `api-version` 7.1), sem ações. Detalhes e roteiro manual: `plugins/azure-devops/README.md`. Requisitos gerais: `mosaic-widget-authoring`.

## Configurar com a pessoa

1. **Organização**: só o nome (`minha-org`), nunca a URL → `AZDO_ORGANIZATION` (obrigatória).
2. **Projetos**: todos da organização ou alguns? → `AZDO_PROJECTS`, separados por vírgula, sem diferenciar maiúsculas de minúsculas.
3. **Token**: PAT em `https://dev.azure.com/<org>/_usersSettings/tokens`, escopo **Code (Read)**, validade de até um ano. **Nunca peça o valor no chat**: ela o guarda numa variável (`AZDO_TOKEN`) ou num arquivo `chmod 600` (`AZDO_TOKEN_FILE`).
4. Duas organizações são dois widgets, cada um com `id` e `[widgets.env]` próprios. Azure DevOps Server (on-premises) não é suportado.

## Bloco do mosaic.toml

```toml
[[widgets]]
id = "azure-devops-prs"
command = "python3"
args = ["plugins/azure-devops/azure_devops.py"]
interval_seconds = 120
timeout_seconds = 30
inherit_env = ["AZDO_TOKEN"]

[widgets.env]
AZDO_ORGANIZATION = "minha-org"
AZDO_TOKEN_ENV = "AZDO_TOKEN"
```

## Verificar

```sh
mosaic check --cwd ~/.config/mosaic \
  --env AZDO_ORGANIZATION=minha-org --env AZDO_TOKEN_ENV=AZDO_TOKEN \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-devops/azure_devops.py
```

Sem conta: `FAKE_PR_TOKEN=segredo python3 plugins/tests/fake_pull_request_server.py` (porta na primeira linha), depois `--env AZDO_BASE_URL=http://127.0.0.1:PORTA --env AZDO_ORGANIZATION=org --env AZDO_TOKEN_ENV=FAKE_TOKEN` com `FAKE_TOKEN=segredo`. `AZDO_STATE_DIR` temporário no `check`. `check` não aplica `inherit_env`.

## Mexendo no código

- Testes: `python3 -m unittest discover -s plugins/tests -p test_azure_devops.py`; fixtures e servidor falso são compartilhados com GitHub e GitLab.
- Primeiro pede `/_apis/connectionData` para saber quem é você (`authenticatedUser`), depois lista PRs ativos com `searchCriteria.reviewerId` e `searchCriteria.creatorId`, paginando com `$top=100` e `$skip` até 200 por seção.
- Votos: 10 e 5 aprovam; -5 (esperar pelo autor) e -10 (rejeitar) pedem mudança; 0 é sem voto → tag `novo`.
- "Aguardando minha revisão" exclui PRs criados por você e os que você aprovou. Revisão pedida a um **time** não aparece (exigiria as APIs Graph e Identities): só entra quem é revisor direto ou votou.
- Resposta 203 ou 3xx é a página de login: tratada como **token inválido**. Um 404 no primeiro acesso indica `AZDO_ORGANIZATION` errada (mensagem própria).
- Arquivo de estado = hash de URL base, organização, `AZDO_PROJECTS`, origem do token e `AZDO_TITLE`; o token nunca entra.
- Permanentes (token inválido, 400/404/410): `erro:` sem cache. Rede, resposta inesperada e limite de requisições: cache com `dados de HH:MM (offline)`.
- `push novo` depende da memória local e só some quando o seu voto muda ou o PR sai da lista.
- `read_token`, `decode_response` e a classe de estado são iguais nos três plugins de PRs: corrija nos três.

## Armadilhas

| Sintoma | Causa | Correção |
| --- | --- | --- |
| "organização não encontrada" | `AZDO_ORGANIZATION` errada ou com URL | Só o nome da organização |
| Token tratado como inválido com PAT novo | PAT criado para outra organização, vencido ou sem o escopo Code (Read) | Criar o PAT na organização certa, escopo Code (Read) |
| PR de time não aparece | Limitação das APIs usadas | Adicionar a pessoa como revisora direta |
| `AZDO_PROJECTS` não filtra | Nome com grafia diferente do projeto | A comparação ignora maiúsculas; confira o nome exato |
