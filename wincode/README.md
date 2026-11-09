# Wincode

Editor de código minimalista em Rust com layout inspirado no VSCode:

```
file tree view | editor (Monaco) | terminal
```

Escopo: **Windows x64**, distribuição em **um único `.exe`** (~30 MB, sem pastas de assets).

## Como funciona

- **Rust + Wry**: a janela usa o **WebView2** já presente no Windows 10/11. Sem Electron, sem Node no runtime.
- **Monaco Editor 0.57**: o pacote oficial (build AMD/min) é baixado uma vez e **embutido no binário**
  via `include_bytes!` gerado pelo `build.rs`.
- **Comunicação**: o front-end fala com o Rust por um protocolo customizado (`editor://localhost`),
  que serve os assets e uma API de arquivos/terminal. Nada de porta TCP aberta.
- **Ícone do app**: `assets/icons/wincode.ico` é embutido no binário e usado na janela, na
  barra de tarefas e (no build MSVC) como ícone do próprio `.exe` no Explorador do Windows.
- **Ícones**: o explorador usa um subconjunto do [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
  (o mesmo conjunto do VSCode). São SVGs avulsos em `assets/icons/`, embutidos no `.exe` — nada é
  carregado da internet em runtime.

## Estrutura

```
wincode/
  Cargo.toml
  build.rs                  # gera a tabela de assets embutidos
  scripts/prepare-monaco.js # baixa o Monaco para assets/monaco/vs
  assets/
    index.html              # UI (árvore | Monaco | terminal)
    icons/                  # ícones de tipo de arquivo/pasta (Material Icon Theme, MIT)
    monaco/vs/              # Monaco 0.57 (gerado pelo script)
  src/
    main.rs                 # janela + WebView
    protocol.rs             # rotas /api/*, fontes e assets
    assets.rs               # assets embutidos (override opcional em disco)
    config.rs               # pasta portátil wincodeconfig/ ao lado do .exe
    fsapi.rs                # raiz do projeto + resolução segura de caminhos
    fonts.rs                # fontes do usuário (lê a família do .ttf/.otf)
    index.rs                # índice de arquivos da abertura rápida (Ctrl+P)
    jobs.rs                 # execução assíncrona dos comandos do terminal
    recent.rs               # projetos recentes da tela inicial
    session.rs              # sessão por projeto (abas, cursores, pastas abertas)
    settings.rs             # preferências do editor (JSON no config do usuário)
```

## Preparar o Monaco

```sh
node scripts/prepare-monaco.js
```

Baixa `monaco-editor@0.57.0` para `.cache/` e extrai em `assets/monaco/vs`.
Os assets são embutidos no `.exe` na próxima compilação.

## Build (Windows x64)

```powershell
cargo build --release --target x86_64-pc-windows-msvc
```

Saída: `target\x86_64-pc-windows-msvc\release\wincode.exe`.

Alternativa: o workflow `build` do GitHub Actions compila em `windows-latest` e publica o
artefato `wincode-windows-x64` a cada push em `main`.

`Cargo.toml` usa `+crt-static`, então o executável não depende do runtime do Visual C++.
O WebView2 é o único componente do sistema (já vem no Windows 10/11; há o
[Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) para versões antigas).

## Uso

```powershell
wincode.exe                                   # mostra a tela inicial de seleção
wincode.exe C:\projetos\meu-app               # abre a pasta
wincode.exe C:\projetos\meu-app\src\main.rs  # abre a pasta do arquivo e o arquivo
```

Sem argumentos, o editor **não lista o diretório atual**: abre um painel inicial no lugar
do editor, com **abrir pasta…**, **abrir arquivo…** e os **projetos recentes** (os dez
últimos, guardados em `wincodeconfig/recent.json`). Cancelar o diálogo mantém o painel.

Ao escolher um **arquivo**, a pasta dele vira a raiz do projeto (para o terminal), mas o
explorador mostra **apenas o arquivo aberto** — nada de listar a pasta inteira, como no
VSCode. Criar arquivos e pastas continua disponível no menu e usa a pasta do arquivo,
mas o item novo abre como aba (a árvore segue fixa no arquivo); use **abrir pasta…** no
menu do explorador para voltar à árvore completa. Já o argumento de CLI com um arquivo
abre esse arquivo em uma aba e entra no mesmo modo.

Também é possível definir a pasta por variável de ambiente:

```powershell
set WINCODE_ROOT=C:\projetos\meu-app
```

O menu **⋯** do explorador (e o clique direito no espaço vazio da árvore) concentra
os cinco itens: **abrir pasta…**, **abrir arquivo…**, **recarregar explorador**,
**novo arquivo** e **nova pasta** (sem pasta escolhida, criar apenas avisa na barra de
status). Os dois primeiros abrem o diálogo nativo do Windows (`IFileOpenDialog`
via crate `rfd`) e trocam a raiz do projeto sem reiniciar o editor.

## Painéis

O explorador e o terminal podem ser redimensionados e ocultados:

- arraste a alça vertical à direita do explorador para mudar a largura (180–640 px,
  sempre sobrando espaço para o editor); arraste a alça horizontal logo acima do terminal
  para a altura (100–400 px). As alças acendem na cor de destaque ao passar o mouse
- **Ctrl+B** mostra/oculta o explorador; **Ctrl+J** mostra/oculta o terminal
- os botões **explorador** e **terminal** na barra superior fazem o mesmo, e o painel
  de configurações traz as opções de exibição e as medidas exatas

Largura, altura e visibilidade ficam salvas no JSON de configurações.

## Recarregamento automático

A cada ~2 segundos o editor confere **apenas o que está visível**: a pasta raiz, as
pastas expandidas e os arquivos abertos (pastas dentro de uma pasta recolhida saem da
checagem). Quando algo muda, só a pasta afetada é relida e suas linhas são atualizadas
no lugar — o resto da árvore, a rolagem e o que já estava carregado não são tocados.
No modo arquivo único só o próprio arquivo entra na checagem: a pasta dele pode ser
enorme (Downloads, Área de Trabalho) e não é lida a cada dois segundos.
Arquivos abertos **sem alterações pendentes** são relidos do disco; abas com edições não
salvas nunca são sobrescritas — salve ou descarte antes.

## Abas

Cada arquivo aberto vira uma aba acima do editor:

- clique troca para o arquivo (sem reler o disco, mantendo edições não salvas)
- **×** (ou clique do meio) fecha a aba; se houver alterações não salvas, um diálogo pede confirmação
- **●** marca a aba com alterações pendentes e acompanha o digitar
- renomear atualiza o nome/caminho da aba; excluir fecha as abas afetadas;
  fechar a aba ativa passa para a última aberta, e a última fechada deixa o editor vazio

A ordem das abas segue a ordem de abertura, e a paleta (`Ctrl+Shift+P`) também
oferece **fechar arquivo atual**.

## Abertura rápida (Ctrl+P)

`Ctrl+P` abre a busca de arquivos do projeto: as abas abertas aparecem primeiro e, abaixo,
o índice da pasta. O filtro é por subsequência e dá peso ao **nome** do arquivo
(`apconf` acha `app.config.json`), com a pasta relativa à direita; `Enter` abre (ou só
troca para a aba, se já estiver aberta) e `↑`/`↓` navegam. Digitar `>` na primeira posição
troca para a paleta de comandos.

O índice é montado na primeira abertura e fica em memória enquanto a pasta estiver aberta
(criar, renomear ou excluir arquivos o invalida). Para não varrer o que ninguém abre,
ficam de fora `node_modules`, `.git`, `target`, `dist`, `build`, `.venv`, `__pycache__` e
outras pastas de cache/saída, com teto de 8000 arquivos e profundidade 16.

## Busca em arquivos (Ctrl+Shift+F)

`Ctrl+Shift+F` abre a busca no mesmo modal, com resultados atualizados enquanto você
digita (250 ms de espera e a partir de dois caracteres). Cada linha mostra o trecho da
ocorrência como título e `arquivo:linha` à direita; `Enter` abre o arquivo (ou troca
para a aba) com o cursor na posição exata.

A busca é textual, sem diferenciar maiúsculas, feita em Rust sobre o mesmo índice do
`Ctrl+P` — herda as pastas ignoradas e não exige `rg` nem `findstr` instalados. Limites
por resposta: 200 ocorrências (20 por arquivo), arquivos até 1 MB, binários de fora e
orçamento de 1,5 s; quando algo fica de fora, a barra de status avisa.

## Sessão por projeto

Cada projeto guarda sua sessão em `wincodeconfig/sessions/<chave>.json`, com chave
derivada do caminho (sem diferenciar maiúsculas nem barras): abas abertas, aba ativa,
linha/coluna e rolagem de cada uma, e as pastas expandidas. Ao abrir a pasta — pelo painel
inicial, pela lista de recentes ou pela CLI — o editor **volta onde parou**; um projeto
aberto pela primeira vez continua caindo no painel inicial.

Trocar de projeto com abas alteradas abre um diálogo com **salvar e trocar** (padrão),
**descartar** e **cancelar**: nada é fechado sem escolha. A sessão do projeto que sai é
gravada antes da troca, então voltar para ele reabre as mesmas abas nos mesmos lugares.
No modo arquivo único nada é gravado — abrir um arquivo avulso não sobrescreve a sessão
cujo caminho o contém.

## Gerenciamento de arquivos e pastas

No **Explorador**, use o menu **⋯** no topo do painel (abrir pasta/arquivo, recarregar,
novo arquivo e nova pasta) ou o **clique direito** em qualquer item:

- **novo arquivo** e **nova pasta** — criam dentro da pasta clicada (ou da pasta selecionada)
- **renomear** — mantém o arquivo aberto no editor, inclusive alterações não salvas
- **excluir** — pede confirmação e remove pastas com todo o conteúdo (sem lixeira)

Tudo passa pela mesma validação do backend: nomes sem `/ \ : * ? " < > |`,
proibido sobrescrever item existente e nenhuma operação fora da pasta do projeto.
Se o terminal estava dentro de uma pasta renomeada/excluída, ele acompanha a mudança
(ou volta para a raiz).

## Configurações

O botão **⚙ configurações** (ou `Ctrl+,`) abre o painel de preferências, aplicado
ao vivo e salvo automaticamente:

- **Editor (Monaco)**: tema (o pacote está em [Temas](#temas)), família e tamanho da fonte,
ligaduras, altura da linha, indentação (espaços ou tabulação + largura), detecção da
indentação do arquivo, quebra de linha, números de linha, espaços em branco visíveis,
estilo do cursor, minimapa, rolagem suave e **salvar automaticamente** (~1s após parar de digitar)
- **Interface**: tema da interface (`seguir o tema do editor`, `escuro` ou `claro`), tamanho
  da fonte da interface, largura do explorador, altura do terminal, exibição dos ícones da
  árvore e visibilidade do explorador, das abas e do terminal
- **Terminal**: tamanho da fonte
- **restaurar padrões** devolve tudo ao inicial

A indentação vale também para os arquivos já abertos, e o tema do Monaco é trocado sem
reabrir o editor. As preferências ficam em um JSON fora do projeto, no diretório de
configuração do usuário:

```
%APPDATA%\Wincode\settings.json
```

Valores fora da faixa ou desconhecidos são ignorados na leitura, então editar o arquivo
à mão não quebra a interface.

## Temas

Além dos quatro temas nativos do Monaco (`vs-dark`, `vs`, alto contraste escuro e claro), o
editor traz 34 temas com as cores do código **e** da interface: Dracula, Monokai, Solarized
Dark/Light, Nord, One Dark/Light, GitHub Dark/Light, Gruvbox Dark/Light, Tokyo Night (e Storm),
Catppuccin Mocha/Macchiato/Frappé/Latte, Material Palenight/Ocean, Ayu Dark/Mirage/Light,
Cobalt2, Night Owl, SynthWave '84, Shades of Purple, Rosé Pine (e Moon), Everforest, Kanagawa,
Zenburn, Quiet Light, Horizon e Darcula.

Cada tema é uma linha de dados em `THEME_SPECS`: fundo, texto, comentário, destaque, sintaxe
principal e perigo. O realce da linha, bordas, seleção, widgets, barra de rolagem e a paleta
da interface são derivados em `editorColors`/`uiPalette`, então incluir um tema novo é copiar
uma linha e trocar os hexadecimais. Com **tema da interface = seguir o tema do editor**, o
explorador, as abas, o terminal e os menus acompanham o tema; `escuro` e `claro` voltam às
paletas fixas, independentes do editor.

## Fontes

A lista do painel traz Cascadia Code/Mono/Next, JetBrains Mono, Fira Code, Iosevka, Monoid,
Hasklig, Victor Mono, IBM Plex Mono, Source Code Pro, Inconsolata, Hack, Ubuntu Mono, Roboto
Mono, Space Mono, Geist Mono, Commit Mono, Maple Mono, Red Hat Mono, Noto Sans Mono, Anonymous
Pro, Cousine, DejaVu/Droid Sans Mono, Consolas, Menlo e SF Mono. O campo é livre e o primeiro
item do stack realmente instalado é o usado. **Ligaduras** transforma `=>`, `!=` e `->` em
glifos únicos nas fontes que suportam (Fira Code, JetBrains Mono, Hasklig, Victor Mono, Iosevka…).

Para usar uma fonte própria, basta jogar o `.ttf`/`.otf` em `wincodeconfig/fonts`, ao lado do
`.exe` — ela não precisa estar instalada no Windows. O Wincode lê a família da tabela `name` do
arquivo, registra o `@font-face` e adiciona o nome à lista. Um `.json` de mesmo nome ajusta
rótulo, peso e estilo (ou aponta para outro arquivo):

```json
{
  "file": "JetBrainsMonoNLNerdFont-Regular.ttf",
  "label": "JetBrains Mono NL (local)",
  "family": "JetBrains Mono NL",
  "weight": "400",
  "style": "normal"
}
```

**fontes personalizadas → recarregar** relê a pasta sem reiniciar (abrir as configurações já
relê). A URL leva `?v=<mtime>`, então substituir o arquivo invalida o cache do WebView.

## Paleta de comandos

`Ctrl+Shift+P` abre a paleta, que reaproveita o mesmo modal das configurações e da
abertura rápida. Digite para filtrar (casamento por subsequência: `cfg` acha
*configurações*), navegue com `↑`/`↓` e execute com `Enter`. Dentro do `Ctrl+P`,
digitar `>` troca para a paleta.

Comandos disponíveis:

- **arquivo**: abrir rapidamente (Ctrl+P), buscar em arquivos (Ctrl+Shift+F), salvar,
  renomear e excluir o arquivo atual, novo arquivo/pasta, recarregar a árvore
- **projeto**: abrir pasta/arquivo, mostrar/ocultar explorador, abas e terminal, focar e limpar o terminal
- **configurações**: abrir o painel, alternar minimapa, quebra de linha, números de linha,
  indentação (espaços ↔ tabulação), tamanho da fonte, tema do editor, tema da interface,
  ícones da árvore e salvamento automático; restaurar padrões

Os toggles aparecem com o rótulo do próximo estado (ex.: *desligar minimapa*) e
persistem na hora, do mesmo jeito que no painel.

## Atalhos e comandos

- `Ctrl+S` — salva o arquivo aberto
- `Ctrl+Shift+S` — salva todas as abas com alterações
- `Ctrl+W` — fecha a aba atual (pede confirmação se houver alterações)
- `Ctrl+Tab` / `Ctrl+Shift+Tab` — próxima/anterior, na ordem de abertura
- `Ctrl+P` — abertura rápida de arquivos do projeto (`>` troca para a paleta)
- `Ctrl+Shift+F` — busca textual nos arquivos do projeto (`>` troca para a paleta)
- `Ctrl+G` — vai para uma linha
- `Ctrl+=` / `Ctrl+-` / `Ctrl+0` — aumenta, diminui ou volta a fonte do editor ao padrão
- `Shift+Alt+F` — formata o documento nas linguagens com formatador no Monaco
  (JavaScript/TypeScript, JSON, CSS, HTML)
- `Ctrl+,` — abre/fecha as configurações (`Esc` também fecha)
- `Ctrl+Shift+P` — abre a paleta de comandos (`Esc` também fecha)
- `Ctrl+B` — mostra/oculta o explorador
- `Ctrl+J` — mostra/oculta o terminal

Os atalhos do navegador embutido (recarregar, imprimir, zoom da página) ficam desligados
na janela, então combinações como `Ctrl+=` chegam ao editor em vez de ampliar a interface.

## Linguagens

O realce de sintaxe do Monaco cobre dezenas de linguagens, entre elas **Lua** (`.lua`),
Rust, JavaScript/TypeScript (incluindo JSX/TSX), Python, Go, C/C++, C#, Java, Kotlin,
Swift, Dart, Scala, Ruby, PHP, Shell, PowerShell, Bat, HTML/CSS/SCSS/Less, JSON, YAML,
TOML/INI, Markdown/MDX, XML, SQL, GraphQL, Dockerfile, Terraform/HCL, Elixir, Erlang,
Haskell, Clojure, Julia, Perl, R, Objective-C, protobuf, Solidity e outras.
Extensões desconhecidas abrem como texto puro.
- Terminal: qualquer comando (`dir`, `git status`, `cargo build`…) executa via `cmd.exe /C`
  no diretório atual do terminal, com saída em streaming. O processo é criado com
  `CREATE_NO_WINDOW`, então **nenhuma janela de console aparece** sobre o editor
- Comandos internos do terminal: `cd <pasta>`, `pwd`, `ls`/`dir`, `clear`/`cls`, `open <arquivo>`
- O prompt é a última linha do buffer (não uma barra fixa): o comando digitado é ecoado ali
  mesmo, rola junto com a saída e aceita **↑**/**↓** para o histórico (a linha em edição é
  preservada ao voltar), **Ctrl+C** para abandonar a linha e **Ctrl+L** para limpar

## Desenvolvimento do front-end sem recompilar

Aponte `WINCODE_ASSET_DIR` para a pasta `assets/` e os arquivos em disco passam
a ter prioridade sobre os embutidos:

```powershell
set WINCODE_ASSET_DIR=%CD%\assets
cargo run
```

## Créditos

Os ícones de arquivos e pastas vêm do **Material Icon Theme** (MIT, © Material Extensions) —
licença em `assets/icons/LICENSE-material-icon-theme.txt`.

## Limitações atuais

- A troca de projeto fecha as abas; o diálogo oferece salvar ou descartar, e a sessão
  (abas, cursores, pastas expandidas) volta ao reabrir — mas o conteúdo descartado não.
- No modo arquivo único, abrir outros arquivos (pelo terminal ou pelo `Ctrl+P`) cria abas
  normalmente, mas o explorador continua mostrando só o arquivo escolhido; use
  **abrir pasta…** para voltar à árvore do projeto.
- A busca é textual e sem expressões regulares; para padrões complexos, use o terminal
  (`findstr`/`rg`).
- `wincodeconfig` precisa ser gravável para guardar a lista de recentes; para as fontes basta
  que a pasta seja legível (útil quando o `.exe` fica em `Program Files`).
- Terminal não é interativo (sem stdin/TTY): comandos que pedem entrada ficam bloqueados.
  Por rodarem sem janela de console, programas que dependem de um terminal real (editores
  de texto interativos, `pause`, prompts de senha) não funcionam.
- Alvos não-Windows compilam apenas como conveniência de desenvolvimento e exigem
  `pkg-config`, `libgtk-3-dev` e `libwebkit2gtk-4.1-dev`.
