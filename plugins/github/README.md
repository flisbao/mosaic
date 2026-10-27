# GitHub

> Instalação: `mosaic plugins install github` (o plugin não vem no binário; veja o README da raiz). Configuração guiada, sem editar o `mosaic.toml` à mão: `mosaic plugins setup github` (pergunta só o que este plugin lê, mostra o bloco e grava depois que você confirmar).

`plugins/github/github_prs.py` é um provider do Mosaic que mostra os pull requests que esperam por você no GitHub: os que pediram sua revisão e os que você abriu. Ele só lê (GraphQL, sem mutation) e só fala com `https://api.github.com`. O token nunca vai em argumento: `GITHUB_TOKEN_ENV` guarda o **nome** da variável que contém o token (ou use `GITHUB_TOKEN_FILE`).

## O que mostra

O widget tem duas seções, sempre com estes nomes:

- **Aguardando minha revisão**: PRs em que pediram a sua revisão, ou em que você pediu mudanças (e continuam abertos). PRs que você aprovou saem da lista, a menos que peçam a sua revisão de novo. PRs que você só comentou não aparecem.
- **Meus PRs**: PRs que você abriu.

Cada item leva uma tag:

| Seção | Tag | Quando aparece |
| --- | --- | --- |
| Aguardando minha revisão | `novo` | Você ainda não revisou o PR, ou aprovou e pediram a sua revisão de novo |
| Aguardando minha revisão | `push novo` | O autor fez push depois da sua última revisão (o item vem com a flag `updated`) |
| Aguardando minha revisão | nenhuma | Você já revisou e não houve push depois |
| Meus PRs | `conflito` | O PR tem conflito com a base |
| Meus PRs | `mudança pedida` | O GitHub marcou o PR como mudança pedida ou, em repositório sem revisão obrigatória, algum revisor (que não seja você) pediu mudanças |
| Meus PRs | `aprovado` | O GitHub marcou o PR como aprovado ou, em repositório sem revisão obrigatória, algum revisor (que não seja você) aprovou e ninguém pediu mudanças |
| Meus PRs | `aguardando` | Nenhuma das situações acima |

Drafts nunca aparecem. Em cada seção, os PRs vão do mais antigo para o mais novo. Os PRs sem data de criação são pulados, e autor apagado (conta "ghost") aparece como `desconhecido`.

## Token

Crie um token em [github.com/settings/tokens](https://github.com/settings/tokens). O escopo mínimo:

- **Fine-grained** (recomendado): permissões `Pull requests: Read` e `Metadata: Read`, com acesso aos repositórios que você quer acompanhar.
- **Classic**: escopo `repo` (o único que alcança repositórios privados).

Guarde o token numa variável de ambiente (`GITHUB_TOKEN_PRS`, por exemplo) ou num arquivo. Se usar arquivo, deixe-o só para o seu usuário:

```sh
chmod 600 ~/.config/mosaic/github-token
```

O token pode vencer (tokens pessoais têm prazo) e precisa ser recriado à mão quando isso acontecer. Veja as limitações.

## Configurar

Adicione ao `mosaic.toml` (o bloco também vem comentado no arquivo instalado pelo Mosaic):

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
```

Para dois GitHubs ou dois conjuntos de repositórios, são dois widgets, cada um com `id` próprio e o seu `[widgets.env]`.

### Variáveis de ambiente

Vão em `[widgets.env]` (o token, por `inherit_env` ou `GITHUB_TOKEN_FILE`).

| Variável | Padrão | O que faz |
| --- | --- | --- |
| `GITHUB_TOKEN_ENV` | | Nome da variável de ambiente que contém o token |
| `GITHUB_TOKEN_FILE` | | Caminho de um arquivo com o token (alternativa a `GITHUB_TOKEN_ENV`) |
| `GITHUB_OWNERS` | vazio | Usuários ou organizações separados por vírgula (ex.: `empresa,minha-conta`). Vazio = todos os repositórios que o token alcança |
| `GITHUB_TITLE` | `GitHub` | Título do widget |
| `GITHUB_STATE_DIR` | `~/.local/state/mosaic/github` | Pasta do estado de cada widget (memória de revisões e cache offline) |
| `GITHUB_API_URL` | `https://api.github.com` | Só para testes: `http://127.0.0.1` ou `localhost` (ou https) no lugar da API padrão |

## Testar

Com a sua conta, exporte o token e rode o provider com `mosaic check` (o `--cwd` é a pasta onde o Mosaic instalou os plugins):

```sh
export GITHUB_TOKEN_PRS="seu-token"
mosaic check --cwd ~/.config/mosaic \
  --env GITHUB_TOKEN_ENV=GITHUB_TOKEN_PRS \
  --env GITHUB_STATE_DIR="$(mktemp -d)" \
  python3 plugins/github/github_prs.py
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
  --env GITHUB_API_URL=http://127.0.0.1:$PORTA \
  --env GITHUB_TOKEN_ENV=FAKE_TOKEN \
  --env GITHUB_STATE_DIR="$(mktemp -d)" \
  python3 plugins/github/github_prs.py
kill "$PID_SERVIDOR"
```

## Estado

Cada widget guarda o seu estado (memória das revisões e cache offline) num arquivo próprio em `GITHUB_STATE_DIR`. O nome do arquivo é um hash da URL da API, de `GITHUB_OWNERS`, da origem do token (o nome da variável ou o caminho do arquivo) e de `GITHUB_TITLE`. O token nunca entra na chave nem no arquivo. Trocar o título, os donos ou a origem do token cria outro arquivo: a memória do `push novo` e o cache offline começam vazios.

## Limitações

- Só GitHub.com (`api.github.com`). GitHub Enterprise não é suportado nesta entrega.
- Só leitura: o widget não declara ações.
- Só token pessoal (PAT). Ele vence e não se renova sozinho; quando vencer, o widget mostra o erro até você trocar o token.
- Cada busca lê no máximo 100 PRs (`first: 100`), sem paginação.
- Só entram os PRs dos repositórios que o token alcança.

## Quando a API não responde

Se a rede falhar, a API responder com erro inesperado ou o limite de requisições do GitHub estourar, o widget mostra o último resultado bom com o aviso `dados de HH:MM (offline)` (`dados de DD/MM HH:MM (offline)` quando o resultado não é de hoje). Sem nenhum resultado guardado, mostra o erro. Erros permanentes **não** usam o cache e aparecem como `erro: ...`: token inválido ou sem permissão para GitHub, requisição recusada (HTTP 400, 404 ou 410), consulta recusada pelo GitHub. Nesses casos, confira a configuração do widget e a versão do plugin.

## Roteiro de teste manual

Com a sua conta, em um repositório de teste:

1. Peça a sua revisão em um PR: aparece `novo` em **Aguardando minha revisão**.
2. Peça mudança e faça o autor enviar um push: aparece `push novo`.
3. Aprove o PR: ele some da lista.
4. Abra um PR seu: aparece `aguardando` em **Meus PRs**.
5. Em um repositório sem revisão obrigatória, peça a outra pessoa para aprovar o seu PR (ou pedir mudanças): a tag vira `aprovado` (ou `mudança pedida`), mesmo sem decisão de revisão do GitHub.
