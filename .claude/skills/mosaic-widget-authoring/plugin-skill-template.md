# Template da skill de um plugin

Copie para `plugins/<pasta>/SKILL.md` e preencha, e crie o link `.claude/skills/mosaic-plugin-<pasta>` → `../../plugins/<pasta>`. O nome da skill (`name`) é `mosaic-plugin-` + o nome da pasta em `plugins/`. O teste `plugins/tests/test_skills.py` confere:

- `name` igual ao nome da pasta da skill; `description` começa com `Use quando` e tem até 1024 caracteres;
- as seções obrigatórias abaixo existem, nesta ordem;
- toda opção do `setup.toml` do plugin aparece, entre crases, na skill;
- todo identificador em MAIÚSCULAS com `_` entre crases (`` `XYZ_TOKEN_ENV` ``) existe no código ou no README do plugin.

A skill é específica do plugin: não repete o contrato do protocolo (isso fica em `mosaic-widget-authoring`) e não copia o README inteiro (aponta para ele). O que ela guarda é o que o README não diz: a entrevista de configuração, as armadilhas e as invariantes do código.

````markdown
---
name: mosaic-plugin-<pasta>
description: Use quando for configurar, depurar ou alterar o widget de <serviço> (plugins/<pasta>/<arquivo>.py), ou ao mexer em <termos que a pessoa usaria: variáveis, opções, erros>.
---

# <Serviço>

## Visão geral
Uma ou duas frases: o que o widget mostra, de onde vem, se é só leitura. Link para `plugins/<pasta>/README.md`.

## Instalar e configurar com o mosaic plugins
Os passos de terminal, na ordem: `mosaic plugins list`, `mosaic plugins install <pasta>` (copia o plugin, a skill e o `setup.toml` e liga a skill ao Claude Code; `--force`, `--no-skill`, `--from`, `--to`, `--skills-dir`), `mosaic plugins setup <pasta>` (`--print`, `--advanced`, `--config-dir`) e `R` no mosaic. Diga que o agente não roda o `setup` (é interativo) e que dentro do Claude Code a pessoa usa `! mosaic plugins setup <pasta>`.

## Configurar com a pessoa
Perguntas na ordem, uma por vez, e o que cada resposta vira no `mosaic.toml`.
Regras de segredo: peça só o **nome** da variável ou o caminho do arquivo; nunca peça o valor no chat, nunca o grave em arquivo do repositório nem o passe em `--env` ou em argumento.

## Bloco do `mosaic.toml`
O `[[widgets]]` completo, com `id`, `command`, `args`, `interval_seconds`, `timeout_seconds`, `inherit_env` e `[widgets.env]`.

## Todas as opções
Tabela com cada variável e opção do `setup.toml`: o que é, padrão, e quando o `setup` pergunta (obrigatória, condição, só com `--advanced`). Inclui as variáveis só de teste e os modelos prontos.

## Verificar
O `mosaic check` para a conta real e o roteiro sem rede (servidor falso). Diga o que a pessoa deve ver.

## Mexendo no código
Invariantes que um teste protege, tabela de erros (permanente ou transitório), o que entra na chave do estado, limites da API, onde ficam os testes e como rodá-los.

## Armadilhas
Tabela "sintoma → causa → correção" com o que já deu problema neste plugin.
````

Seções obrigatórias (títulos exatos, nesta ordem): `Visão geral`, `Instalar e configurar com o mosaic plugins`, `Configurar com a pessoa`, `Bloco do mosaic.toml`, `Todas as opções`, `Verificar`, `Mexendo no código`, `Armadilhas`.

## O `setup.toml`

O roteiro do `mosaic plugins setup`, ao lado do `README.md`. Só o que o plugin já lê:

```toml
plugin = "<pasta>"
script = "plugins/<pasta>/<nome>.py"
default_id = "<id sugerido>"
interval_seconds = 120
timeout_seconds = 30
testing_only = ["<VARIAVEL_SO_DE_TESTE>"]   # no README, mas o setup não pergunta

[[steps]]                  # na ordem em que o setup pergunta
kind = "variable"          # variable (um [widgets.env]), argument (uma opção de linha de comando) ou secret
name = "<VARIAVEL>"        # argument usa flag = "--opcao"; secret usa environment_key e file_key
prompt = "..."
type = "text"              # text, list, integer (min, max) ou choice (choices = [{ value, description }])
default = "..."            # o padrão do README; Enter mantém e não grava
required = true            # ou required_when = { variable = "X", in = ["a"] }
advanced = true            # só com --advanced
when = { variable = "X", in = ["a"] }   # só pergunta se uma resposta anterior bate

[[inherit]]                # variável de ambiente que o widget precisa enxergar
name = "DBUS_SESSION_BUS_ADDRESS"
when = { variable = "X", in = ["a"] }

[[notes]]                  # aviso mostrado depois do bloco
text = "..."

[[presets]]                # modelos prontos do README; {entrada} vem de inputs
name = "..."
description = "..."
id = "id-sugerido"
arguments = { "--opcao" = "valor" }
inputs = [{ name = "entrada", prompt = "...", quote_list = false }]
```

Segredo: `kind = "secret"` pergunta só o **nome** da variável (`environment_key`) ou o caminho do arquivo (`file_key`); o nome entra em `inherit_env`. Uma condição só pode citar uma pergunta anterior.
