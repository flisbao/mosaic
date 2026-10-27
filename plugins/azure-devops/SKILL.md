---
name: mosaic-plugin-azure-devops
description: Use quando for configurar, depurar ou alterar o widget de pull requests do Azure DevOps (plugins/azure-devops/azure_devops.py), ou ao mexer em AZDO_ORGANIZATION, AZDO_PROJECTS, AZDO_AUTH_METHOD, login do Entra, votos de revisores ou o estado de revisões do Azure.
---

# Azure DevOps (pull requests)

## Visão geral

`plugins/azure-devops/azure_devops.py` mostra os PRs ativos do **Azure DevOps Services** (`dev.azure.com`) que esperam por você: "Aguardando minha revisão" e "Meus PRs". Só leitura (`GET` na API REST `api-version` 7.1), sem ações. A autenticação é **Microsoft Entra ID** (token `Bearer` curto); o PAT foi removido. Detalhes e roteiro manual: `plugins/azure-devops/README.md`. Requisitos gerais: `mosaic-widget-authoring`.

> Instalada por `mosaic plugins install` (junto com o plugin, em `~/.config/mosaic/plugins/azure-devops/`), os caminhos `plugins/...` são os de `~/.config/mosaic/`. A pasta `plugins/tests/` (servidores falsos e testes) e a seção "Mexendo no código" só valem com o repositório clonado.

## Instalar e configurar com o mosaic plugins

Passos que a pessoa roda no terminal dela (agente: não rode o `setup` por ela, ele é interativo):

1. `mosaic plugins list`: lista os plugins disponíveis no pacote (`/usr/share/mosaic/plugins`).
2. `mosaic plugins install azure-devops`: copia a pasta para `~/.config/mosaic/plugins/azure-devops/` (script, `README.md`, esta `SKILL.md` e o `setup.toml`) e liga esta skill ao Claude Code em `~/.claude/skills/mosaic-plugin-azure-devops`, se `~/.claude` existir. Não sobrescreve nada: depois de instalar um pacote novo, use `--force`. `--no-skill` pula a skill; `--from DIR`, `--to DIR` e `--skills-dir DIR` apontam para outras pastas (tarball, testes).
3. `mosaic plugins setup azure-devops`: entrevista no terminal, só com o que o plugin já lê (veja "Todas as opções"). Mostra o `[[widgets]]` pronto e só grava no `mosaic.toml` depois de um "S". Nunca pede o valor do segredo: só o nome da variável ou o caminho do arquivo. `--print` só mostra o bloco, `--advanced` inclui as opções avançadas e `--config-dir DIR` usa outra pasta de configuração. Para um segundo widget (outra conta, outro filtro), rode de novo com outro id: cada execução acrescenta um bloco.
4. Aperte `R` no mosaic para recarregar; `mosaic check --config` valida o arquivo.

Dentro do Claude Code, a pessoa pode rodar `! mosaic plugins setup azure-devops` sem sair da conversa. Se ela preferir conversar, siga a entrevista abaixo e entregue o bloco para colar.

## Configurar com a pessoa

1. **Organização**: só o nome (`minha-org`), nunca a URL → `AZDO_ORGANIZATION` (obrigatória).
2. **Projetos**: todos da organização ou alguns? → `AZDO_PROJECTS`, separados por vírgula, sem diferenciar maiúsculas de minúsculas.
3. **Método de autenticação** → `AZDO_AUTH_METHOD`. Pergunte nesta ordem:
   - Tem o Azure CLI e faz `az login`? → `azure-cli` (padrão, não precisa registrar app).
   - Não tem o `az`, mas pode registrar um app no Entra? → `device-code`: `AZDO_CLIENT_ID` e `AZDO_TENANT_ID`; a pessoa roda `python3 plugins/azure-devops/azure_devops.py login` **no terminal**, com as mesmas variáveis, uma vez.
   - Automação fora do Azure? → `service-principal`: `AZDO_CLIENT_ID`, `AZDO_TENANT_ID` (um diretório específico) e `AZDO_CLIENT_SECRET_ENV`.
   - Mosaic numa VM do Azure? → `managed-identity` (`AZDO_CLIENT_ID` só para identidade atribuída pelo usuário).
4. **Segredo**: **nunca peça o valor no chat.** Ela o guarda numa variável (nome em `AZDO_CLIENT_SECRET_ENV`) ou num arquivo `chmod 600` (`AZDO_CLIENT_SECRET_FILE`) e o widget declara `inherit_env` com o nome da variável. O `azure-cli`, o `device-code` e o `managed-identity` não têm segredo no `mosaic.toml`.
5. Quem usava PAT: `AZDO_TOKEN_ENV` e `AZDO_TOKEN_FILE` agora dão erro com orientação; troque por `AZDO_AUTH_METHOD`.
6. Duas organizações são dois widgets, cada um com `id` e `[widgets.env]` próprios. Azure DevOps Server (on-premises) e contas pessoais Microsoft (MSA) não são suportados.

## Bloco do mosaic.toml

```toml
[[widgets]]
id = "azure-devops-prs"
command = "python3"
args = ["plugins/azure-devops/azure_devops.py"]
interval_seconds = 120
timeout_seconds = 30
inherit_env = []

[widgets.env]
AZDO_ORGANIZATION = "minha-org"
AZDO_AUTH_METHOD = "azure-cli"
```

Para o `service-principal`: `inherit_env = ["AZDO_SEGREDO"]` e, em `[widgets.env]`, `AZDO_TENANT_ID`, `AZDO_CLIENT_ID` e `AZDO_CLIENT_SECRET_ENV = "AZDO_SEGREDO"`.

## Todas as opções

O que o `mosaic plugins setup` pergunta, na ordem. Vem do `setup.toml` desta pasta, que um teste confere com o README e com o código do plugin.

| Opção | O que é | Padrão | Situação |
| --- | --- | --- | --- |
| `AZDO_ORGANIZATION` | Nome da organização do Azure DevOps (só o nome, sem URL) | — | obrigatória |
| `AZDO_PROJECTS` | Projetos, sem diferenciar maiúsculas de minúsculas (Enter = todos os projetos da organização) (separados por vírgula) | — | opcional |
| `AZDO_AUTH_METHOD` | Como o widget se autentica no Entra: `azure-cli`, `device-code`, `service-principal`, `managed-identity` | `azure-cli` | opcional |
| `AZDO_TENANT_ID` | Directory (tenant) ID (padrão: organizations) | — | obrigatória se `AZDO_AUTH_METHOD` = `service-principal`; perguntada se `AZDO_AUTH_METHOD` = `azure-cli` ou `device-code` ou `service-principal` |
| `AZDO_CLIENT_ID` | Application (client) ID do app registrado no Entra (na managed-identity, só para identidade atribuída pelo usuário) | — | obrigatória se `AZDO_AUTH_METHOD` = `device-code` ou `service-principal`; perguntada se `AZDO_AUTH_METHOD` = `device-code` ou `service-principal` ou `managed-identity` |
| `AZDO_CLIENT_SECRET_ENV` ou `AZDO_CLIENT_SECRET_FILE` | Segredo do app registrado no Entra (nome da variável ou caminho do arquivo; o valor nunca é pedido) | — | obrigatória; perguntada se `AZDO_AUTH_METHOD` = `service-principal` |
| `AZDO_TITLE` | Título do widget | `Azure DevOps` | opcional |
| `AZDO_CREDENTIALS_DIR` | Pasta (0700) do refresh token do device-code | `~/.local/state/mosaic/azure-devops-credentials` | opcional; perguntada se `AZDO_AUTH_METHOD` = `device-code`; só com `--advanced` |
| `AZDO_STATE_DIR` | Pasta do estado do widget (memória de revisões e cache offline) | `~/.local/state/mosaic/azure-devops` | opcional; só com `--advanced` |

- Só para testes locais, o `setup` não pergunta: `AZDO_BASE_URL`, `AZDO_LOGIN_URL`, `AZDO_IMDS_URL`.
- Valores de partida do widget: `interval_seconds = 120` e `timeout_seconds = 30`; o id padrão é `azure-devops-prs`.

## Verificar

```sh
az login
mosaic check --cwd ~/.config/mosaic \
  --env AZDO_ORGANIZATION=minha-org --env AZDO_AUTH_METHOD=azure-cli \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-devops/azure_devops.py
```

Sem conta: `FAKE_PR_TOKEN=token-falso python3 plugins/tests/fake_pull_request_server.py` e `FAKE_PR_TOKEN=token-falso FAKE_ENTRA_SECRET=segredo-falso python3 plugins/tests/fake_entra_server.py` (a porta vem na primeira linha de cada um), depois `--env AZDO_BASE_URL=http://127.0.0.1:PORTA_AZURE --env AZDO_LOGIN_URL=http://127.0.0.1:PORTA_ENTRA` com `service-principal` (roteiro completo no README). Use `AZDO_STATE_DIR` temporário no `check`. `check` não aplica `inherit_env`: confira-o à mão.

## Mexendo no código

- Testes: `python3 -m unittest discover -s plugins/tests -p test_azure_devops.py` e `-p test_entra_auth.py` (os mesmos testes de autenticação rodam contra os dois plugins que têm o bloco). Fixtures e servidor falso de PRs são compartilhados com GitHub e GitLab.
- O bloco entre `# --- entra-auth: início ---` e `# --- entra-auth: fim ---` é **idêntico** ao do plugin `azure-pipelines`: mude os dois (um teste confere). Provedores de token (`AzureCliTokenProvider`, `ServicePrincipalTokenProvider`, `ManagedIdentityTokenProvider`, `DeviceCodeTokenProvider`) são escolhidos por `token_provider_for`; um método novo entra em `AUTH_PROVIDER_FACTORIES` sem mexer nos outros.
- O token é opaco: nunca decodifique o JWT. Mensagem de erro do Entra é fixa por código (`ENTRA_ERROR_MESSAGES`); nunca ecoe `error_description` nem o stderr do `az`.
- O refresh token do `device-code` mora só em `AZDO_CREDENTIALS_DIR` (pasta `0700`, arquivo `0600`), nunca em `AZDO_STATE_DIR`.
- Primeiro pede `/_apis/connectionData` para saber quem é você (`authenticatedUser`), depois lista PRs ativos com `searchCriteria.reviewerId` e `searchCriteria.creatorId`, paginando com `$top=100` e `$skip` até 200 por seção.
- Votos: 10 e 5 aprovam; -5 (esperar pelo autor) e -10 (rejeitar) pedem mudança; 0 é sem voto → tag `novo`.
- "Aguardando minha revisão" exclui PRs criados por você e os que você aprovou. Revisão pedida a um **time** não aparece (exigiria as APIs Graph e Identities): só entra quem é revisor direto ou votou.
- Resposta 203 ou 3xx é a página de login: **credencial inválida**. Um 401 com `TF401444` é identidade fora da organização (mensagem própria). Um 404 no primeiro acesso indica `AZDO_ORGANIZATION` errada.
- Arquivo de estado = hash de URL base, organização, `AZDO_PROJECTS`, método, tenant, client id e `AZDO_TITLE`; nenhum segredo entra.
- Permanentes (credencial inválida, 400/404/410, `az` sem sessão, `login` pendente): `erro:` sem cache. Rede (inclui IMDS fora do ar e `az` lento), resposta inesperada e limite de requisições: cache com `dados de HH:MM (offline)`. `az` ausente é erro de configuração.
- `push novo` depende da memória local e só some quando o seu voto muda ou o PR sai da lista.
- `decode_response` e a classe de estado são iguais nos três plugins de PRs: corrija nos três.

## Armadilhas

| Sintoma | Causa | Correção |
| --- | --- | --- |
| "organização não encontrada" | `AZDO_ORGANIZATION` errada ou com URL | Só o nome da organização |
| "PAT não é mais suportado" | `AZDO_TOKEN_ENV` ou `AZDO_TOKEN_FILE` ainda no `mosaic.toml` | Trocar por `AZDO_AUTH_METHOD`; revogar o PAT |
| "a identidade não foi adicionada à organização" | App ou identidade fora de Organization settings > Users, ou sem acesso Basic | Adicionar com o Object ID de Enterprise applications |
| "Azure CLI (az) não encontrado" | `az` fora do `PATH` do Mosaic | Instalar o `az` ou usar outro método; conferir o `PATH` |
| "o az não tem sessão válida" | `az login` vencido ou feito com outra conta | Rodar `az login` de novo |
| "sem login do Entra; rode: ... login" | `device-code` sem `login` ou refresh token revogado | Rodar `login` com as mesmas variáveis do widget |
| `login` não vale para o widget | `AZDO_TENANT_ID`, `AZDO_CLIENT_ID` ou `AZDO_LOGIN_URL` diferentes entre o `login` e o widget | Usar os mesmos valores nos dois |
| "IMDS indisponível" | `managed-identity` fora de uma VM do Azure | Usar `azure-cli` ou `service-principal` |
| PR de time não aparece | Limitação das APIs usadas | Adicionar a pessoa como revisora direta |
| `AZDO_PROJECTS` não filtra | Nome com grafia diferente do projeto | A comparação ignora maiúsculas; confira o nome exato |
