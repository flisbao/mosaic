---
name: mosaic-plugin-calendar
description: Use quando for configurar, depurar ou alterar o widget de agenda (plugins/calendar/calendar_widget.py), ou ao mexer em CALENDAR_SOURCE, link ICS do Google Agenda, GNOME Online Accounts (GOA), gcalcli ou recorrência (RRULE).
---

# Agenda (Google Agenda)

## Visão geral

`plugins/calendar/calendar_widget.py` mostra a agenda de **agora até a próxima segunda-feira às 00:00** (fuso local); o que já terminou some a cada leitura. Só leitura, sem ações, só biblioteca padrão. Três fontes, uma por widget, escolhidas com `CALENDAR_SOURCE`: `ics`, `goa`, `gcalcli`. Passo a passo e mensagens de erro: `plugins/calendar/README.md`. Requisitos gerais: `mosaic-widget-authoring`.

> Instalada pelo pacote (`mosaic skills install`), os caminhos `plugins/...` são os de `~/.config/mosaic/`. A pasta `plugins/tests/` (servidores falsos e testes) e a seção "Mexendo no código" só valem com o repositório clonado.

## Configurar com a pessoa

Pergunte primeiro **que fonte usar**:

| Situação | Fonte |
| --- | --- |
| Qualquer sistema; conta pessoal ou Workspace com link ICS liberado | `ics` |
| Linux com GNOME e a conta Google já em Contas Online | `goa` |
| Já usa o gcalcli | `gcalcli` |

**ICS**: o link secreto (Google Agenda → Configurações → a agenda → Integrar agenda → endereço secreto em formato iCal) é um **segredo**: quem o tem lê a agenda. Peça só o **nome** da variável (`CALENDAR_ICS_PERSONAL`); a pessoa exporta o link no próprio `~/.profile`. Nunca peça o link no chat, nunca o grave no `mosaic.toml`, em arquivo do repositório, em `--env` ou em argumento. Vários links: `CALENDAR_ICS_URL_ENV = "A,B"` ou um arquivo com um link por linha em `CALENDAR_ICS_URL_FILE`; aumente `timeout_seconds` (até 15 s por link, somados).
**GOA**: a conta precisa estar em Configurações → Contas Online com **Agenda** ligada; `inherit_env` leva `DBUS_SESSION_BUS_ADDRESS`.
**gcalcli**: instalado, client OAuth próprio, app publicado "In production" (em "Testing" o token vence em 7 dias) e `gcalcli init` feito uma vez.
Opcionais: `CALENDAR_TITLE`, `CALENDAR_CALENDARS` (goa e gcalcli), `CALENDAR_GOA_ACCOUNT`.

## Bloco do mosaic.toml

```toml
[[widgets]]
id = "agenda"
command = "python3"
args = ["plugins/calendar/calendar_widget.py"]
interval_seconds = 300
timeout_seconds = 20
inherit_env = ["CALENDAR_ICS_PERSONAL"]

[widgets.env]
CALENDAR_SOURCE = "ics"
CALENDAR_ICS_URL_ENV = "CALENDAR_ICS_PERSONAL"
```

GOA: `CALENDAR_SOURCE = "goa"` e `inherit_env = ["DBUS_SESSION_BUS_ADDRESS"]`. gcalcli: `CALENDAR_SOURCE = "gcalcli"` e, se o `gcalcli` não estiver no `PATH` do widget, `PATH` em `[widgets.env]`.

## Verificar

```sh
mosaic check --cwd ~/.config/mosaic \
  --env CALENDAR_SOURCE=ics --env CALENDAR_ICS_URL_ENV=CALENDAR_ICS_PERSONAL \
  python3 plugins/calendar/calendar_widget.py
```

A pessoa exporta o link no shell **antes**; o link nunca vai em `--env`. `check` passa o ambiente inteiro e **não aplica `inherit_env`**: um check verde não prova que o widget real enxerga `CALENDAR_ICS_PERSONAL` ou `DBUS_SESSION_BUS_ADDRESS`. Confira o `inherit_env` à mão. Para comparar fontes, rode GOA e ICS e compare as listas. O roteiro de aceite (recorrência semanal, dia inteiro, link errado, `gcalcli` fora do `PATH`) está no README.

## Mexendo no código

- Testes: `python3 -m unittest discover -s plugins/tests -p test_calendar_widget.py` (o maior da suíte); sem rede.
- `week_window(now)` define a janela; `visible_events` descarta o que já terminou. O fuso vem de `TZ` ou de `/etc/localtime`.
- Recorrência é implementada à mão (`expand_occurrences`): só `FREQ` DAILY, WEEKLY, MONTHLY e YEARLY com `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY`, `BYMONTHDAY`, `BYMONTH` e `WKST`. Regra fora disso levanta `UnsupportedRule` e vira aviso `repetição não suportada em “título”`, sem derrubar as outras agendas.
- Falha parcial (uma agenda de várias) mostra os eventos lidos mais um aviso; falha total sai com `erro: ...` e o Mosaic mantém o último dado bom. Erros nunca mostram o link.
- ICS: cada link baixa em até 15 s, calendário até 8 MiB, `webcal://` vira `https://`. O ICS não diz quem é você, então convite recusado aparece.
- Títulos de seção do detalhe são os dias (`Hoje`, `Amanhã`, `Qui 23/10`): mudam todo dia e só casam por igualdade em `llm_sections` (item do backlog).
- Eventos de quem usa Workspace: o administrador pode desativar o link ICS; use GOA ou gcalcli.

## Armadilhas

| Sintoma | Causa | Correção |
| --- | --- | --- |
| `a variável 'X' ... está vazia ou ausente` | Variável fora do `inherit_env` | Listar no `inherit_env` |
| `nenhuma agenda ICS respondeu` | Link errado, redefinido ou desativado | Copiar o endereço secreto de novo; ver Workspace |
| `não foi possível consultar o GNOME Online Accounts pelo D-Bus` | Falta `DBUS_SESSION_BUS_ADDRESS` | Adicionar ao `inherit_env` |
| `gcalcli não respondeu; rode gcalcli init` | Sem autenticação, ou travou por mais de 15 s | `gcalcli init` no terminal |
| Widget do gcalcli para após uma semana | App OAuth em "Testing" | Publicar como "In production" |
| Link vazado | Foi colado em chat, `--env` ou arquivo versionado | Redefinir o endereço secreto no Google Agenda e atualizar a variável |
