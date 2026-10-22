# GitHub

`plugins/github/github_prs.py` é um provider do Mosaic que mostra os pull requests que esperam por você no GitHub: os que pediram sua revisão e os que você abriu. Ele só lê (GraphQL, sem mutation) e só fala com `https://api.github.com`. O token nunca vai em argumento: `GITHUB_TOKEN_ENV` guarda o **nome** da variável que contém o token (ou use `GITHUB_TOKEN_FILE`).

## O que mostra

O widget tem duas seções, sempre com estes nomes:

- **Aguardando minha revisão**: PRs em que você foi pedido como revisor, ou que você já revisou e continuam abertos. PRs já aprovados por você saem da lista.
- **Meus PRs**: PRs que você abriu.

Cada item leva uma tag:

| Seção | Tag | Quando aparece |
| --- | --- | --- |
| Aguardando minha revisão | `novo` | Você ainda não revisou o PR |
| Aguardando minha revisão | `push novo` | O autor fez push depois da sua última revisão (o item vem com a flag `updated`) |
| Aguardando minha revisão | nenhuma | Você já revisou e não houve push depois |
| Meus PRs | `conflito` | O PR tem conflito com a base |
| Meus PRs | `mudança pedida` | Um revisor pediu mudanças |
| Meus PRs | `aprovado` | O GitHub marcou o PR como aprovado |
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

Adicione ao `config.toml` (o bloco também vem comentado no arquivo instalado pelo Mosaic):

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

Sem rede e sem conta, use o servidor falso dos testes. Ele imprime a porta na primeira linha e aceita o token definido em `FAKE_PR_TOKEN`:

```sh
FAKE_PR_TOKEN=segredo python3 plugins/tests/fake_pull_request_server.py > /tmp/porta.txt &
PORTA=$(head -1 /tmp/porta.txt)
export FAKE_TOKEN=segredo
mosaic check --cwd ~/.config/mosaic \
  --env GITHUB_API_URL=http://127.0.0.1:$PORTA \
  --env GITHUB_TOKEN_ENV=FAKE_TOKEN \
  --env GITHUB_STATE_DIR="$(mktemp -d)" \
  python3 plugins/github/github_prs.py
kill %1
```

## Limitações

- Só GitHub.com (`api.github.com`). GitHub Enterprise não é suportado nesta entrega.
- Só leitura: o widget não declara ações.
- Só token pessoal (PAT). Ele vence e não se renova sozinho; quando vencer, o widget mostra o erro até você trocar o token.
- Cada busca lê no máximo 100 PRs (`first: 100`), sem paginação.
- Só entram os PRs dos repositórios que o token alcança.

## Quando a API não responde

Se a rede falhar, a API responder com erro inesperado ou o limite de requisições do GitHub estourar, o widget mostra o último resultado bom com o aviso `dados de HH:MM (offline)`. Sem nenhum resultado guardado, mostra o erro. Token inválido ou sem permissão **não** usa o cache: o widget mostra `erro: token inválido ou sem permissão para GitHub`.

## Roteiro de teste manual

Com a sua conta, em um repositório de teste:

1. Peça a sua revisão em um PR: aparece `novo` em **Aguardando minha revisão**.
2. Peça mudança e faça o autor enviar um push: aparece `push novo`.
3. Aprove o PR: ele some da lista.
4. Abra um PR seu: aparece `aguardando` em **Meus PRs**.
