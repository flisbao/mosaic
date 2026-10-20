# Widget de agenda (Google Agenda)

Plugin do mosaic que mostra a sua agenda da semana: de **agora** até a **próxima segunda-feira às 00:00** (no fuso local, então domingo à noite ainda mostra os eventos de domingo). Os eventos que já terminaram somem sozinhos a cada atualização.

Há três fontes de dados, escolhidas com `CALENDAR_SOURCE`:

| Fonte | Quando usar | Requisitos |
| --- | --- | --- |
| `ics` | Caso geral, qualquer sistema | Link ICS secreto da agenda |
| `goa` | Linux com GNOME, sem guardar link nenhum | GNOME Online Accounts com a conta Google |
| `gcalcli` | Quem já usa o gcalcli | gcalcli instalado e autenticado |

O plugin usa só a biblioteca padrão do Python 3; não há `pip install`.

## Instalação

Se o mosaic instalou a configuração padrão agora (não existia `config.toml`), o plugin já está em `~/.config/mosaic/plugins/calendar/calendar_widget.py`. Basta descomentar o bloco "Agenda Google" no `config.toml`.

Para quem já tem `~/.config/mosaic/config.toml`, copie o plugin à mão:

```bash
mkdir -p ~/.config/mosaic/plugins/calendar && cp plugins/calendar/calendar_widget.py ~/.config/mosaic/plugins/calendar/
```

## Variáveis `CALENDAR_*`

Configuradas em `[widgets.env]`.

| Variável | Obrigatória | Padrão | Para que serve |
| --- | --- | --- | --- |
| `CALENDAR_SOURCE` | sim | — | Fonte: `ics`, `goa` ou `gcalcli`. Outro valor é erro. |
| `CALENDAR_ICS_URL_ENV` | só na fonte `ics` | — | Nomes de variáveis de ambiente que guardam links ICS, separados por vírgula. |
| `CALENDAR_ICS_URL_FILE` | só na fonte `ics` | — | Caminho de um arquivo com um link ICS por linha (linhas em branco são ignoradas). |
| `CALENDAR_GOA_ACCOUNT` | não | todas as contas Google com agenda | E-mail de uma conta Google específica (fonte `goa`). |
| `CALENDAR_CALENDARS` | não | todas as agendas | Nomes de agendas, separados por vírgula (fontes `goa` e `gcalcli`). |
| `CALENDAR_TITLE` | não | `Agenda` | Título do widget. |

Na fonte `ics` é preciso definir `CALENDAR_ICS_URL_ENV` **ou** `CALENDAR_ICS_URL_FILE` (ou os dois; os links se somam).

## Como escolher a fonte

- **ICS**: funciona em qualquer sistema e em qualquer conta, desde que o administrador da conta permita o link ICS. Não depende de nada instalado.
- **GOA**: não guarda link nem token no `config.toml`, pois usa a conta que você já adicionou no GNOME. Só funciona em Linux com GNOME.
- **gcalcli**: útil se você já usa o gcalcli e quer as mesmas agendas dele. Exige a autenticação feita uma vez no terminal.

Use uma fonte por widget. Para ver duas fontes, crie dois widgets com `id` diferentes.

## Fonte ICS

1. No Google Agenda, abra **Configurações**, escolha a agenda na coluna da esquerda, e em **Integrar agenda** copie o **Endereço secreto em formato iCal**.
2. O link é um segredo: quem tiver o link lê a sua agenda. Não o coloque no `config.toml` nem em repositórios. Guarde-o numa variável de ambiente, por exemplo no seu `~/.profile`:

   ```bash
   export CALENDAR_ICS_PERSONAL="https://calendar.google.com/calendar/ical/.../basic.ics"
   ```

3. Se usar `inherit_env`, liste a variável nele, pois o widget não enxerga o ambiente do shell sem isso.
4. Para várias agendas, use vários links: `CALENDAR_ICS_URL_ENV = "CALENDAR_ICS_PERSONAL,CALENDAR_ICS_TRABALHO"`, ou um arquivo com um link por linha em `CALENDAR_ICS_URL_FILE`. Links `webcal://` são aceitos e convertidos para `https://`.
5. **Workspace**: administradores de contas Google Workspace podem desativar o link ICS. Se o link não funcionar e a sua conta for do trabalho, peça ao administrador ou use GOA ou gcalcli.
6. **Link vazado**: no Google Agenda, em **Integrar agenda**, redefina o endereço secreto. O link antigo deixa de funcionar; atualize a variável de ambiente com o novo.
7. **Convites recusados** aparecem na agenda: o formato ICS não diz qual participante é você, então o plugin não consegue filtrar convites recusados.

Configuração:

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

## Fonte GOA (GNOME Online Accounts)

Só Linux com GNOME.

1. Em **Configurações → Contas Online**, adicione a conta Google e deixe **Agenda** ligada.
2. Se usar `inherit_env`, adicione `DBUS_SESSION_BUS_ADDRESS`: sem ele o widget não consegue falar com o GNOME Online Accounts.
3. Se aparecer o aviso **"precisa entrar de novo"** (estado `AttentionNeeded`), entre de novo na conta em **Configurações → Contas Online**.

Configuração:

```toml
[[widgets]]
id = "agenda"
command = "python3"
args = ["plugins/calendar/calendar_widget.py"]
interval_seconds = 300
timeout_seconds = 20
inherit_env = ["DBUS_SESSION_BUS_ADDRESS"]

[widgets.env]
CALENDAR_SOURCE = "goa"
```

## Fonte gcalcli

1. **Instale** o gcalcli: `pipx install gcalcli`, ou o pacote da sua distribuição.
2. **Crie o seu próprio client OAuth**, seguindo o [guia de autenticação do gcalcli](https://github.com/insanum/gcalcli/blob/main/docs/api-auth.md): um projeto no Google Cloud, a API Google Agenda habilitada e um client OAuth do tipo **Desktop**.
3. **Publique o app como "In production"** na tela de consentimento OAuth (público-alvo). Com o app em "Testing", o token de atualização vence em 7 dias e o widget para de funcionar. Ao publicar, o Google mostra um aviso de app não verificado: clique em **Avançado** e continue. É esperado para uso pessoal.
4. **Autentique uma vez** no terminal, com `gcalcli init`. Sem isso, o widget mostra "gcalcli não respondeu".

Configuração:

```toml
[[widgets]]
id = "agenda"
command = "python3"
args = ["plugins/calendar/calendar_widget.py"]
interval_seconds = 300
timeout_seconds = 20

[widgets.env]
CALENDAR_SOURCE = "gcalcli"
```

Se o `gcalcli` não estiver no `PATH` do widget, ajuste `PATH` em `[widgets.env]`.

## Testar antes de abrir o mosaic

Use `mosaic check`, que roda o plugin com as mesmas regras do mosaic e imprime os problemas e a prévia do widget. Rode-o no diretório de configuração, como o mosaic faz:

```bash
mosaic check python3 plugins/calendar/calendar_widget.py --env CALENDAR_SOURCE=goa --cwd ~/.config/mosaic
```

Para a fonte ICS, exporte `CALENDAR_ICS_PERSONAL` no shell antes do comando e passe `--env CALENDAR_SOURCE=ics --env CALENDAR_ICS_URL_ENV=CALENDAR_ICS_PERSONAL`. Evite pôr o próprio link em `--env`, porque ele ficaria no histórico do shell. A saída de erro nunca mostra o link.

## Problemas comuns

**Fonte ICS**

- `defina CALENDAR_ICS_URL_ENV ou CALENDAR_ICS_URL_FILE para a fonte ics`: falta indicar onde está o link.
- `a variável 'X' indicada em CALENDAR_ICS_URL_ENV está vazia ou ausente`: a variável não existe no ambiente do widget. Confira se ela está em `inherit_env`.
- `nenhuma agenda ICS respondeu; confira o link`: o link está errado, foi redefinido ou foi desativado pelo Workspace.
- `agenda ICS N indisponível` (aviso): uma das agendas falhou e as outras foram mostradas.
- `o calendário excede o tamanho máximo permitido`: o calendário ultrapassa 8 MiB.

**Fonte GOA**

- `nenhuma conta Google com agenda no GNOME Online Accounts`: a conta não existe ou a Agenda está desligada nela.
- `não foi possível consultar o GNOME Online Accounts pelo D-Bus`: falta `DBUS_SESSION_BUS_ADDRESS` em `inherit_env`, ou o widget roda fora da sessão gráfica.
- `não foi possível consultar a API do Google Agenda`: sem rede, ou o token expirou.
- `nenhuma conta Google respondeu`: todas as contas falharam; veja os avisos.
- `conta X precisa entrar de novo em Configurações → Contas Online` (aviso): estado `AttentionNeeded`. Entre de novo na conta.
- `conta X indisponível` (aviso): uma conta falhou e as outras foram mostradas.

**Fonte gcalcli**

- `gcalcli não encontrado no PATH`: instale o gcalcli ou ajuste o `PATH`.
- `gcalcli não respondeu; rode gcalcli init no terminal`: a autenticação não foi feita, ou o gcalcli travou por mais de 15 s.
- `gcalcli falhou: ...`: o gcalcli devolveu erro; a mensagem traz a primeira linha de erro dele.
- `saída do gcalcli sem as colunas: ...`: versão do gcalcli incompatível com o plugin. Atualize o gcalcli.
- `linha do gcalcli ignorada` (aviso): uma linha da saída não pôde ser lida e foi descartada.

**Qualquer fonte**

- `CALENDAR_SOURCE precisa ser 'ics', 'goa' ou 'gcalcli'`: corrija o valor de `CALENDAR_SOURCE`.
- Falha total faz o plugin sair com `erro: ...` em stderr; o mosaic mostra o erro no widget e mantém o último dado bom. Falha parcial mostra os eventos que deu para ler, com uma linha de aviso.

## Roteiro de aceite

Conferência manual feita pelo dono da conta pessoal, antes de considerar o widget pronto:

1. Copiar o plugin para `~/.config/mosaic/plugins/calendar/` (seção de instalação acima).
2. **ICS**: configurar o link, rodar `mosaic check` e abrir o mosaic. Conferir que um evento recorrente semanal e um evento de dia inteiro aparecem nos dias certos e que eventos passados somem.
3. **GOA**: adicionar a conta em Configurações → Contas Online, rodar `mosaic check` com `CALENDAR_SOURCE=goa` e comparar a lista com a do ICS.
4. **gcalcli**: instalar, seguir a seção gcalcli até o `gcalcli init`, rodar `gcalcli --nocolor agenda --tsv --details id --details calendar --details location` uma vez à mão para conferir as colunas, depois `mosaic check` com `CALENDAR_SOURCE=gcalcli`.
5. Quebrar de propósito: link ICS errado (o widget mostra erro, sem o link) e `gcalcli` fora do `PATH` (o widget mostra a mensagem de não encontrado).
