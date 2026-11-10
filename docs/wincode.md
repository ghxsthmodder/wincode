# Wincode — correções e melhorias

Lista de pontos levantados na revisão de uso, com o estado de cada um. O que já está
aplicado aparece com **feito** e uma nota curta de como ficou.

## Editor

- **barra de rolagem vermelha** — *feito*. A cor vinha do WebView2, que no Windows 11 usa a
  cor de destaque do sistema; o Monaco ficava com a barra do tema nativo. Agora todas as
  barras (explorador, terminal, configurações, diálogos e editor) usam `--scrollbar` /
  `--scrollbar-hover`, derivadas do tema, e o Monaco recebe `scrollbarSlider.*` e
  `minimapSlider.*` do mesmo tom.

## Início

- **abrir arquivos em branco** — *feito*. O painel inicial (e o menu do explorador, e a
  paleta) ganhou **novo arquivo em branco…**: pede só o nome e abre uma aba sem caminho. O
  destino só é pedido no `Ctrl+S`, num diálogo com o caminho já sugerido dentro do projeto;
  também aceita um caminho completo. A aba deixa de ser "nova" assim que grava.

## Explorador

- **arquivo aberto some ao fechar a aba** — *feito*. No modo arquivo único, fechar a aba do
  arquivo fixado limpa a fixação (`/api/unpin`) e a árvore volta a listar a pasta.
- **menu ⋯ como toggle** — *feito*. O botão abre e o segundo clique fecha (fica marcado
  enquanto o menu está aberto); clicar fora continua fechando, como antes.
- **abrir terminal em uma pasta** — *feito*. Botão **›_** aparece na linha da pasta ao passar
  o mouse e abre o terminal já nela (clique esquerdo). O mesmo item existe no menu de
  contexto da pasta e no menu ⋯ ("abrir terminal na pasta selecionada").
- **copiar nomes no menu** — *feito*. **copiar nome** e **copiar caminho** no menu de
  contexto de qualquer item, com fallback para `execCommand` quando a API de clipboard não
  estiver disponível na origem do WebView.

## Terminal

- **terminal em tela cheia** — *feito*. Botão **⛶** no cabeçalho do terminal (e comando na
  paleta); `Esc`, o mesmo botão ou ocultar com `Ctrl+J` voltam ao normal.
- **caracteres não renderizáveis** — *feito*. A saída não é mais despejada crua:
  - a decodificação guarda a sequência UTF-8 cortada entre dois blocos (antes o acento
    virava `�`), aceita Latin-1/Windows-1252 para ferramentas antigas e reconhece UTF-16
    (BOM ou padrão de bytes nulos) do PowerShell;
  - o comando é precedido de `chcp 65001`, então `cmd.exe`, PowerShell 5.1 e Python escrevem
    UTF-8 em vez de usar a página de código do console;
  - códigos ANSI/OSC são interpretados e descartados (sem `←[32m`), `\r` reescreve a linha
    (barras de progresso deixam de empilhar), `\b`/`ESC [ K` apagam o trecho e o restante dos
    controles (BEL, NUL, C1) não aparece.

## Como conferir

- `Ctrl+Shift+P` → *novo arquivo em branco…* → digitar texto → `Ctrl+S` (o diálogo sugere
  `raiz/nome`) → a aba passa a mostrar o caminho na barra de status.
- Explorador: passar o mouse numa pasta (**›_**), abrir o menu **⋯** duas vezes, e o clique
  direito num item para ver **abrir terminal aqui**, **copiar nome** e **copiar caminho**.
- Terminal: `dir` numa pasta com acentos no nome, um comando que imprime acentos
  (`echo configuração não é opcional`) e uma ferramenta com barra de progresso; depois
  **⛶** e `Esc`.
- Editor: alternar entre temas claros e escuros e conferir as barras de rolagem.

## Pendências conhecidas

- O terminal é texto puro (sem cores) e não tem TTY: programas que exigem um terminal real
  continuam fora do escopo.
- A saída depende de UTF-8/Latin-1; quem trocar a página de código na mão (`chcp 850`) pode
  ver alguns acentos diferentes.
- Arquivo criado em branco e salvo fora da raiz do projeto fica editável, mas não entra na
  árvore, no `Ctrl+P` nem no recarregamento automático.
