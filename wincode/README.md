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
    protocol.rs             # rotas /api/* e assets
    assets.rs               # assets embutidos (override opcional em disco)
    fsapi.rs                # raiz do projeto + resolução segura de caminhos
    jobs.rs                 # execução assíncrona dos comandos do terminal
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

Sem argumentos, o editor **não lista o diretório atual**: abre uma tela inicial para
escolher uma pasta ou um arquivo. Ao escolher um arquivo, a pasta dele vira a raiz do
projeto e o arquivo já abre em uma aba. Cancelar o diálogo mantém a tela inicial.

Também é possível definir a pasta por variável de ambiente:

```powershell
set WINCODE_ROOT=C:\projetos\meu-app
```

Os botões **abrir pasta** e **abrir arquivo** na barra superior abrem o diálogo nativo
do Windows (`IFileOpenDialog` via crate `rfd`) e trocam a raiz do projeto sem reiniciar
o editor.

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

## Gerenciamento de arquivos e pastas

No **Explorador**, use o menu **⋯** no topo do painel (com **novo arquivo** e **nova pasta**)
ou o **clique direito** em qualquer item (ou no espaço vazio, para criar na raiz):

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

- **Editor (Monaco)**: tema (`vs-dark`, `vs`, alto contraste), família e tamanho da fonte,
altura da linha, indentação (espaços ou tabulação + largura), detecção da indentação
do arquivo, quebra de linha, números de linha, espaços em branco visíveis, estilo do
cursor, minimapa, rolagem suave e **salvar automaticamente** (~1s após parar de digitar)
- **Interface**: tema claro/escuro (independente do tema do editor), tamanho da fonte da
interface, largura do explorador, altura do terminal, exibição dos ícones da árvore e
visibilidade do explorador, das abas e do terminal
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

## Paleta de comandos

`Ctrl+Shift+P` abre a paleta, que reaproveita o mesmo modal das configurações. Digite
para filtrar (casamento por subsequência: `cfg` acha *configurações*), navegue com
`↑`/`↓` e execute com `Enter`.

Comandos disponíveis:

- **arquivo**: salvar, renomear e excluir o arquivo atual, novo arquivo/pasta, recarregar a árvore
- **projeto**: abrir pasta/arquivo, mostrar/ocultar explorador, abas e terminal, focar e limpar o terminal
- **configurações**: abrir o painel, alternar minimapa, quebra de linha, números de linha,
  indentação (espaços ↔ tabulação), tamanho da fonte, tema do editor, tema da interface,
  ícones da árvore e salvamento automático; restaurar padrões

Os toggles aparecem com o rótulo do próximo estado (ex.: *desligar minimapa*) e
persistem na hora, do mesmo jeito que no painel.

## Atalhos e comandos

- `Ctrl+S` — salva o arquivo aberto
- `Ctrl+,` — abre/fecha as configurações (`Esc` também fecha)
- `Ctrl+Shift+P` — abre a paleta de comandos (`Esc` também fecha)
- `Ctrl+B` — mostra/oculta o explorador
- `Ctrl+J` — mostra/oculta o terminal

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

- A troca de pasta recomeça a sessão: arquivos abertos e modelos do Monaco são descartados
  (alterações não salvas são perdidas — salve antes de trocar).
- Terminal não é interativo (sem stdin/TTY): comandos que pedem entrada ficam bloqueados.
  Por rodarem sem janela de console, programas que dependem de um terminal real (editores
  de texto interativos, `pause`, prompts de senha) não funcionam.
- Alvos não-Windows compilam apenas como conveniência de desenvolvimento e exigem
  `pkg-config`, `libgtk-3-dev` e `libwebkit2gtk-4.1-dev`.
