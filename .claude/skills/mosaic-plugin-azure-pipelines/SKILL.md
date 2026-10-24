---
name: mosaic-plugin-azure-pipelines
description: Use quando for configurar, depurar ou alterar o widget de pipelines lentas do Azure DevOps (plugins/azure-pipelines/azure_pipelines.py), ou ao mexer em AZDO_PIPELINE_DAYS, AZDO_PIPELINE_TOP, AZDO_PIPELINE_SAMPLE, no ranking de duração média, nos passos que mais demoram ou no cache de timelines.
---

# Azure Pipelines (pipelines lentas)

## Visão geral

`plugins/azure-pipelines/azure_pipelines.py` mostra, na Home, as pipelines de build mais lentas do **Azure DevOps Services** (média de duração das execuções concluídas na janela) e, no detalhe (Enter no widget), os passos que mais demoram em cada uma. Só leitura (`GET`, `api-version` 7.1), sem ações. A autenticação é Microsoft Entra ID, igual à do widget de PRs (`mosaic-plugin-azure-devops`). Detalhes e roteiro manual: `plugins/azure-pipelines/README.md`. Requisitos gerais: `mosaic-widget-authoring`.

> Instalada pelo pacote (`mosaic skills install`), os caminhos `plugins/...` são os de `~/.config/mosaic/`. A pasta `plugins/tests/` (servidores falsos e testes) e a seção "Mexendo no código" só valem com o repositório clonado.

## Configurar com a pessoa

1. **Organização**: só o nome (`minha-org`), nunca a URL → `AZDO_ORGANIZATION` (obrigatória).
2. **Projetos**: quais? → `AZDO_PROJECTS`, separados por vírgula. Vazio lista todos os projetos da organização (até 50), o que custa uma requisição de builds por projeto: prefira listar.
3. **Autenticação**: igual ao widget de PRs. Pergunte o método (`azure-cli` é o padrão) e siga a entrevista de `mosaic-plugin-azure-devops` → `AZDO_AUTH_METHOD`, `AZDO_TENANT_ID`, `AZDO_CLIENT_ID`, `AZDO_CLIENT_SECRET_ENV`. **Nunca peça o valor do segredo no chat.** A identidade precisa de acesso **Basic** e permissão de leitura de **Build** nos projetos.
4. **Janela e tamanho**: quantos dias de histórico (`AZDO_PIPELINE_DAYS`, 1 a 90, padrão 14), quantas pipelines no ranking (`AZDO_PIPELINE_TOP`, 1 a 10, padrão 5) e quantas execuções recentes de cada uma entram na análise dos passos (`AZDO_PIPELINE_SAMPLE`, 1 a 20, padrão 10).
5. Duas organizações são dois widgets, cada um com `id` e `[widgets.env]` próprios. Azure DevOps Server (on-premises) não é suportado.

## Bloco do mosaic.toml

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

O intervalo é longo de propósito: o ranking muda devagar e cada leitura faz uma requisição de builds por projeto. Para o `service-principal`, acrescente `AZDO_TENANT_ID`, `AZDO_CLIENT_ID`, `AZDO_CLIENT_SECRET_ENV` e o nome da variável em `inherit_env`.

## Verificar

```sh
az login
mosaic check --cwd ~/.config/mosaic \
  --env AZDO_ORGANIZATION=minha-org --env AZDO_PROJECTS=meu-projeto \
  --env AZDO_AUTH_METHOD=azure-cli \
  --env AZDO_STATE_DIR="$(mktemp -d)" \
  python3 plugins/azure-pipelines/azure_pipelines.py
```

A prévia mostra o título "Pipelines lentas", as pipelines mais lentas com a duração média à direita e, nas etiquetas, o número de execuções e a taxa de falhas, mais a linha `mini: <duração> pipeline mais lenta`. Os passos por pipeline só aparecem no detalhe do widget (Enter); o `check` mostra só a Home. Sem conta: `FAKE_PR_TOKEN=token-falso FAKE_ENTRA_SECRET=segredo-falso python3 plugins/tests/fake_pipeline_server.py` e `python3 plugins/tests/fake_entra_server.py` (a porta vem na primeira linha de cada um), depois `AZDO_BASE_URL`/`AZDO_LOGIN_URL` com `service-principal` (roteiro completo no README). `check` não aplica `inherit_env`.

## Mexendo no código

- Testes: `python3 -m unittest discover -s plugins/tests -p test_azure_pipelines.py` e `-p test_entra_auth.py`. Fixtures e servidor falso: `plugins/tests/pipeline_fixtures.py`, `fake_pipeline_server.py`.
- O bloco entre `# --- entra-auth: início ---` e `# --- entra-auth: fim ---` é **idêntico** ao do `azure_devops.py`; `test_entra_auth_blocks_are_identical` confere. Mude os dois arquivos juntos. O bloco depende de nomes definidos fora dele (`ConfigError`, `AuthError`, `read_base_url`, `LOGIN_INSTRUCTION` etc.); o teste `test_every_module_defines_the_names_the_block_expects` lista todos.
- Fluxo: `fetch_projects` → `fetch_runs` por projeto (`statusFilter=completed`, `minTime`, até 1000 execuções) → `summarize` (ranking por média de `finishTime - startTime`; canceladas fora; fila = `startTime - queueTime`, negativo vira 0) → para cada pipeline do ranking, `select_sample` (as execuções mais recentes) → `parse_timeline` de cada uma → `aggregate_steps`.
- Passo = registro `Task` da timeline, nomeado `Job › Tarefa`. Registro sem data, com duração negativa ou com ciclo de `parentId` é pulado. A espera na fila entra na tabela como o passo `espera na fila`, mas **não** conta como gargalo. `% do total` = média do passo ÷ (média da execução + média da fila).
- Cache de timelines em `AZDO_STATE_DIR` (chave `timelines`): um build concluído não muda, então a timeline dele nunca é pedida de novo. No máximo `MAXIMUM_TIMELINE_REQUESTS_PER_RUN` (30) timelines novas por leitura, o cache guarda no máximo `MAXIMUM_CACHED_TIMELINES` (300) builds (os de maior id) e a pipeline com amostra incompleta mostra `(parcial)` na dica. Na primeira leitura a amostra é parcial até o cache esquentar.
- Erro de uma timeline (404, resposta inesperada) pula só aquele build; autenticação, rede e limite de requisições interrompem a leitura (rede e limite servem o cache com `dados de HH:MM (offline)`; autenticação e requisição recusada nunca).
- Arquivo de estado = hash de URL base, organização, projetos, janela, ranking, amostra, método, tenant, client id e `AZDO_TITLE`; nenhum segredo entra.
- Limites do protocolo: título das linhas, do cabeçalho e dos passos em 80 caracteres, `mini.value` em 8 (acima disso mostra `999h+`). A Home leva só o ranking (número de execuções e taxa de falhas) para o nome da pipeline caber; o passo mais lento fica no detalhe.

## Armadilhas

| Sintoma | Causa | Correção |
| --- | --- | --- |
| "nenhuma execução concluída nos últimos N dias" | Janela curta, projeto errado ou pipeline só com execuções canceladas | Aumentar `AZDO_PIPELINE_DAYS`, conferir `AZDO_PROJECTS` |
| Dica "(parcial)" ou pipeline sem tabela de passos | Cache frio: o teto de 30 timelines por leitura ainda não cobriu a amostra | Esperar as próximas leituras; reduzir `AZDO_PIPELINE_SAMPLE` ou `AZDO_PIPELINE_TOP` |
| "projeto ou organização não encontrado" | `AZDO_PROJECTS` com grafia errada (aqui a comparação é exata) ou `AZDO_ORGANIZATION` errada | Conferir o nome exato do projeto |
| 401 ou "credencial inválida" nas builds | Identidade sem permissão de leitura de **Build** ou fora da organização | Dar a permissão; ver as armadilhas de `mosaic-plugin-azure-devops` |
| Projeto com espaço no nome falha | Nome sem escape na URL | O plugin já escapa; confira só o nome |
| Pipelines de release clássicas não aparecem | O widget lê só pipelines de build | Fora do escopo |
| Widget estoura o tempo | Muitos projetos listados (`AZDO_PROJECTS` vazio) | Listar os projetos; subir `timeout_seconds` |
