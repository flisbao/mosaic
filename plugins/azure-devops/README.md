# Azure DevOps

`plugins/azure-devops/azure_devops.py` é um provider do Mosaic que mostra os pull requests do Azure DevOps que esperam por você: os que pediram sua revisão e os que você abriu. Ele só faz `GET` na API REST (`api-version` 7.1) e só fala com `https://dev.azure.com`. O token nunca vai em argumento: `AZDO_TOKEN_ENV` guarda o **nome** da variável que contém o token (ou use `AZDO_TOKEN_FILE`).

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

## Token

Crie um PAT em `https://dev.azure.com/<sua-org>/_usersSettings/tokens`. O escopo mínimo é **Code (Read)**.

Guarde o token numa variável de ambiente (`AZDO_TOKEN`, por exemplo) ou num arquivo. Se usar arquivo, deixe-o só para o seu usuário:

```sh
chmod 600 ~/.config/mosaic/azdo-token
```

O PAT vence (no máximo um ano) e precisa ser recriado à mão. Veja as limitações.

## Configurar

Adicione ao `config.toml` (o bloco também vem comentado no arquivo instalado pelo Mosaic):

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

Para duas organizações, são dois widgets, cada um com `id` próprio e o seu `[widgets.env]`.

### Variáveis de ambiente

Vão em `[widgets.env]` (o token, por `inherit_env` ou `AZDO_TOKEN_FILE`).

| Variável | Padrão | O que faz |
| --- | --- | --- |
| `AZDO_ORGANIZATION` | obrigatória | Nome da organização, ex.: `minha-org` (só o nome, sem URL) |
| `AZDO_PROJECTS` | vazio | Projetos separados por vírgula, sem diferenciar maiúsculas de minúsculas. Vazio = todos os projetos da organização |
| `AZDO_TOKEN_ENV` | | Nome da variável de ambiente que contém o PAT |
| `AZDO_TOKEN_FILE` | | Caminho de um arquivo com o PAT (alternativa a `AZDO_TOKEN_ENV`) |
| `AZDO_TITLE` | `Azure DevOps` | Título do widget |
| `AZDO_STATE_DIR` | `~/.local/state/mosaic/azure-devops` | Pasta do estado de cada widget (memória de revisões e cache offline) |
| `AZDO_BASE_URL` | `https://dev.azure.com` | Só para testes: `http://127.0.0.1` ou `localhost` (ou https) no lugar do Azure DevOps |

## Testar

Com a sua conta, exporte o PAT e rode o provider com `mosaic check` (o `--cwd` é a pasta onde o Mosaic instalou os plugins):

```sh
export AZDO_TOKEN="seu-pat"
mosaic check --cwd ~/.config/mosaic \
  --env AZDO_ORGANIZATION=minha-org \
  --env AZDO_TOKEN_ENV=AZDO_TOKEN \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-devops/azure_devops.py
```

Sem rede e sem conta, use o servidor falso dos testes. Ele imprime a porta na primeira linha e aceita o token definido em `FAKE_PR_TOKEN`. Rode os comandos a partir da raiz do repositório:

```sh
ARQUIVO_PORTA=$(mktemp)
FAKE_PR_TOKEN=segredo python3 plugins/tests/fake_pull_request_server.py > "$ARQUIVO_PORTA" &
PID_SERVIDOR=$!
until [ -s "$ARQUIVO_PORTA" ]; do sleep 0.1; done
PORTA=$(head -1 "$ARQUIVO_PORTA")
export FAKE_TOKEN=segredo
mosaic check \
  --env AZDO_BASE_URL=http://127.0.0.1:$PORTA \
  --env AZDO_ORGANIZATION=org \
  --env AZDO_TOKEN_ENV=FAKE_TOKEN \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-devops/azure_devops.py
kill "$PID_SERVIDOR"
```

## Estado

Cada widget guarda o seu estado (memória das revisões e cache offline) num arquivo próprio em `AZDO_STATE_DIR`. O nome do arquivo é um hash da URL base, da organização, de `AZDO_PROJECTS`, da origem do token (o nome da variável ou o caminho do arquivo) e de `AZDO_TITLE`. O token nunca entra na chave nem no arquivo. Trocar o título, a organização, os projetos ou a origem do token cria outro arquivo: a memória do `push novo` e o cache offline começam vazios.

## Limitações

- Só Azure DevOps Services na nuvem (`dev.azure.com`). Azure DevOps Server (on-premises) não é suportado nesta entrega.
- Só leitura: o widget não declara ações.
- Só PAT. Ele vence e não se renova sozinho; quando vencer, o widget mostra o erro até você trocar o token.
- Cada seção lê no máximo 200 PRs.
- Revisão pedida a um time não aparece: o PR só entra em **Aguardando minha revisão** quando você vota nele ou é adicionado diretamente como revisor. Mapear um time para os seus membros exige as APIs Graph e Identities, que ainda não são usadas.
- O `push novo` depende da memória local: ele some só quando a sua revisão muda de valor ou o PR sai da lista. Se você revisar de novo depois de um push sem mudar o seu voto, a tag `push novo` continua aparecendo até lá.

## Quando a API não responde

Se a rede falhar, a API responder com erro inesperado ou o limite de requisições do Azure DevOps estourar, o widget mostra o último resultado bom com o aviso `dados de HH:MM (offline)` (`dados de DD/MM HH:MM (offline)` quando o resultado não é de hoje). Sem nenhum resultado guardado, mostra o erro. Erros permanentes **não** usam o cache e aparecem como `erro: ...`: token inválido ou sem permissão para Azure DevOps, requisição recusada (HTTP 400, 404 ou 410; um 404 no primeiro acesso indica `AZDO_ORGANIZATION` errada). Nesses casos, confira a configuração do widget e a versão do plugin.

## Roteiro de teste manual

Com a sua conta, em um projeto de teste:

1. Peça a sua revisão em um PR: aparece `novo` em **Aguardando minha revisão**.
2. Peça mudança (vote para esperar pelo autor) e faça o autor enviar um push: aparece `push novo`.
3. Aprove o PR: ele some da lista.
4. Abra um PR seu: aparece `aguardando` em **Meus PRs**.
