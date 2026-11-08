# CodeBuff Editor

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

## Estrutura

```
codebuff-editor/
  Cargo.toml
  build.rs                  # gera a tabela de assets embutidos
  scripts/prepare-monaco.js # baixa o Monaco para assets/monaco/vs
  assets/
    index.html              # UI (árvore | Monaco | terminal)
    monaco/vs/              # Monaco 0.57 (gerado pelo script)
  src/
    main.rs                 # janela + WebView
    protocol.rs             # rotas /api/* e assets
    assets.rs               # assets embutidos (override opcional em disco)
    fsapi.rs                # raiz do projeto + resolução segura de caminhos
    jobs.rs                 # execução assíncrona dos comandos do terminal
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

Saída: `target\x86_64-pc-windows-msvc\release\codebuff-editor.exe`.

`Cargo.toml` usa `+crt-static`, então o executável não depende do runtime do Visual C++.
O WebView2 é o único componente do sistema (já vem no Windows 10/11; há o
[Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) para versões antigas).

## Uso

```powershell
codebuff-editor.exe            # abre o diretório atual
codebuff-editor.exe C:\projetos\meu-app
```

Também é possível definir a pasta por variável de ambiente:

```powershell
set CODEBUFF_EDITOR_ROOT=C:\projetos\meu-app
```

O botão **abrir pasta** na barra superior abre o diálogo nativo do Windows
(`IFileOpenDialog` via crate `rfd`) e troca a raiz do projeto sem reiniciar o editor.

Atalhos e comandos:

- `Ctrl+S` — salva o arquivo aberto
- `Ctrl+Shift+P` — não implementado ainda
- Terminal: qualquer comando (`dir`, `git status`, `cargo build`…) executa via `cmd.exe /C`
  no diretório atual do terminal, com saída em streaming
- Comandos internos do terminal: `cd <pasta>`, `pwd`, `ls`/`dir`, `clear`/`cls`, `open <arquivo>`

## Desenvolvimento do front-end sem recompilar

Aponte `CODEBUFF_EDITOR_ASSET_DIR` para a pasta `assets/` e os arquivos em disco passam
a ter prioridade sobre os embutidos:

```powershell
set CODEBUFF_EDITOR_ASSET_DIR=%CD%\assets
cargo run
```

## Limitações atuais

- A troca de pasta recomeça a sessão: arquivos abertos e modelos do Monaco são descartados
  (alterações não salvas são perdidas — salve antes de trocar).
- Terminal não é interativo (sem stdin/TTY): comandos que pedem entrada ficam bloqueados.
- Alvos não-Windows compilam apenas como conveniência de desenvolvimento e exigem
  `pkg-config`, `libgtk-3-dev` e `libwebkit2gtk-4.1-dev`.
