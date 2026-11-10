use crate::assets::Assets;
use crate::fonts::Fonts;
use crate::fsapi::FsState;
use crate::history::History;
use crate::jobs::Jobs;
use crate::recent::Recent;
use crate::session::Sessions;
use crate::settings::Settings;
use wry::http::{header::CONTENT_TYPE, Request, Response, StatusCode};
use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use wry::Result;

type Body = Cow<'static, [u8]>;

/// Handler do protocolo customizado `editor://`, usado tanto para o front-end
/// (HTML + Monaco embutidos) quanto para a API de arquivos/terminal.
pub(super) fn serve(
    assets: &Assets,
    state: &Arc<Mutex<FsState>>,
    jobs: &Arc<Jobs>,
    settings: &Arc<Settings>,
    fonts: &Arc<Fonts>,
    recent: &Arc<Recent>,
    sessions: &Arc<Sessions>,
    history: &Arc<History>,
    request: &Request<Vec<u8>>,
) -> Result<Response<Body>> {
    let uri = request.uri();
    let path = uri.path().to_string();
    let query = uri.query();

    if let Some(route) = path.strip_prefix("/api/") {
        return Ok(api(
            route,
            query,
            request.body(),
            state,
            jobs,
            settings,
            fonts,
            recent,
            sessions,
            history,
        ));
    }

    // Fontes do usuário (`wincodeconfig/fonts`): servidas só dessa pasta.
    if let Some(name) = path.strip_prefix("/fonts/") {
        return Ok(match fonts.read(name) {
            Some(bytes) => Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, mime_for(name))
                .header("Cache-Control", "public, max-age=3600")
                .body(Cow::Owned(bytes))
                .unwrap_or_else(|_| {
                    plain(StatusCode::INTERNAL_SERVER_ERROR, "erro ao montar resposta")
                }),
            None => plain(StatusCode::NOT_FOUND, "fonte não encontrada"),
        });
    }

    let rel = if path == "/" { "index.html" } else { path.as_str() };

    Ok(match assets.get(rel) {
        Some(body) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, mime_for(rel))
            .body(body)
            .unwrap_or_else(|_| plain(StatusCode::INTERNAL_SERVER_ERROR, "erro ao montar resposta")),
        None => plain(
            StatusCode::NOT_FOUND,
            &format!("asset não encontrado: {rel}"),
        ),
    })
}

fn api(
    route: &str,
    query: Option<&str>,
    body: &[u8],
    state: &Arc<Mutex<FsState>>,
    jobs: &Arc<Jobs>,
    settings: &Arc<Settings>,
    fonts: &Arc<Fonts>,
    recent: &Arc<Recent>,
    sessions: &Arc<Sessions>,
    history: &Arc<History>,
) -> Response<Body> {
    match route {
        "root" => {
            let guard = state.lock().unwrap();
            json(workspace_json(&guard))
        }
        "pickfolder" => pick_folder(state, recent),
        "pickfile" => pick_file(state, recent),
        "open" => open_entry(param(query, "path"), state, recent),
        "recent" => json(recent.list()),
        "recentremove" => {
            recent.remove(&param(query, "path").unwrap_or_default());
            json(serde_json::json!({ "ok": true }))
        }
        "fonts" => json(fonts.list()),
        "index" => index_files(state),
        "search" => search_files(param(query, "q"), search_options(query), state),
        "replace" => replace_files(
            param(query, "q"),
            param(query, "replace"),
            search_options(query),
            state,
        ),
        "pathexists" => path_exists(param(query, "path"), state),
        "undoreplace" => undo_replace(state),
        "gitstatus" => git_status(state),
        "gitcommit" => git_commit(body, state),
        "gitstage" => git_stage(body, state),
        "gitdiff" => git_diff(
            param(query, "path"),
            param(query, "staged"),
            state,
        ),
        "gitbranches" => git_branches(state),
        "gitcheckout" => git_checkout(body, state),
        "gitsync" => git_sync(param(query, "action"), state, jobs),
        "history" => json(serde_json::Value::from(
            history.load(&param(query, "root").unwrap_or_default()),
        )),
        "savehistory" => save_history(body, history),
        "session" => json(sessions.load(&param(query, "root").unwrap_or_default())),
        "savesession" => match sessions.save(&param(query, "root").unwrap_or_default(), body) {
            Ok(()) => json(serde_json::json!({ "ok": true })),
            Err(err) => plain(StatusCode::BAD_REQUEST, &err),
        },
        "unpin" => {
            let payload = {
                let mut guard = state.lock().unwrap();
                guard.unpin_file();
                workspace_json(&guard)
            };
            json(payload)
        }
        "ls" => list(param(query, "path"), param(query, "scope"), state),
        "read" => read(param(query, "path"), state),
        "write" => write(param(query, "path"), body, state),
        "saveas" => save_as(param(query, "path"), body, state),
        "create" => create(param(query, "path"), param(query, "kind"), state),
        "rename" => rename(param(query, "from"), param(query, "to"), state),
        "delete" => delete_entry(param(query, "path"), state),
        "cd" => cd(param(query, "path"), state),
        "exec" => exec(param(query, "cmd").unwrap_or_default(), state, jobs),
        "poll" => poll(param(query, "id"), jobs),
        "termkill" => term_kill(param(query, "id"), jobs),
        "watch" => watch(body, state),
        "settings" => settings_get(settings),
        "configfile" => config_file(settings),
        "savesettings" => settings_save(body, settings),
        _ => plain(
            StatusCode::NOT_FOUND,
            &format!("rota desconhecida: /api/{route}"),
        ),
    }
}

/// Estado da área de trabalho como o front-end o conhece. `file` só existe
/// quando um arquivo foi aberto sozinho (modo arquivo único).
fn workspace_json(guard: &FsState) -> serde_json::Value {
    serde_json::json!({
        "root": guard.root.to_string_lossy(),
        "cwd": guard.cwd.to_string_lossy(),
        "file": guard.open_file.as_ref().map(|file| file.to_string_lossy().to_string()),
        "singleFile": guard.open_file.is_some(),
        "selectionRequired": !guard.selected,
    })
}

/// Índice do projeto para a abertura rápida (Ctrl+P). Sem projeto, lista vazia.
fn index_files(state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let root = {
        let guard = state.lock().unwrap();
        if !guard.selected {
            return json(serde_json::json!({ "files": [] }));
        }
        guard.root.clone()
    };

    match std::fs::canonicalize(&root) {
        Ok(root) => json(serde_json::json!({ "files": crate::index::list(&root) })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &format!("pasta inválida: {err}")),
    }
}

/// Opções de busca vindas da interface: `regex=1` e `case=1` (diferenciar
/// maiúsculas). Ausentes, valem busca literal sem diferenciar caixa.
fn search_options(query: Option<&str>) -> crate::search::Options {
    let flag = |key: &str| {
        param(query, key)
            .map(|value| value == "1" || value == "true")
            .unwrap_or(false)
    };

    crate::search::Options::new(flag("regex"), flag("case"))
}

/// Busca textual no projeto (Ctrl+Shift+F). Uma letra custaria a varredura toda,
/// então só a partir de dois caracteres.
fn search_files(
    query: Option<String>,
    options: crate::search::Options,
    state: &Arc<Mutex<FsState>>,
) -> Response<Body> {
    let query = query.unwrap_or_default();
    if query.chars().count() > 500 {
        return plain(StatusCode::BAD_REQUEST, "busca longa demais");
    }
    if query.trim().chars().count() < 2 {
        return json(serde_json::json!({ "files": [], "truncated": false }));
    }

    let root = match project_root(state) {
        Some(root) => root,
        None => return json(serde_json::json!({ "files": [], "truncated": false })),
    };

    match crate::search::run(&root, &query, &options) {
        Ok(value) => json(value),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// Substituir em todos os arquivos do projeto (o "substituir tudo" da busca).
fn replace_files(
    query: Option<String>,
    replacement: Option<String>,
    options: crate::search::Options,
    state: &Arc<Mutex<FsState>>,
) -> Response<Body> {
    let query = query.unwrap_or_default();
    if query.trim().is_empty() {
        return plain(StatusCode::BAD_REQUEST, "digite o que procurar");
    }
    if query.chars().count() > 500 {
        return plain(StatusCode::BAD_REQUEST, "busca longa demais");
    }

    let Some(root) = project_root(state) else {
        return plain(StatusCode::BAD_REQUEST, "nenhum projeto aberto");
    };

    match crate::search::replace(&root, &query, &replacement.unwrap_or_default(), &options) {
        Ok(value) => json(value),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// Raiz canônica do projeto, ou `None` quando nenhuma pasta foi escolhida.
fn project_root(state: &Arc<Mutex<FsState>>) -> Option<std::path::PathBuf> {
    let root = {
        let guard = state.lock().unwrap();
        if !guard.selected {
            return None;
        }
        guard.root.clone()
    };

    std::fs::canonicalize(root).ok()
}

fn pick_folder(state: &Arc<Mutex<FsState>>, recent: &Arc<Recent>) -> Response<Body> {
    match crate::dialog::pick_folder() {
        Ok(Some(dir)) => {
            let payload = {
                let mut guard = state.lock().unwrap();
                guard.set_root(dir.clone());
                workspace_json(&guard)
            };
            recent.touch(&dir, "dir");
            json(payload)
        }
        Ok(None) => json(serde_json::json!({ "canceled": true })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

fn pick_file(state: &Arc<Mutex<FsState>>, recent: &Arc<Recent>) -> Response<Body> {
    match crate::dialog::pick_file() {
        Ok(Some(file)) => {
            let payload = {
                let mut guard = state.lock().unwrap();
                if let Err(err) = guard.set_file(file.clone()) {
                    return plain(StatusCode::BAD_REQUEST, &err);
                }
                workspace_json(&guard)
            };
            recent.touch(&file, "file");
            json(payload)
        }
        Ok(None) => json(serde_json::json!({ "canceled": true })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// Abre uma pasta ou um arquivo pelo caminho (usado pela lista de recentes).
fn open_entry(
    path_param: Option<String>,
    state: &Arc<Mutex<FsState>>,
    recent: &Arc<Recent>,
) -> Response<Body> {
    let Some(requested) = path_param else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };

    let path = std::path::PathBuf::from(&requested);
    let kind = if path.is_dir() {
        "dir"
    } else if path.is_file() {
        "file"
    } else {
        return plain(StatusCode::NOT_FOUND, &format!("caminho não encontrado: {requested}"));
    };

    let payload = {
        let mut guard = state.lock().unwrap();
        if kind == "dir" {
            guard.set_root(path.clone());
        } else if let Err(err) = guard.set_file(path.clone()) {
            return plain(StatusCode::BAD_REQUEST, &err);
        }
        workspace_json(&guard)
    };

    recent.touch(&path, kind);
    json(payload)
}

fn list(
    path_param: Option<String>,
    scope: Option<String>,
    state: &Arc<Mutex<FsState>>,
) -> Response<Body> {
    let guard = state.lock().unwrap();
    let requested = path_param.unwrap_or_else(|| guard.cwd.to_string_lossy().to_string());

    let dir = match guard.resolve(&requested) {
        Ok(dir) => dir,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };

    // Modo arquivo único: o explorador lista só o arquivo aberto. O terminal
    // (`scope` ausente) continua enxergando a pasta inteira.
    if scope.as_deref() == Some("tree") {
        if let Some(file) = guard.pinned_file_in(&dir) {
            let name = file
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            return json(serde_json::json!([{
                "name": name,
                "path": file.to_string_lossy(),
                "dir": false,
            }]));
        }
    }

    drop(guard);

    let read_dir = match std::fs::read_dir(&dir) {
        Ok(read_dir) => read_dir,
        Err(err) => {
            return plain(
                StatusCode::BAD_REQUEST,
                &format!("não foi possível ler a pasta: {err}"),
            )
        }
    };

    let mut entries: Vec<(bool, String, String)> = Vec::new();
    for entry in read_dir.flatten() {
        let full = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        entries.push((full.is_dir(), name, full.to_string_lossy().to_string()));
    }

    // Pastas primeiro, depois ordem alfabética.
    entries.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });

    let items: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|(is_dir, name, path)| {
            serde_json::json!({ "name": name, "path": path, "dir": is_dir })
        })
        .collect();

    json(serde_json::Value::Array(items))
}

/// Assinaturas de pastas e arquivos, usadas pelo front-end para recarregar
/// automaticamente o que mudou fora do editor. Recebe um array JSON de
/// caminhos e devolve `{ caminho: assinatura }`.
fn watch(body: &[u8], state: &Arc<Mutex<FsState>>) -> Response<Body> {
    const MAX_PATHS: usize = 200;

    let requested: Vec<String> = match serde_json::from_slice(body) {
        Ok(list) => list,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &format!("lista inválida: {err}")),
    };
    if requested.len() > MAX_PATHS {
        return plain(StatusCode::BAD_REQUEST, "caminhos demais na checagem");
    }

    let guard = state.lock().unwrap();
    let mut out = serde_json::Map::with_capacity(requested.len());
    for item in requested {
        if let Ok(path) = guard.resolve(&item) {
            out.insert(item, serde_json::Value::String(signature(&path)));
        }
    }

    json(serde_json::Value::Object(out))
}

/// Hash estável e curto do conteúdo de uma pasta (nomes e tipos) ou da
/// impressão digital de um arquivo (tamanho e mtime).
fn signature(path: &std::path::Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => {
            let mut entries: Vec<String> = Vec::new();
            if let Ok(read_dir) = std::fs::read_dir(path) {
                for entry in read_dir.flatten() {
                    let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
                    entries.push(format!(
                        "{}{}",
                        entry.file_name().to_string_lossy(),
                        if is_dir { "/" } else { "" }
                    ));
                }
            }
            entries.sort();
            "d".hash(&mut hasher);
            entries.hash(&mut hasher);
        }
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|age| age.as_nanos())
                .unwrap_or(0);
            "f".hash(&mut hasher);
            meta.len().hash(&mut hasher);
            modified.hash(&mut hasher);
        }
        Err(_) => return String::new(),
    }

    format!("{:016x}", hasher.finish())
}

fn read(path_param: Option<String>, state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let Some(requested) = path_param else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };

    let target = match state.lock().unwrap().resolve(&requested) {
        Ok(target) => target,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };

    match std::fs::read(&target) {
        Ok(bytes) if bytes.len() > 8 * 1024 * 1024 => plain(
            StatusCode::BAD_REQUEST,
            "arquivo grande demais para o editor (limite de 8 MiB)",
        ),
        Ok(bytes) => respond(StatusCode::OK, "text/plain; charset=utf-8", bytes),
        Err(err) => plain(
            StatusCode::BAD_REQUEST,
            &format!("não foi possível ler o arquivo: {err}"),
        ),
    }
}

fn write(path_param: Option<String>, body: &[u8], state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let Some(requested) = path_param else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };

    let target = match state.lock().unwrap().resolve(&requested) {
        Ok(target) => target,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };

    match std::fs::write(&target, body) {
        Ok(()) => json(serde_json::json!({
            "ok": true,
            "path": target.to_string_lossy(),
        })),
        Err(err) => plain(
            StatusCode::BAD_REQUEST,
            &format!("não foi possível salvar: {err}"),
        ),
    }
}

/// Grava um arquivo cujo caminho foi escolhido agora (aba criada em branco).
///
/// Um caminho relativo continua restrito à raiz do projeto; um caminho completo
/// é respeitado porque foi digitado pelo usuário no diálogo de salvamento — o
/// arquivo novo pode nascer fora do projeto aberto.
fn save_as(path_param: Option<String>, body: &[u8], state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let Some(requested) = path_param.map(|value| value.trim().to_string()) else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };
    if requested.is_empty() {
        return plain(StatusCode::BAD_REQUEST, "informe o caminho do arquivo");
    }

    let target = if std::path::Path::new(&requested).is_absolute() {
        std::path::absolute(&requested).unwrap_or_else(|_| std::path::PathBuf::from(&requested))
    } else {
        match state.lock().unwrap().resolve(&requested) {
            Ok(target) => target,
            Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
        }
    };

    if target.is_dir() {
        return plain(StatusCode::BAD_REQUEST, "o caminho é uma pasta");
    }
    if !target.parent().is_some_and(|parent| parent.is_dir()) {
        return plain(StatusCode::BAD_REQUEST, "a pasta de destino não existe");
    }

    match std::fs::write(&target, body) {
        Ok(()) => json(serde_json::json!({
            "ok": true,
            "path": target.to_string_lossy(),
        })),
        Err(err) => plain(
            StatusCode::BAD_REQUEST,
            &format!("não foi possível salvar: {err}"),
        ),
    }
}

fn create(
    path_param: Option<String>,
    kind: Option<String>,
    state: &Arc<Mutex<FsState>>,
) -> Response<Body> {
    let Some(requested) = path_param else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };

    let is_dir = kind.as_deref() == Some("dir");
    let target = {
        let guard = state.lock().unwrap();
        match guard.resolve_new(&requested) {
            Ok(target) => target,
            Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
        }
    };

    let result = if is_dir {
        std::fs::create_dir(&target)
    } else {
        // `create_new` evita sobrescrever um arquivo que apareceu nesse meio tempo.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map(|_| ())
    };

    match result {
        Ok(()) => json(serde_json::json!({
            "path": target.to_string_lossy(),
            "dir": is_dir,
        })),
        Err(err) => plain(
            StatusCode::BAD_REQUEST,
            &format!("não foi possível criar: {err}"),
        ),
    }
}

fn rename(
    from_param: Option<String>,
    to_param: Option<String>,
    state: &Arc<Mutex<FsState>>,
) -> Response<Body> {
    let (Some(from), Some(to)) = (from_param, to_param) else {
        return plain(StatusCode::BAD_REQUEST, "parâmetros from e to são obrigatórios");
    };

    let mut guard = state.lock().unwrap();

    let root = match guard.canonical_root() {
        Ok(root) => root,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };
    let source = match guard.resolve(&from) {
        Ok(source) => source,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };

    if source == root {
        return plain(
            StatusCode::BAD_REQUEST,
            "não é possível renomear a pasta do projeto",
        );
    }
    if !source.exists() {
        return plain(StatusCode::NOT_FOUND, "item não encontrado");
    }

    let target = match guard.resolve_new(&to) {
        Ok(target) => target,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };

    if let Err(err) = std::fs::rename(&source, &target) {
        return plain(
            StatusCode::BAD_REQUEST,
            &format!("não foi possível renomear: {err}"),
        );
    }

    guard.remap_cwd(&source, &target);
    guard.remap_file(&source, &target);

    json(serde_json::json!({ "path": target.to_string_lossy() }))
}

fn delete_entry(path_param: Option<String>, state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let Some(requested) = path_param else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };

    let mut guard = state.lock().unwrap();

    let root = match guard.canonical_root() {
        Ok(root) => root,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };
    let target = match guard.resolve(&requested) {
        Ok(target) => target,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };

    if target == root {
        return plain(
            StatusCode::BAD_REQUEST,
            "não é possível excluir a pasta do projeto",
        );
    }
    if !target.exists() {
        return plain(StatusCode::NOT_FOUND, "item não encontrado");
    }

    let result = if target.is_dir() {
        std::fs::remove_dir_all(&target)
    } else {
        std::fs::remove_file(&target)
    };

    match result {
        Ok(()) => {
            guard.reset_cwd_if_inside(&target);
            guard.clear_file_if_inside(&target);
            json(serde_json::json!({
                "ok": true,
                "path": target.to_string_lossy(),
            }))
        }
        Err(err) => plain(
            StatusCode::BAD_REQUEST,
            &format!("não foi possível excluir: {err}"),
        ),
    }
}

fn cd(path_param: Option<String>, state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let Some(requested) = path_param else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };

    let mut guard = state.lock().unwrap();

    // `cd pasta` é relativo ao diretório atual do terminal, não à raiz do projeto.
    let target = if std::path::Path::new(&requested).is_absolute() {
        requested.clone()
    } else {
        guard.cwd.join(&requested).to_string_lossy().to_string()
    };

    match guard.resolve(&target) {
        Ok(dir) if dir.is_dir() => {
            guard.cwd = dir.clone();
            json(serde_json::json!({ "cwd": dir.to_string_lossy() }))
        }
        Ok(_) => plain(StatusCode::BAD_REQUEST, "o caminho não é uma pasta"),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

fn exec(cmd: String, state: &Arc<Mutex<FsState>>, jobs: &Arc<Jobs>) -> Response<Body> {
    let cwd = state.lock().unwrap().cwd.clone();

    match jobs.spawn(&cmd, &cwd) {
        Ok(id) => json(serde_json::json!({ "id": id })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

fn poll(id: Option<String>, jobs: &Arc<Jobs>) -> Response<Body> {
    let Some(id) = id.and_then(|value| value.parse::<u64>().ok()) else {
        return plain(StatusCode::BAD_REQUEST, "id inválido");
    };

    match jobs.poll(id) {
        Ok((out, code)) => json(serde_json::json!({
            "out": out,
            "done": code.is_some(),
            "code": code,
        })),
        Err(err) => plain(StatusCode::NOT_FOUND, &err),
    }
}

/// O destino de um "salvar como" já existe? A interface pergunta antes de
/// sobrescrever (o `/api/saveas` grava por cima sem perguntar).
fn path_exists(path_param: Option<String>, state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let Some(requested) = path_param.map(|value| value.trim().to_string()) else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };
    if requested.is_empty() {
        return plain(StatusCode::BAD_REQUEST, "informe o caminho do arquivo");
    }

    let target = if std::path::Path::new(&requested).is_absolute() {
        std::path::absolute(&requested).unwrap_or_else(|_| std::path::PathBuf::from(&requested))
    } else {
        match state.lock().unwrap().resolve(&requested) {
            Ok(target) => target,
            Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
        }
    };

    json(serde_json::json!({
        "exists": target.exists(),
        "dir": target.is_dir(),
    }))
}

/// Situação do git na raiz aberta; `repo: false` quando não é repositório (ou
/// quando o git não está instalado).
fn git_status(state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let (selected, root) = {
        let guard = state.lock().unwrap();
        (guard.selected, guard.root.clone())
    };

    // Sem projeto escolhido não há repositório: a pasta do próprio .exe não conta.
    if !selected {
        return json(serde_json::json!({ "repo": false }));
    }

    json(crate::git::status(&root))
}

/// Commit. Com arquivos preparados à mão, só eles entram; sem nada preparado, é
/// o `git add -A` + `git commit -m` de sempre.
fn git_commit(body: &[u8], state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let (selected, root) = {
        let guard = state.lock().unwrap();
        (guard.selected, guard.root.clone())
    };

    if !selected {
        return plain(StatusCode::BAD_REQUEST, "nenhum projeto aberto");
    }

    // Compatível com o formato anterior (a mensagem crua no corpo).
    let parsed: serde_json::Value = serde_json::from_slice(body)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(body).to_string()));
    let (message, stage_all) = match &parsed {
        serde_json::Value::String(message) => (message.clone(), true),
        other => (
            other
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            other.get("all").and_then(|value| value.as_bool()).unwrap_or(true),
        ),
    };

    match crate::git::commit(&root, &message, stage_all) {
        Ok(value) => json(value),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// Prepara/desprepara arquivos no índice (`{ paths: [...], stage: true }`).
fn git_stage(body: &[u8], state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let (selected, root) = {
        let guard = state.lock().unwrap();
        (guard.selected, guard.root.clone())
    };
    if !selected {
        return plain(StatusCode::BAD_REQUEST, "nenhum projeto aberto");
    }

    let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(body) else {
        return plain(StatusCode::BAD_REQUEST, "corpo inválido: esperado JSON");
    };

    let paths: Vec<String> = parsed
        .get("paths")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let stage = parsed
        .get("stage")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    match crate::git::stage(&root, &paths, stage) {
        Ok(()) => json(serde_json::json!({ "ok": true })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// Diff de um arquivo para o painel (`staged=1` mostra o que já está preparado).
fn git_diff(
    path: Option<String>,
    staged: Option<String>,
    state: &Arc<Mutex<FsState>>,
) -> Response<Body> {
    let Some(path) = path else {
        return plain(StatusCode::BAD_REQUEST, "parâmetro path ausente");
    };

    let (selected, root) = {
        let guard = state.lock().unwrap();
        (guard.selected, guard.root.clone())
    };
    if !selected {
        return plain(StatusCode::BAD_REQUEST, "nenhum projeto aberto");
    }

    let staged = staged
        .map(|value| value == "1" || value == "true")
        .unwrap_or(false);

    match crate::git::diff(&root, &path, staged) {
        Ok(value) => json(value),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// Branches locais para o seletor do painel.
fn git_branches(state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let (selected, root) = {
        let guard = state.lock().unwrap();
        (guard.selected, guard.root.clone())
    };
    if !selected {
        return plain(StatusCode::BAD_REQUEST, "nenhum projeto aberto");
    }

    json(crate::git::branches(&root))
}

/// Troca de branch (`{ branch: "nome" }`).
fn git_checkout(body: &[u8], state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let (selected, root) = {
        let guard = state.lock().unwrap();
        (guard.selected, guard.root.clone())
    };
    if !selected {
        return plain(StatusCode::BAD_REQUEST, "nenhum projeto aberto");
    }

    let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(body) else {
        return plain(StatusCode::BAD_REQUEST, "corpo inválido: esperado JSON");
    };
    let branch = parsed
        .get("branch")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    match crate::git::checkout(&root, branch) {
        Ok(value) => json(value),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// `fetch`/`pull`/`push` como job de terminal: a resposta é o `id` para o poll.
/// A rede pode demorar, e o handler roda na thread da interface.
fn git_sync(
    action: Option<String>,
    state: &Arc<Mutex<FsState>>,
    jobs: &Arc<crate::jobs::Jobs>,
) -> Response<Body> {
    let (selected, root) = {
        let guard = state.lock().unwrap();
        (guard.selected, guard.root.clone())
    };
    if !selected {
        return plain(StatusCode::BAD_REQUEST, "nenhum projeto aberto");
    }

    // `--ff-only` evita um merge silencioso (e um editor aberto por ele): quando
    // o histórico divergiu, o git recusa e a mensagem explica o que fazer.
    let args: &[&str] = match action.as_deref() {
        Some("fetch") => &["fetch", "--all"],
        Some("pull") => &["pull", "--ff-only"],
        Some("push") => &["push"],
        _ => return plain(StatusCode::BAD_REQUEST, "ação inválida: fetch, pull ou push"),
    };

    match jobs.spawn_program("git", args, &root) {
        Ok(id) => json(serde_json::json!({ "id": id })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// Desfaz a última substituição em arquivos (uma vez).
fn undo_replace(state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let Some(root) = project_root(state) else {
        return plain(StatusCode::BAD_REQUEST, "nenhum projeto aberto");
    };

    match crate::search::undo(&root) {
        Ok(value) => json(value),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

fn save_history(body: &[u8], history: &Arc<History>) -> Response<Body> {
    match history.save(body) {
        Ok(entries) => json(serde_json::json!({ "ok": true, "count": entries.len() })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

fn term_kill(id: Option<String>, jobs: &Arc<Jobs>) -> Response<Body> {
    let Some(id) = id.and_then(|value| value.parse::<u64>().ok()) else {
        return plain(StatusCode::BAD_REQUEST, "id inválido");
    };

    match jobs.kill(id) {
        Ok(()) => json(serde_json::json!({ "ok": true })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

/// `settings.json` aberto como arquivo no editor: devolve o caminho real e o
/// texto do disco para a aba ser salva pelo mesmo `savesettings` (com validação).
fn config_file(settings: &Settings) -> Response<Body> {
    let Some(path) = settings.path() else {
        return plain(
            StatusCode::BAD_REQUEST,
            "não foi possível localizar a pasta de configuração do usuário",
        );
    };

    json(serde_json::json!({
        "path": path.to_string_lossy(),
        "text": settings.read_raw(),
    }))
}

fn settings_get(settings: &Settings) -> Response<Body> {
    match serde_json::from_str::<serde_json::Value>(&settings.load()) {
        Ok(value) => json(value),
        Err(_) => json(serde_json::json!({})),
    }
}

fn settings_save(body: &[u8], settings: &Settings) -> Response<Body> {
    match settings.save(body) {
        Ok(()) => json(serde_json::json!({ "ok": true })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

fn param(query: Option<&str>, key: &str) -> Option<String> {
    for pair in query?.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == key {
            // "+" representa espaço em query strings (URLSearchParams envia assim).
            let value = value.replace('+', " ");
            return Some(
                urlencoding::decode(&value)
                    .map(|decoded| decoded.into_owned())
                    .unwrap_or(value),
            );
        }
    }
    None
}

fn respond(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, mime)
        .body(Cow::Owned(body))
        .unwrap_or_else(|_| Response::new(Cow::Owned(Vec::new())))
}

fn plain(status: StatusCode, message: &str) -> Response<Body> {
    respond(status, "text/plain; charset=utf-8", message.as_bytes().to_vec())
}

fn json(value: serde_json::Value) -> Response<Body> {
    let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/json")
        .header("Cache-Control", "no-store")
        .body(Cow::Owned(body))
        .unwrap_or_else(|_| Response::new(Cow::Owned(Vec::new())))
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}
