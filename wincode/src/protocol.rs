use crate::assets::Assets;
use crate::fsapi::FsState;
use crate::jobs::Jobs;
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
    request: &Request<Vec<u8>>,
) -> Result<Response<Body>> {
    let uri = request.uri();
    let path = uri.path().to_string();
    let query = uri.query();

    if let Some(route) = path.strip_prefix("/api/") {
        return Ok(api(route, query, request.body(), state, jobs, settings));
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
) -> Response<Body> {
    match route {
        "root" => {
            let guard = state.lock().unwrap();
            json(serde_json::json!({
                "root": guard.root.to_string_lossy(),
                "cwd": guard.cwd.to_string_lossy(),
                "selectionRequired": !guard.selected,
            }))
        }
        "pickfolder" => pick_folder(state),
        "pickfile" => pick_file(state),
        "ls" => list(param(query, "path"), state),
        "read" => read(param(query, "path"), state),
        "write" => write(param(query, "path"), body, state),
        "create" => create(param(query, "path"), param(query, "kind"), state),
        "rename" => rename(param(query, "from"), param(query, "to"), state),
        "delete" => delete_entry(param(query, "path"), state),
        "cd" => cd(param(query, "path"), state),
        "exec" => exec(param(query, "cmd").unwrap_or_default(), state, jobs),
        "poll" => poll(param(query, "id"), jobs),
        "settings" => settings_get(settings),
        "savesettings" => settings_save(body, settings),
        _ => plain(
            StatusCode::NOT_FOUND,
            &format!("rota desconhecida: /api/{route}"),
        ),
    }
}

fn pick_folder(state: &Arc<Mutex<FsState>>) -> Response<Body> {
    match crate::dialog::pick_folder() {
        Ok(Some(dir)) => {
            let mut guard = state.lock().unwrap();
            guard.set_root(dir.clone());
            json(serde_json::json!({
                "root": dir.to_string_lossy(),
                "cwd": dir.to_string_lossy(),
                "selectionRequired": false,
            }))
        }
        Ok(None) => json(serde_json::json!({ "canceled": true })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

fn pick_file(state: &Arc<Mutex<FsState>>) -> Response<Body> {
    match crate::dialog::pick_file() {
        Ok(Some(file)) => {
            let Some(dir) = file.parent().filter(|dir| !dir.as_os_str().is_empty()) else {
                return plain(StatusCode::BAD_REQUEST, "o arquivo não está em uma pasta");
            };
            let mut guard = state.lock().unwrap();
            guard.set_root(dir.to_path_buf());
            json(serde_json::json!({
                "root": dir.to_string_lossy(),
                "cwd": dir.to_string_lossy(),
                "file": file.to_string_lossy(),
                "selectionRequired": false,
            }))
        }
        Ok(None) => json(serde_json::json!({ "canceled": true })),
        Err(err) => plain(StatusCode::BAD_REQUEST, &err),
    }
}

fn list(path_param: Option<String>, state: &Arc<Mutex<FsState>>) -> Response<Body> {
    let guard = state.lock().unwrap();
    let requested = path_param.unwrap_or_else(|| guard.cwd.to_string_lossy().to_string());

    let dir = match guard.resolve(&requested) {
        Ok(dir) => dir,
        Err(err) => return plain(StatusCode::BAD_REQUEST, &err),
    };
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
