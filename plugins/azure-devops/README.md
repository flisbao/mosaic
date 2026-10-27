# Azure DevOps

> Instalação: `mosaic plugins install azure-devops` (o plugin não vem no binário; veja o README da raiz).

`plugins/azure-devops/azure_devops.py` é um provider do Mosaic que mostra os pull requests do Azure DevOps que esperam por você: os que pediram sua revisão e os que você abriu. Ele só faz `GET` na API REST (`api-version` 7.1) e só fala com `https://dev.azure.com`. A autenticação é feita com **Microsoft Entra ID** (token de acesso curto, enviado como `Bearer`); o PAT não é mais suportado. Nenhum segredo vai em argumento: o plugin lê o **nome** de uma variável (ou um arquivo) e, no `device-code`, um arquivo de credencial só seu.

## O que mostra

O widget tem duas seções, sempre com estes nomes:

- **Aguardando minha revisão**: PRs ativos em que você é revisor e que não foram criados por você. PRs que você aprovou saem da lista.
- **Meus PRs**: PRs ativos que você abriu.

Cada item leva uma tag:

| Seção | Tag | Quando aparece |
| --- | --- | --- |
| Aguardando minha revisão | `novo` | Você ainda não votou no PR |
| Aguardando minha revisão | `push novo` | O autor fez push depois da sua última revisão (o item vem com a flag `updated`) |
| Aguardando minha revisão | nenhuma | Você já votou e não houve push depois |
| Meus PRs | `conflito` | O PR tem conflito com a base |
| Meus PRs | `mudança pedida` | Um revisor votou para esperar pelo autor ou rejeitou |
| Meus PRs | `aprovado` | Um revisor aprovou |
| Meus PRs | `aguardando` | Nenhuma das situações acima |

Drafts nunca aparecem. Em cada seção, os PRs vão do mais antigo para o mais novo. Os PRs sem data de criação são pulados, e autor sem nome aparece como `desconhecido`.

## Autenticação

A Microsoft está aposentando o PAT: os **PATs globais** deixam de funcionar em **01/12/2026**, e a orientação oficial é usar tokens do Microsoft Entra ID. Escolha o método em `AZDO_AUTH_METHOD`:

| Método | Quando usar | Como o token é obtido |
| --- | --- | --- |
| `azure-cli` (padrão) | Máquina pessoal com `az login` | O widget roda `az account get-access-token` |
| `device-code` | Sem o `az`, com um app registrado no Entra | `login` interativo uma vez; depois o refresh token renova o acesso |
| `service-principal` | Automação fora do Azure | Segredo de um app registrado (client credentials) |
| `managed-identity` | Mosaic numa VM do Azure com identidade gerenciada | IMDS (`169.254.169.254`) |

Em todos os métodos, o Azure DevOps autoriza pela **identidade** no Azure DevOps, não por escopos do Entra: a pessoa (ou o app) precisa estar na organização com acesso **Basic** e permissão de leitura nos repositórios. Contas pessoais da Microsoft (MSA) não funcionam com o Entra para o Azure DevOps.

### azure-cli

1. Instale o [Azure CLI](https://learn.microsoft.com/cli/azure/install-azure-cli) e rode `az login` com a conta que acessa a organização.
2. Use `AZDO_AUTH_METHOD = "azure-cli"` (ou nada: é o padrão). Se a sua conta tem vários diretórios, informe `AZDO_TENANT_ID`.
3. O Mosaic precisa achar o `az` no `PATH` e ler a pasta do `az` em `~/.azure`. Se você usa `AZURE_CONFIG_DIR`, liste-a em `inherit_env`.

### device-code

1. No [Microsoft Entra admin center](https://entra.microsoft.com): **App registrations > New registration**. Em **Authentication**, ative **Allow public client flows**. Em **API permissions > Add a permission > Azure DevOps**, escolha **Delegated** e `user_impersonation`. Anote o **Application (client) ID** e o **Directory (tenant) ID**.
2. Rode o login uma vez, no terminal (não pelo Mosaic), com as mesmas variáveis do widget:

   ```sh
   AZDO_AUTH_METHOD=device-code AZDO_CLIENT_ID=<client-id> AZDO_TENANT_ID=<tenant-id> \
     python3 ~/.config/mosaic/plugins/azure-devops/azure_devops.py login
   ```

   O comando mostra a URL e o código para abrir no navegador e, ao terminar, imprime `login concluído`.
3. O refresh token fica em `AZDO_CREDENTIALS_DIR` (pasta `0700`, arquivo `0600`), separado do estado do widget. A cada leitura o widget troca o refresh token por um token de acesso novo. Se o Entra revogar o login, o widget apaga o arquivo e pede `login` de novo.

### service-principal

1. Registre um app no Entra (**App registrations**) e crie um **client secret**. Certificado e credencial federada ainda não são suportados.
2. Adicione o app à organização: **Organization settings > Users > Add users**, com o nome do app, acesso **Basic** e os projetos. Se o Azure DevOps não achar o app, use o **Object ID** de **Enterprise applications**, não o de App registrations.
3. Guarde o segredo numa variável ou num arquivo `chmod 600` e informe só o **nome** em `AZDO_CLIENT_SECRET_ENV` (ou o caminho em `AZDO_CLIENT_SECRET_FILE`). Informe também `AZDO_CLIENT_ID` e `AZDO_TENANT_ID` (um diretório específico: `organizations` e `common` não servem).

### managed-identity

Só funciona numa VM (ou serviço) do Azure com identidade gerenciada. Adicione a identidade à organização como no `service-principal`. Para uma identidade atribuída pelo usuário, informe o client id em `AZDO_CLIENT_ID`.

### Migrando do PAT

`AZDO_TOKEN_ENV` e `AZDO_TOKEN_FILE` passam a ser recusados com uma mensagem de orientação. Para migrar: rode `az login`, troque as duas variáveis por `AZDO_AUTH_METHOD = "azure-cli"` e tire o PAT de `inherit_env`. Revogue o PAT antigo.

## Configurar

Adicione ao `mosaic.toml` (o bloco também vem comentado no arquivo instalado pelo Mosaic):

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

Para o `service-principal`, acrescente `AZDO_TENANT_ID`, `AZDO_CLIENT_ID` e `AZDO_CLIENT_SECRET_ENV = "AZDO_SEGREDO"` em `[widgets.env]` e `inherit_env = ["AZDO_SEGREDO"]`. Para duas organizações, são dois widgets, cada um com `id` próprio e o seu `[widgets.env]`.

### Variáveis de ambiente

Vão em `[widgets.env]` (o segredo, por `inherit_env` ou `AZDO_CLIENT_SECRET_FILE`).

| Variável | Padrão | O que faz |
| --- | --- | --- |
| `AZDO_ORGANIZATION` | obrigatória | Nome da organização, ex.: `minha-org` (só o nome, sem URL) |
| `AZDO_PROJECTS` | vazio | Projetos separados por vírgula, sem diferenciar maiúsculas de minúsculas. Vazio = todos os projetos da organização |
| `AZDO_AUTH_METHOD` | `azure-cli` | `azure-cli`, `device-code`, `service-principal` ou `managed-identity` |
| `AZDO_TENANT_ID` | `organizations` | Directory (tenant) ID; obrigatório no `service-principal` |
| `AZDO_CLIENT_ID` | | Application (client) ID; obrigatório no `device-code` e no `service-principal`; opcional no `managed-identity` (identidade atribuída pelo usuário) |
| `AZDO_CLIENT_SECRET_ENV` | | Nome da variável que contém o segredo do app (`service-principal`) |
| `AZDO_CLIENT_SECRET_FILE` | | Caminho de um arquivo com o segredo (alternativa a `AZDO_CLIENT_SECRET_ENV`) |
| `AZDO_CREDENTIALS_DIR` | `~/.local/state/mosaic/azure-devops-credentials` | Pasta (`0700`) do refresh token do `device-code` |
| `AZDO_TITLE` | `Azure DevOps` | Título do widget |
| `AZDO_STATE_DIR` | `~/.local/state/mosaic/azure-devops` | Pasta do estado de cada widget (memória de revisões e cache offline) |
| `AZDO_BASE_URL` | `https://dev.azure.com` | Só para testes: `http://127.0.0.1` ou `localhost` (ou https) no lugar do Azure DevOps |
| `AZDO_LOGIN_URL` | `https://login.microsoftonline.com` | Só para testes: o servidor do Entra |
| `AZDO_IMDS_URL` | `http://169.254.169.254` | Só para testes: o IMDS |

## Testar

Com a sua conta, faça `az login` e rode o provider com `mosaic check` (o `--cwd` é a pasta onde o Mosaic instalou os plugins):

```sh
az login
mosaic check --cwd ~/.config/mosaic \
  --env AZDO_ORGANIZATION=minha-org \
  --env AZDO_AUTH_METHOD=azure-cli \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-devops/azure_devops.py
```

Sem rede e sem conta, use os servidores falsos dos testes (Azure DevOps e Entra). Cada um imprime a porta na primeira linha. Rode os comandos a partir da raiz do repositório:

```sh
export FAKE_PR_TOKEN=token-falso FAKE_ENTRA_SECRET=segredo-falso
ARQUIVO_AZURE=$(mktemp); ARQUIVO_ENTRA=$(mktemp)
python3 plugins/tests/fake_pull_request_server.py > "$ARQUIVO_AZURE" &
PID_AZURE=$!
python3 plugins/tests/fake_entra_server.py > "$ARQUIVO_ENTRA" &
PID_ENTRA=$!
until [ -s "$ARQUIVO_AZURE" ] && [ -s "$ARQUIVO_ENTRA" ]; do sleep 0.1; done
PORTA_AZURE=$(head -1 "$ARQUIVO_AZURE")
PORTA_ENTRA=$(head -1 "$ARQUIVO_ENTRA")
export SEGREDO_FALSO=segredo-falso
mosaic check \
  --env AZDO_BASE_URL=http://127.0.0.1:$PORTA_AZURE \
  --env AZDO_LOGIN_URL=http://127.0.0.1:$PORTA_ENTRA \
  --env AZDO_ORGANIZATION=org \
  --env AZDO_AUTH_METHOD=service-principal \
  --env AZDO_TENANT_ID=tenant-1 \
  --env AZDO_CLIENT_ID=client-1 \
  --env AZDO_CLIENT_SECRET_ENV=SEGREDO_FALSO \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-devops/azure_devops.py
kill "$PID_AZURE" "$PID_ENTRA"
```

## Estado

Cada widget guarda o seu estado (memória das revisões e cache offline) num arquivo próprio em `AZDO_STATE_DIR`. O nome do arquivo é um hash da URL base, da organização, de `AZDO_PROJECTS`, do método de autenticação, do tenant, do client id e de `AZDO_TITLE`. Nenhum token nem segredo entra na chave nem no arquivo. Trocar o título, a organização, os projetos ou a identidade de autenticação cria outro arquivo: a memória do `push novo` e o cache offline começam vazios.

## Limitações

- Só Azure DevOps Services na nuvem (`dev.azure.com`). Azure DevOps Server (on-premises) não é suportado nesta entrega.
- Só leitura: o widget não declara ações.
- Só Microsoft Entra ID. Contas pessoais da Microsoft (MSA) não funcionam; o `service-principal` aceita só client secret (não certificado nem credencial federada).
- O `device-code` exige rodar `login` de novo quando o Entra revogar ou vencer o refresh token.
- Cada seção lê no máximo 200 PRs.
- Revisão pedida a um time não aparece: o PR só entra em **Aguardando minha revisão** quando você vota nele ou é adicionado diretamente como revisor. Mapear um time para os seus membros exige as APIs Graph e Identities, que ainda não são usadas.
- O `push novo` depende da memória local: ele some só quando a sua revisão muda de valor ou o PR sai da lista. Se você revisar de novo depois de um push sem mudar o seu voto, a tag `push novo` continua aparecendo até lá.

## Quando a API não responde

Se a rede falhar, a API responder com erro inesperado ou o limite de requisições do Azure DevOps estourar, o widget mostra o último resultado bom com o aviso `dados de HH:MM (offline)` (`dados de DD/MM HH:MM (offline)` quando o resultado não é de hoje). Sem nenhum resultado guardado, mostra o erro. Erros permanentes **não** usam o cache e aparecem como `erro: ...`: credencial inválida ou sem permissão para Azure DevOps (inclui `az` sem sessão, login do Entra vencido e identidade que não foi adicionada à organização), requisição recusada (HTTP 400, 404 ou 410; um 404 no primeiro acesso indica `AZDO_ORGANIZATION` errada). Nesses casos, confira a configuração do widget e a versão do plugin.

## Roteiro de teste manual

Com a sua conta, em um projeto de teste:

1. Peça a sua revisão em um PR: aparece `novo` em **Aguardando minha revisão**.
2. Peça mudança (vote para esperar pelo autor) e faça o autor enviar um push: aparece `push novo`.
3. Aprove o PR: ele some da lista.
4. Abra um PR seu: aparece `aguardando` em **Meus PRs**.
