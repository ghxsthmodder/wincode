# Wincode — correções e melhorias

Lista de pontos levantados na revisão de uso, com o estado de cada um. O que já está
aplicado aparece com **feito** e uma nota curta de como ficou. Os recursos do editor
(atalhos, abas, busca, sessão, temas) estão no `wincode/README.md`; este documento
acompanha as correções e melhorias pedidas.

## Editor

- **barra de rolagem vermelha** — *feito*. A suspeita é o WebView2, que no Windows 11
  pinta as barras nativas com a cor de destaque do sistema. Para não depender disso, todas
  as barras (explorador, terminal, configurações e diálogos) passaram a usar `--scrollbar` /
  `--scrollbar-hover`, derivadas do tema, e o Monaco recebeu `scrollbarSlider.*` e
  `minimapSlider.*` do mesmo tom. Vale conferir nos temas claros e escuros.

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

## Segunda rodada (ganhos rápidos)

- **interromper o comando em execução** — *feito*. Enquanto algo roda, o cabeçalho do
  terminal mostra **■ parar**, e `Ctrl+C` no prompt interrompe também. O processo é morto em
  árvore (`taskkill /T /F` via `/api/termkill`), então filhos de `cargo build`/`npm run dev`
  não ficam rodando; sem comando, `Ctrl+C` segue abandonando a linha digitada. O comando
  também aparece na paleta durante a execução.
- **salvar como…** — *feito*. Grava em outro caminho e leva a aba para lá; **salvar uma
  cópia…** grava sem mexer na aba. Caminho relativo continua preso ao projeto; completo pode
  sair dele (mesma rota `/api/saveas` do arquivo em branco).
- **reabrir aba fechada** — *feito*. `Ctrl+Shift+T` desfaz o último fechamento (pilha de 20).
  Arquivo salvo volta do disco com cursor e rolagem; aba em branco volta com o texto, ainda
  sem caminho.
- **ir para símbolo** — *feito*. `Ctrl+Shift+O` lista as declarações do arquivo ativo
  (funções, classes, tipos, `impl`, métodos, variáveis e cabeçalhos Markdown) com filtro por
  subsequência; o Monaco standalone não tem o *outline* do VSCode, então o mapa é textual.
- **revelar no explorador / breadcrumb** — *feito*. Faixa abaixo das abas com o caminho do
  arquivo ativo em trechos clicáveis: cada um expande a pasta no explorador e destaca a
  linha. Na paleta, **revelar arquivo ativo no explorador** faz o mesmo.
- **atalhos de linha** — *feito*. `Alt+↑`/`Alt+↓` movem a linha, `Shift+Alt+↓` duplica e
  `Ctrl+/` comenta (comandos nativos do Monaco ligados ao editor).
- **settings.json no editor** — *feito*. **abrir settings.json** na paleta abre o arquivo real
  de preferências numa aba; o `Ctrl+S` passa por `/api/savesettings`, que valida o JSON e
  reaplica as opções na hora. JSON inválido é recusado e a aba continua pendente.

## Terceira rodada (novos recursos)

- **git na interface** — *feito*. A barra de status mostra **⑂ branch • N** quando a pasta
  é um repositório; o clique abre um painel com a lista de arquivos alterados (clique abre
  o arquivo), o resumo (à frente/atrás) e o campo de mensagem. O **commit** roda
  `git add -A` + `git commit -m` e mostra a saída do git, inclusive quando não há nada para
  commitar. As linhas do explorador ganham a letra do estado (M/A/D/R/?). O status é pedido
  ao abrir o painel, depois de salvar e ao recarregar a árvore — não a cada tecla.
- **confirmação de sobrescrita** — *feito*. Antes de gravar um "salvar como", o destino é
  conferido com `/api/pathexists`: se já existe, um diálogo pede confirmação; se é uma
  pasta, o salvamento nem começa.
- **histórico do terminal persistente** — *feito*. Os comandos vão para
  `wincodeconfig/history.json` (500 últimos, sem repetições seguidas) cerca de um segundo
  depois de cada execução e voltam na próxima abertura; as setas continuam navegando.
- **busca com regex e substituição** — *feito*. O modal de busca ganhou **Aa** (diferenciar
  maiúsculas) e **.*** (expressão regular) no rodapé, e um botão **substituir…** que aplica
  a busca em todos os arquivos do projeto, com diálogo para o texto novo (vazio apaga). A
  busca literal é escapada (digitar `(` procura o parêntese) e a troca literal mantém `$1`
  como texto; com regex, grupos de captura funcionam. Abas abertas sem alterações pendentes
  são relidas do disco depois da troca.

## Como conferir

- `Ctrl+Shift+P` → *novo arquivo em branco…* → digitar texto → `Ctrl+S` (o diálogo sugere
  `raiz/nome`) → a aba passa a mostrar o caminho na barra de status.
- Explorador: passar o mouse numa pasta (**›_**), abrir o menu **⋯** duas vezes, e o clique
  direito num item para ver **abrir terminal aqui**, **copiar nome** e **copiar caminho**.
- Terminal: `dir` numa pasta com acentos no nome, um comando que imprime acentos
  (`echo configuração não é opcional`) e uma ferramenta com barra de progresso; depois
  **⛶** e `Esc`.
- Editor: alternar entre temas claros e escuros e conferir as barras de rolagem.
- Terminal: `ping -n 60 127.0.0.1` e **■ parar** (ou `Ctrl+C`); `cargo build` interrompido no
  meio para ver que o `rustc` não fica rodando no Gerenciador de Tarefas.
- Abas: `Ctrl+W` num arquivo com alterações e salvas, `Ctrl+Shift+T` para reabrir; conferir
  cursor e rolagem. Fazer o mesmo com uma aba em branco.
- `Ctrl+Shift+O` num `.ts` e num `.md`; clicar nos trechos do breadcrumb para ver o
  explorador abrir a pasta; `Alt+↓` e `Shift+Alt+↓`.
- Paleta → *abrir settings.json*, mudar `"fontSize"`, `Ctrl+S` e ver a fonte do editor mudar;
  depois quebrar o JSON de propósito e conferir a recusa.
- Git: abrir um repositório e conferir o **⑂** na barra, os marcadores no explorador e um
  commit pelo painel; tentar commitar sem alterações para ver a saída do git.
- Terminal: fechar e reabrir o editor e apertar `↑` para ver o comando anterior.
- Busca: `Ctrl+Shift+F`, ligar **.***, procurar `fn\s+\w+`, depois **substituir…** com
  `fn $1`; conferir o aviso na barra e as abas relidas.
- Salvar como: escolher um nome que já existe e ver o diálogo de substituir.

## Pendências conhecidas

- O terminal é texto puro (sem cores) e não tem TTY: programas que exigem um terminal real
  continuam fora do escopo.
- A saída depende de UTF-8/Latin-1; quem trocar a página de código na mão (`chcp 850`) pode
  ver alguns acentos diferentes.
- Arquivo criado em branco e salvo fora da raiz do projeto fica editável, mas não entra na
  árvore, no `Ctrl+P` nem no recarregamento automático.
- **Ir para símbolo** depende do formato da linha; declarações multi-linha ou geradas podem
  não aparecer (não é um servidor de linguagem).
- **Salvar como** sobrescreve um arquivo existente sem pedir confirmação.
- A aba de um arquivo aberto fora da raiz não é reaberta por `Ctrl+Shift+T` (a leitura é
  restrita ao projeto).
- O git depende do `git.exe` no PATH e só cobre status e commit de tudo: sem staging por
  arquivo, diff embutido, troca de branch ou `push`/`pull`.
- A substituição em arquivos respeita os limites do índice: pastas ignoradas, binários e
  arquivos acima de 1 MB não são tocados.
- O histórico do terminal é único para a máquina, não por projeto.
