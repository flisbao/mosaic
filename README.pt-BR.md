# Mosaic

Mosaic é um painel de terminal (TUI) feito de peças independentes. Cada widget é um programa, em Python, Node, shell ou qualquer linguagem, que imprime um JSON seguindo um protocolo simples e versionado. O Mosaic desenha o resultado em widgets (linhas, agenda, indicadores, tabelas), avisa quando algo muda e traz um pomodoro e uma integração opcional com LLMs.

> A terminal dashboard built from pluggable widgets. Any program that prints JSON becomes a widget.

## Instalação

Uma linha, sem `sudo` (Linux x86_64 e aarch64, macOS Apple Silicon e Intel). O script baixa o binário do seu sistema (estático no Linux, então roda em qualquer distribuição), confere o SHA-256 e instala em `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/flisbao/mosaic/main/install.sh | sh
```

`MOSAIC_VERSION=v0.0.11` instala uma versão específica e `MOSAIC_INSTALL_DIR=/pasta` muda o destino. Se o destino não estiver no `PATH`, o script imprime a linha para acrescentar. Precisa de `curl` ou `wget`, `tar` e `sha256sum` ou `shasum`.

Outras opções: Homebrew (`brew install flisbao/mosaic/mosaic`, disponível quando o tap `flisbao/homebrew-mosaic` for publicado; a fórmula `mosaic.rb` acompanha cada release), o `.deb` da [última release](https://github.com/flisbao/mosaic/releases/latest) (`sudo dpkg -i mosaic_*_amd64.deb`), o tarball conferido com o `.sha256` ou com o `SHA256SUMS`, ou `cargo install --path .` (Rust 1.88+).

## Atualização

```sh
mosaic update --check    # informa se há versão nova
mosaic update            # atualiza, perguntando [s/N] antes (--yes pula a pergunta)
```

O `mosaic update` descobre como o binário foi instalado. Binário do `install.sh` ou de tarball manual: baixa o tarball do seu sistema, confere o SHA-256, extrai ao lado do executável e troca de forma atômica, sem `sudo` (pasta sem permissão de escrita recebe uma explicação). Instalação por `.deb` não é alterada: ele imprime a URL do `.deb` novo e o `sudo apt install ./...`. Homebrew recebe `brew upgrade mosaic` e `cargo install` recebe o comando do cargo.

**Aviso de versão nova.** Ao abrir o painel, uma thread em segundo plano consulta o `github.com` (o redirecionamento de `releases/latest`, uma única requisição), no máximo uma vez a cada 24 horas, e mostra `versão X disponível; rode mosaic update` na faixa de avisos. Nenhum dado seu é enviado, a tela nunca espera e a falha é silenciosa. Para desligar: `check = false` em `[update]` no `mosaic.toml` ou `MOSAIC_NO_UPDATE_CHECK=1` no ambiente. `MOSAIC_DOWNLOAD_BASE` aponta o instalador e o `mosaic update` para um espelho da página de releases.

## Uso

```sh
cargo run --release
```

Na primeira execução o Mosaic cria `~/.config/mosaic/` com a configuração, os widgets de exemplo (kanban, example, galeria) e o prompt de prioridade.

Os widgets de serviço são instalados diretamente de repositórios públicos do GitHub com Git e Mosaic 0.0.10 ou mais recente: `mosaic plugins install jira github` usa atalhos oficiais; `mosaic plugins install flisbao/mosaic/plugins/jira` usa a referência `owner/repo/subdir`; `owner/repo` busca um manifesto na raiz. `mosaic plugins list` mostra os seis atalhos sem rede. Plugins de terceiros usam a referência completa, uma por chamada. A instalação mostra origem, versão, mínimo e destino antes de pedir `[s/N]`: Enter ou EOF cancela; `--yes` autoriza sem pergunta. `--force` permite substituir edições locais, mas não dispensa confirmação. Cada plugin declara `version` e `min_mosaic_version` no `setup.toml`; mínimo incompatível é recusado mesmo com `--force`. A origem segue a branch padrão, sem fixar revisão.

O pacote coletivo `mosaic-plugins` foi aposentado; instalações existentes continuam funcionando. Para copiar os arquivos antigos offline: `mosaic plugins install --from /usr/share/mosaic/plugins jira`. `--from DIR` também aceita a pasta `plugins` de um checkout local. O painel e a cópia local não exigem Git. `--to DIR` muda o destino dos plugins. A instalação remota só copia arquivos regulares diretos `.py`, `.js`, `README.md`, `SKILL.md` e `setup.toml`; não executa o plugin nem instala dependências. Scripts Python precisam de `python3`; scripts JavaScript precisam de `node`, selecionado pelo setup conforme a extensão. O campo `script` aponta ao caminho instalado `plugins/<nome>/<arquivo>.py`; o arquivo de origem fica diretamente na pasta selecionada no clone. A skill é ligada ao Claude Code quando `~/.claude` existe; `--no-skill` pula o vínculo e `--skills-dir DIR` muda sua pasta.

Depois rode `mosaic plugins setup jira`: a entrevista mostra o bloco e só grava no `mosaic.toml` após sua confirmação. Nunca pede o token, apenas o nome da variável ou o caminho do arquivo; `--print` mostra o bloco, `--advanced` inclui opções avançadas e `--config-dir DIR` muda a configuração. Pressione `R` no painel para recarregar.

Sem sair do painel, a tecla `+` abre a mesma entrevista como um modal: lista os plugins instalados que têm `setup.toml`, faz as mesmas perguntas com a mesma validação (uma resposta inválida mostra a mensagem e mantém a pergunta), mostra o bloco `[[widgets]]` e, com `Enter`, grava no `mosaic.toml` e recarrega como a tecla `R`. `Esc` (ou `Ctrl+C`) cancela em qualquer passo sem gravar nada. Sem plugin instalado o modal mostra `mosaic plugins install <nome>` e os nomes oficiais; instalar de dentro do painel não é suportado. O painel vazio diz `nenhum widget ainda` e aponta para a tecla `+`.

Se preferir editar à mão, `mosaic --default-config` mostra todas as opções comentadas. Uma chave que o Mosaic não conhece é ignorada com aviso, no painel e no `mosaic check --config`. Se você já usava a versão anterior, copie `~/.config/panel-tui/` para `~/.config/mosaic/`.

O prompt é enviado ao `claude` e ao `opencode` pela entrada padrão, nunca como argumento, para não aparecer em `ps`.

## Idioma

A interface é em inglês por padrão; o português do Brasil (`pt-BR`) está disponível. O Mosaic escolhe o idioma nesta ordem:

1. `language` em `[ui]` no `mosaic.toml`: `"en"` ou `"pt-BR"`.
2. A variável de ambiente `MOSAIC_LANGUAGE` (mesmos valores).
3. O locale: a primeira variável não vazia entre `LC_ALL`, `LC_MESSAGES` e `LANG`. Valor que começa com `pt` vale pt-BR; qualquer outro vale inglês.

```toml
[ui]
language = "pt-BR"
```

Um valor inválido em `[ui] language` vira aviso e o Mosaic volta para a detecção acima. Aperte `R` para recarregar o `mosaic.toml` e aplicar um idioma novo. Os plugins oficiais seguem o mesmo idioma: o Mosaic passa `MOSAIC_LANGUAGE` (`en` ou `pt-BR`) a cada provider e ação (veja [`PROTOCOL.pt-BR.md`](PROTOCOL.pt-BR.md)), e um plugin sem a variável responde em inglês. Dados que vêm de um serviço, como o título de uma issue, nunca são traduzidos. O `install.sh` escreve só em inglês, porque roda antes de o Mosaic existir.

<!-- demo: ponha aqui o GIF gravado com `vhs scripts/demo.tape` (passo manual) -->

## Escrevendo um widget

Um widget responde ao argumento `snapshot` imprimindo um JSON:

```python
import json
print(json.dumps({"summary": {"title": "Olá", "blocks": [
    {"type": "lines", "lines": [{"text": "tudo certo", "tone": "ok"}]}
]}}))
```

Valide com `mosaic check python3 meu_widget.py` (o `check` executa o script a partir do diretório atual; só valide widgets em que você confia). O contrato completo está em [`PROTOCOL.md`](PROTOCOL.md) e o esquema JSON em [`schema/protocol.schema.json`](schema/protocol.schema.json). Os widgets `plugins/gallery.py` e `plugins/gallery.js` mostram todos os blocos.

## Contribuindo

Um plugin novo precisa vir com a sua skill (`plugins/<pasta>/SKILL.md`) e com o roteiro do `mosaic plugins setup` (`plugins/<pasta>/setup.toml`). O passo a passo está em [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Jira

O provider do Jira Cloud (`plugins/jira/jira.py`) é genérico: cada widget define o próprio JQL e como mostrar o resultado. Exemplos, opções, variáveis e como testar com uma conta gratuita estão em [`plugins/jira/README.md`](plugins/jira/README.md).

## Abas e atalhos

Sem `[[tabs]]` na configuração, todos os widgets ficam numa aba única. Para separar em abas, declare cada aba com `name` e aponte os widgets para ela com `tab`. `max_items` limita quantos itens o widget mostra:

```toml
[[tabs]]
name = "Trabalho"

[[tabs]]
name = "Pessoal"

[[widgets]]
id = "jira-minhas-tarefas"
command = "python3"
args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC", "--title", "Minhas tarefas"]
interval_seconds = 120
tab = "Trabalho"
max_items = 5

[[widgets]]
id = "galeria"
command = "python3"
args = ["plugins/gallery.py"]
tab = "Pessoal"
```

Um widget sem `tab` vai para a primeira aba. Você não precisa editar o arquivo à mão para usar abas:

```sh
mosaic tabs list                 # cada aba com os seus widgets
mosaic tabs add Estudos          # declara uma aba nova
mosaic plugins setup jira        # também pergunta em que aba o widget fica (ou cria uma aba nova)
```

Cuidado com a primeira aba: se o arquivo ainda não declara `[[tabs]]`, a primeira que você criar passa a ser a casa de todos os widgets que já existem (eles não têm `tab`), e o mosaic avisa antes de gravar.

Teclas (modo grade, com os widgets visíveis):

| Tecla | Ação |
| --- | --- |
| `1` a `9` | Vai para a aba de mesmo número |
| `Tab` / `BackTab` | Próxima / aba anterior |
| `←` `↑` `→` `↓` ou `h` `j` `k` `l` | Move o foco entre os widgets |
| `Enter` | Expande o widget focado |
| `Shift+letra` ou `Alt+letra` (`A` a `Z`, exceto `R`) | Foca o widget com essa letra de atalho. `Alt` exige que o terminal envie `Esc` antes da letra (no macOS, ative "Option como Meta") |
| `a` | Abre o menu de ações do widget focado |
| `r` | Atualiza todos os widgets agora |
| `R` | Recarrega o `mosaic.toml` |
| `+` | Adiciona um widget: escolhe um plugin instalado, responde às perguntas, revisa o bloco e grava |
| `i` | Pede uma recomendação à IA (se `[llm]` estiver ligado) |
| `p` / `x` | Inicia ou pausa / reinicia o pomodoro |
| `q` ou `Ctrl+C` | Sai |
| `?` | Abre a ajuda com as teclas da tela atual (`Esc` ou `?` fecham) |

Com um widget expandido, as setas (ou `h` `j` `k` `l`) movem entre os itens, `PageUp`/`PageDown` pulam vários itens, `Home`/`End` vão ao primeiro e ao último, e `Esc` volta à grade. No menu de ações, `Enter` escolhe a ação, `s` confirma e `n` ou `Esc` cancela. A tecla `1` a `9` troca de aba só na grade. A última linha do terminal mostra as dicas de teclas do contexto (a partir de 21 linhas de altura) e cada aba exibe o número que a abre. `Enter` em um widget sem `detail` mostra um aviso em vez de expandir.

Após editar a configuração, `R` recarrega widgets, abas e intervalos. Mudanças em `[llm]`, `[pomodoro]` e notificações só valem após reiniciar o Mosaic, mesmo com a recarga.

`Ctrl+C` (ou `SIGTERM`/`SIGHUP`) encerra o Mosaic com calma: restaura o terminal e finaliza os providers em execução. Um **segundo** sinal enquanto ele ainda encerra força a saída imediata (código 1): o terminal pode ficar em modo bruto, bastando rodar `reset`, e providers que ainda estavam em execução podem continuar vivos até terminarem sozinhos.

Valide o arquivo sem abrir o painel:

```sh
mosaic check --config            # usa ~/.config/mosaic/mosaic.toml
mosaic check --config caminho.toml
```

Por padrão, todo widget herda o ambiente do Mosaic (compatível com versões anteriores). Para que um widget receba só o necessário, liste as variáveis em `inherit_env`: `PATH`, `HOME`, `LC_*`, `XDG_*` e algumas outras básicas sempre passam, o resto é removido, e `[widgets.env]` sempre vale. Exemplo para o Jira, que lê o token pela variável nomeada em `JIRA_TOKEN_ENV`:

```toml
[[widgets]]
id = "jira-empresa"
command = "python3"
args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done"]
inherit_env = ["JIRA_TOKEN_EMPRESA"]

[widgets.env]
JIRA_SITE = "empresa.atlassian.net"
JIRA_EMAIL = "voce@empresa.com"
JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"
```

O `notify_command` e a CLI da IA ainda herdam o ambiente inteiro.

Título e corpo vêm do provider: o Mosaic limita cada um a 200 caracteres, acrescenta um espaço na frente de textos que começam com `-` e escapa `&`, `<` e `>` no corpo. Ainda assim, prefira programas que não interpretem o texto de outras formas.

`[pomodoro] notify_command` é uma lista de argumentos (ex.: `["notify-send", "--urgency=low"]`); o Mosaic acrescenta título e corpo como os **dois últimos argumentos**, sem shell. Cada notificação tem limite de 5 s e no máximo 4 rodam ao mesmo tempo. Título e corpo vêm do provider: não use programas que interpretem argumentos começando com `-` de forma perigosa.

## Licença

MIT
