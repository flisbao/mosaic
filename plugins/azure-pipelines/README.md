# Azure Pipelines (pipelines lentas)

> Instalação: `mosaic plugins install azure-pipelines` (o plugin não vem no binário; veja o README da raiz). Configuração guiada, sem editar o `mosaic.toml` à mão: `mosaic plugins setup azure-pipelines` (pergunta só o que este plugin lê, mostra o bloco e grava depois que você confirmar).

`plugins/azure-pipelines/azure_pipelines.py` é um provider do Mosaic que mostra as pipelines de build mais lentas do Azure DevOps e, ao abrir o detalhe do widget (Enter), os passos que mais demoram em cada uma. Ele só faz `GET` na API REST (`api-version` 7.1) e só fala com `https://dev.azure.com` e, para autenticar, com o Microsoft Entra ID. Nenhum segredo vai em argumento.

## O que mostra

Na Home, o widget lista as pipelines com maior **duração média** (de `startTime` a `finishTime`) entre as execuções **concluídas** nos últimos `AZDO_PIPELINE_DAYS` dias, da mais lenta para a mais rápida. Execuções canceladas ficam de fora.

- Um quadro com o número de pipelines, o de execuções e a janela.
- Uma linha por pipeline do ranking (`projeto/pipeline`), com a duração média à direita e etiquetas: número de execuções e taxa de falhas (quando existe). O passo mais lento fica no detalhe (Enter).
- A pipeline mais lenta fica em vermelho e vai para a coluna do modo foco (`mini`).

No **detalhe** (Enter), há uma seção por pipeline do ranking, com a média, o máximo, a fila média e a taxa de falhas, e uma tabela dos passos que mais demoram:

| Coluna | O que é |
| --- | --- |
| Passo | `Job › Tarefa`, tirado da timeline do build; a espera na fila aparece como `espera na fila` |
| Média | Soma da duração do passo em cada execução da amostra, dividida pelo número de execuções da amostra |
| % do total | Média do passo dividida pela média (execução + fila) das mesmas execuções da amostra |
| Máximo | A maior soma do passo numa só execução |

O passo mais lento (sem contar a fila) vai destacado com `⚠`. O quadro do topo (média, máximo, fila e falhas) vale para **todas** as execuções da janela; a tabela, só para a amostra, e a dica diz de quantas execuções vem cada uma e, se alguma timeline ainda não foi lida, `(parcial)`.

## Autenticação

É a mesma do widget de pull requests: Microsoft Entra ID, com `AZDO_AUTH_METHOD` igual a `azure-cli` (padrão), `device-code`, `service-principal` ou `managed-identity`. O passo a passo de cada método, o registro do app no Entra e a migração do PAT estão em [`plugins/azure-devops/README.md`](../azure-devops/README.md). Aqui vale o mesmo: a identidade precisa estar na organização com acesso **Basic** e ter permissão de leitura de **Build** nos projetos. Para o `device-code`, rode o login com este script:

```sh
AZDO_AUTH_METHOD=device-code AZDO_CLIENT_ID=<client-id> AZDO_TENANT_ID=<tenant-id> \
  python3 ~/.config/mosaic/plugins/azure-pipelines/azure_pipelines.py login
```

O `login` serve aos dois widgets quando o tenant e o client id são os mesmos.

## Configurar

Adicione ao `mosaic.toml` (o bloco também vem comentado no arquivo instalado pelo Mosaic):

```toml
[[widgets]]
id = "azure-pipelines"
command = "python3"
args = ["plugins/azure-pipelines/azure_pipelines.py"]
interval_seconds = 900
timeout_seconds = 60
inherit_env = []

[widgets.env]
AZDO_ORGANIZATION = "minha-org"
AZDO_PROJECTS = "meu-projeto"
AZDO_AUTH_METHOD = "azure-cli"
```

O intervalo é de 15 minutos porque o ranking muda devagar e cada leitura faz uma requisição de builds por projeto. Para duas organizações, são dois widgets, cada um com `id` próprio e o seu `[widgets.env]`.

### Catálogo de pipelines (escolher quais monitorar)

Sem catálogo, o widget varre os projetos (`AZDO_PROJECTS`, ou todos) e mostra o ranking de tudo. Para monitorar só algumas pipelines, guarde as URLs delas num arquivo à parte e aponte para ele com `AZDO_PIPELINES_FILE`. O formato é INI (só biblioteca padrão do Python, igual no Linux e no macOS); cada seção é uma pipeline e o nome da seção é o nome que você usa para escolhê-la:

```ini
# pipelines.ini (ao lado do mosaic.toml)
[build-api]
repository = api
url = https://dev.azure.com/minha-org/Plataforma/_build?definitionId=42

[deploy-api]
repository = api
url = https://dev.azure.com/minha-org/Plataforma/_build/latest?definitionId=43

[lint-site]
repository = site
url = https://dev.azure.com/minha-org/Marketing/_build?definitionId=7
```

- `url` (obrigatória): o endereço da pipeline no Azure DevOps, com `definitionId=NÚMERO` (abra a pipeline no navegador e copie). O projeto e o número vêm da própria URL; a organização precisa ser a de `AZDO_ORGANIZATION`.
- `repository` (opcional): rótulo do repositório, só para agrupar e escolher.
- O nome da seção não pode ter `/`.

No widget, `AZDO_PIPELINES_FILE = "pipelines.ini"` (relativo à pasta do `mosaic.toml`) e, para escolher, `AZDO_PIPELINE_WATCH` com uma lista separada por vírgula de:

| Escolha | Seleciona |
| --- | --- |
| `build-api` | a pipeline com esse nome |
| `Plataforma/api` | todas as pipelines do projeto `Plataforma` com `repository = api` |
| `Plataforma/*` | todas as pipelines do projeto `Plataforma` |

Vazio, `AZDO_PIPELINE_WATCH` seleciona o catálogo inteiro. Dois widgets com `AZDO_PIPELINE_WATCH` diferentes dão duas seleções do mesmo arquivo. O widget só pede à API as pipelines escolhidas e o ranking vale só para elas. Um nome que não existe no catálogo é erro, e a mensagem lista os nomes e grupos válidos. Com o catálogo, `AZDO_PROJECTS` não pode estar definido (os projetos vêm das URLs). Para conferir o que o catálogo e a escolha selecionam, sem rede:

```sh
AZDO_ORGANIZATION=minha-org AZDO_PIPELINES_FILE=~/.config/mosaic/pipelines.ini \
  python3 ~/.config/mosaic/plugins/azure-pipelines/azure_pipelines.py pipelines
```

O catálogo é lido a cada atualização: edite o arquivo e aperte `r` no mosaic.

### Variáveis de ambiente

Vão em `[widgets.env]` (o segredo, por `inherit_env` ou `AZDO_CLIENT_SECRET_FILE`).

| Variável | Padrão | O que faz |
| --- | --- | --- |
| `AZDO_ORGANIZATION` | obrigatória | Nome da organização, ex.: `minha-org` (só o nome, sem URL) |
| `AZDO_PROJECTS` | vazio | Projetos separados por vírgula (nome exato). Vazio = todos os projetos da organização, até 50. Não combina com `AZDO_PIPELINES_FILE` |
| `AZDO_PIPELINES_FILE` | | Caminho do catálogo de pipelines (INI, veja acima). Com ele, só as pipelines do catálogo entram no ranking |
| `AZDO_PIPELINE_WATCH` | vazio | Quais pipelines do catálogo monitorar: nomes, `projeto/repositório` ou `projeto/*`, separados por vírgula. Vazio = todas. Só vale com `AZDO_PIPELINES_FILE` |
| `AZDO_PIPELINE_DAYS` | `14` | Janela de histórico, de 1 a 90 dias |
| `AZDO_PIPELINE_TOP` | `5` | Quantas pipelines entram no ranking, de 1 a 10 |
| `AZDO_PIPELINE_SAMPLE` | `10` | Quantas execuções recentes de cada pipeline entram na análise dos passos, de 1 a 20 |
| `AZDO_AUTH_METHOD` | `azure-cli` | `azure-cli`, `device-code`, `service-principal` ou `managed-identity` |
| `AZDO_TENANT_ID` | `organizations` | Directory (tenant) ID; obrigatório no `service-principal` |
| `AZDO_CLIENT_ID` | | Application (client) ID; obrigatório no `device-code` e no `service-principal` |
| `AZDO_CLIENT_SECRET_ENV` | | Nome da variável que contém o segredo do app (`service-principal`) |
| `AZDO_CLIENT_SECRET_FILE` | | Caminho de um arquivo com o segredo (alternativa a `AZDO_CLIENT_SECRET_ENV`) |
| `AZDO_CREDENTIALS_DIR` | `~/.local/state/mosaic/azure-devops-credentials` | Pasta (`0700`) do refresh token do `device-code` |
| `AZDO_TITLE` | `Pipelines lentas` | Título do widget |
| `AZDO_STATE_DIR` | `~/.local/state/mosaic/azure-pipelines` | Pasta do estado do widget (cache offline e cache de timelines) |
| `AZDO_BASE_URL` | `https://dev.azure.com` | Só para testes: `http://127.0.0.1` ou `localhost` (ou https) no lugar do Azure DevOps |
| `AZDO_LOGIN_URL` | `https://login.microsoftonline.com` | Só para testes: o servidor do Entra |
| `AZDO_IMDS_URL` | `http://169.254.169.254` | Só para testes: o IMDS |

## Testar

Com a sua conta, faça `az login` e rode o provider com `mosaic check` (o `--cwd` é a pasta onde o Mosaic instalou os plugins). O `check` mostra a Home; os passos aparecem no detalhe do widget no Mosaic.

```sh
az login
mosaic check --cwd ~/.config/mosaic \
  --env AZDO_ORGANIZATION=minha-org \
  --env AZDO_PROJECTS=meu-projeto \
  --env AZDO_AUTH_METHOD=azure-cli \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-pipelines/azure_pipelines.py
```

Sem rede e sem conta, use os servidores falsos dos testes (Azure DevOps e Entra). Cada um imprime a porta na primeira linha. Rode os comandos a partir da raiz do repositório:

```sh
export FAKE_PR_TOKEN=token-falso FAKE_ENTRA_SECRET=segredo-falso
ARQUIVO_AZURE=$(mktemp); ARQUIVO_ENTRA=$(mktemp)
python3 plugins/tests/fake_pipeline_server.py > "$ARQUIVO_AZURE" &
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
  --env AZDO_PROJECTS=Alpha,Beta \
  --env AZDO_AUTH_METHOD=service-principal \
  --env AZDO_TENANT_ID=tenant-1 \
  --env AZDO_CLIENT_ID=client-1 \
  --env AZDO_CLIENT_SECRET_ENV=SEGREDO_FALSO \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-pipelines/azure_pipelines.py
kill "$PID_AZURE" "$PID_ENTRA"
```

## Estado e cache

O widget guarda, em `AZDO_STATE_DIR`, o último resultado bom (cache offline) e as **timelines** dos builds já lidos. Um build concluído não muda, então a timeline dele é pedida uma vez só. Cada leitura pede no máximo 30 timelines novas e para de pedir após 40 segundos (para terminar e gravar o cache antes do `timeout_seconds`); o cache guarda até 300 builds (os de maior id). Um build sem timeline (404 ou sem tarefas) é lembrado como indisponível e não conta na amostra. Na primeira leitura, uma pipeline com muitas execuções pode mostrar a amostra `(parcial)` até o cache esquentar nas leituras seguintes. O nome do arquivo é um hash da URL base, da organização, dos projetos, da janela, do ranking, da amostra, do método de autenticação, do tenant, do client id e de `AZDO_TITLE`. Nenhum token nem segredo entra na chave nem no arquivo.

## Limitações

- Só Azure DevOps Services na nuvem (`dev.azure.com`). Azure DevOps Server (on-premises) não é suportado.
- Só leitura: o widget não declara ações.
- Lê até 1000 execuções concluídas por projeto na janela. Pipelines de release clássicas (Releases) não entram: só pipelines de build.
- A análise dos passos usa a amostra das `AZDO_PIPELINE_SAMPLE` execuções mais recentes de cada pipeline, não todas.
- Execuções em paralelo (jobs simultâneos) somam no passo: a soma dos passos pode passar da duração da execução.
- Contas pessoais da Microsoft (MSA) não funcionam com o Entra. Veja o README do Azure DevOps.

## Quando a API não responde

Se a rede falhar, a API responder com erro inesperado ou o limite de requisições estourar, o widget mostra o último resultado bom com o aviso `dados de HH:MM (offline)`. Sem nenhum resultado guardado, mostra o erro. Erros permanentes **não** usam o cache e aparecem como `erro: ...`: credencial inválida ou sem permissão, identidade que não foi adicionada à organização, `az` sem sessão, `login` pendente e requisição recusada (HTTP 400, 404 ou 410; um 404 indica `AZDO_ORGANIZATION` ou `AZDO_PROJECTS` errados). Um erro na timeline de **um** build só pula aquele build; falha de rede ou limite de requisições nas timelines interrompe só os pedidos de timeline, e o que já foi lido é guardado.
