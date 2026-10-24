# GitLab

> Instalação: `mosaic plugins install gitlab` (o plugin não vem no binário; veja o README da raiz).

`plugins/gitlab/gitlab_mrs.py` é um provider do Mosaic que mostra os merge requests do GitLab que esperam por você: os que pediram sua revisão e os que você abriu. Ele só lê (consultas GraphQL, sem mutation) e só fala com `https://gitlab.com`. O token nunca vai em argumento: `GITLAB_TOKEN_ENV` guarda o **nome** da variável que contém o token (ou use `GITLAB_TOKEN_FILE`).

## O que mostra

O widget tem duas seções, sempre com estes nomes:

- **Aguardando minha revisão**: merge requests abertos em que você foi pedido como revisor e que não foram criados por você. Os que você aprovou saem da lista.
- **Meus PRs**: merge requests abertos que você abriu.

Cada item leva uma tag:

| Seção | Tag | Quando aparece |
| --- | --- | --- |
| Aguardando minha revisão | `novo` | Você ainda não revisou o MR |
| Aguardando minha revisão | `push novo` | O autor fez push depois da sua última revisão (o item vem com a flag `updated`) |
| Aguardando minha revisão | nenhuma | Você já revisou e não houve push depois |
| Meus PRs | `conflito` | O MR tem conflito com a base |
| Meus PRs | `mudança pedida` | Um revisor pediu mudanças |
| Meus PRs | `aprovado` | Um revisor designado aprovou |
| Meus PRs | `aguardando` | Nenhuma das situações acima |

Drafts nunca aparecem. Em cada seção, os MRs vão do mais antigo para o mais novo. Os MRs sem data de criação são pulados, e autor apagado aparece como `desconhecido`.

## Token

Crie um PAT em `https://gitlab.com/-/user_settings/personal_access_tokens`. O escopo mínimo é **read_api**.

Guarde o token numa variável de ambiente (`GITLAB_TOKEN_MRS`, por exemplo) ou num arquivo. Se usar arquivo, deixe-o só para o seu usuário:

```sh
chmod 600 ~/.config/mosaic/gitlab-token
```

O PAT vence e precisa ser recriado à mão. Veja as limitações.

## Configurar

Adicione ao `mosaic.toml` (o bloco também vem comentado no arquivo instalado pelo Mosaic):

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

Para dois GitLabs ou duas contas, são dois widgets, cada um com `id` próprio e o seu `[widgets.env]`.

### Variáveis de ambiente

Vão em `[widgets.env]` (o token, por `inherit_env` ou `GITLAB_TOKEN_FILE`).

| Variável | Padrão | O que faz |
| --- | --- | --- |
| `GITLAB_TOKEN_ENV` | | Nome da variável de ambiente que contém o PAT |
| `GITLAB_TOKEN_FILE` | | Caminho de um arquivo com o PAT (alternativa a `GITLAB_TOKEN_ENV`) |
| `GITLAB_TITLE` | `GitLab` | Título do widget |
| `GITLAB_STATE_DIR` | `~/.local/state/mosaic/gitlab` | Pasta do estado de cada widget (memória de revisões e cache offline) |
| `GITLAB_URL` | `https://gitlab.com` | Só para testes: `http://127.0.0.1` ou `localhost` (ou https) no lugar do GitLab padrão |

## Testar

Com a sua conta, exporte o PAT e rode o provider com `mosaic check` (o `--cwd` é a pasta onde o Mosaic instalou os plugins):

```sh
export GITLAB_TOKEN_MRS="seu-pat"
mosaic check --cwd ~/.config/mosaic \
  --env GITLAB_TOKEN_ENV=GITLAB_TOKEN_MRS \
  --env GITLAB_STATE_DIR="$(mktemp -d)" \
  python3 plugins/gitlab/gitlab_mrs.py
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
  --env GITLAB_URL=http://127.0.0.1:$PORTA \
  --env GITLAB_TOKEN_ENV=FAKE_TOKEN \
  --env GITLAB_STATE_DIR="$(mktemp -d)" \
  python3 plugins/gitlab/gitlab_mrs.py
kill "$PID_SERVIDOR"
```

## Estado

Cada widget guarda o seu estado (memória das revisões e cache offline) num arquivo próprio em `GITLAB_STATE_DIR`. O nome do arquivo é um hash da URL base, da origem do token (o nome da variável ou o caminho do arquivo) e de `GITLAB_TITLE`. O token nunca entra na chave nem no arquivo. Trocar o título ou a origem do token cria outro arquivo: a memória do `push novo` e o cache offline começam vazios.

## Limitações

- Só GitLab.com (`gitlab.com`). GitLab self-hosted não é testado nesta entrega.
- Só leitura: o widget não declara ações.
- Só PAT. Ele vence e não se renova sozinho; quando vencer, o widget mostra o erro até você trocar o token.
- Cada lista lê no máximo 100 MRs (`first: 100`), sem paginação.
- Em **Meus PRs**, `aprovado` vem só da aprovação de revisores designados. Em um projeto sem regra de aprovação, um MR sem nenhuma revisão não ganha `aprovado` só por estar pronto para o merge.
- Se você pedir a sua própria revisão em um MR seu, ele fica só em **Meus PRs**.
- O `push novo` depende da memória local: ele some só quando a sua revisão muda de valor ou o MR sai da lista. Se você revisar de novo depois de um push sem mudar o seu estado de revisão, a tag `push novo` continua aparecendo até lá.

## Quando a API não responde

Se a rede falhar, a API responder com erro inesperado ou o limite de requisições do GitLab estourar, o widget mostra o último resultado bom com o aviso `dados de HH:MM (offline)` (`dados de DD/MM HH:MM (offline)` quando o resultado não é de hoje). Sem nenhum resultado guardado, mostra o erro. Erros permanentes **não** usam o cache e aparecem como `erro: ...`: token inválido ou sem permissão para GitLab, requisição recusada (HTTP 400, 404 ou 410), consulta recusada pelo GitLab. Nesses casos, confira a configuração do widget e a versão do plugin.

## Roteiro de teste manual

Com a sua conta, em um projeto de teste:

1. Peça a sua revisão em um MR: aparece `novo` em **Aguardando minha revisão**.
2. Peça mudança e faça o autor enviar um push: aparece `push novo`.
3. Aprove o MR: ele some da lista.
4. Abra um MR seu: aparece `aguardando` em **Meus PRs**.
