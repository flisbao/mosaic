# Template da skill de um plugin

Copie para `.claude/skills/mosaic-plugin-<pasta>/SKILL.md` e preencha. O nome da pasta da skill é `mosaic-plugin-` + o nome da pasta em `plugins/`. O teste `plugins/tests/test_skills.py` confere:

- `name` igual ao nome da pasta da skill; `description` começa com `Use quando` e tem até 1024 caracteres;
- as seções obrigatórias abaixo existem;
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

## Configurar com a pessoa
Perguntas na ordem, uma por vez, e o que cada resposta vira no `mosaic.toml`.
Regras de segredo: peça só o **nome** da variável ou o caminho do arquivo; nunca peça o valor no chat, nunca o grave em arquivo do repositório nem o passe em `--env` ou em argumento.

## Bloco do `mosaic.toml`
O `[[widgets]]` completo, com `id`, `command`, `args`, `interval_seconds`, `timeout_seconds`, `inherit_env` e `[widgets.env]`.

## Verificar
O `mosaic check` para a conta real e o roteiro sem rede (servidor falso). Diga o que a pessoa deve ver.

## Mexendo no código
Invariantes que um teste protege, tabela de erros (permanente ou transitório), o que entra na chave do estado, limites da API, onde ficam os testes e como rodá-los.

## Armadilhas
Tabela "sintoma → causa → correção" com o que já deu problema neste plugin.
````

Seções obrigatórias (títulos exatos): `Visão geral`, `Configurar com a pessoa`, `Bloco do mosaic.toml`, `Verificar`, `Mexendo no código`, `Armadilhas`.
