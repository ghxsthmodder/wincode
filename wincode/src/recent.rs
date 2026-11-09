use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Quantos projetos recentes a tela inicial guarda.
const LIMIT: usize = 10;

/// Projetos e arquivos abertos recentemente, gravados em
/// `wincodeconfig/recent.json` para aparecerem na tela inicial.
pub(super) struct Recent {
    path: Option<PathBuf>,
    items: Mutex<Vec<Entry>>,
}

#[derive(Clone)]
struct Entry {
    path: String,
    kind: String,
}

impl Recent {
    pub(super) fn new() -> Self {
        let path = crate::config::dir().join("recent.json");
        let items = load(&path);
        Self {
            path: Some(path),
            items: Mutex::new(items),
        }
    }

    /// Lista do mais recente para o mais antigo, sem os caminhos que sumiram.
    pub(super) fn list(&self) -> serde_json::Value {
        let mut items = self.items.lock().unwrap();

        // Pasta/arquivo removido do disco sai da lista (e do arquivo) de vez.
        let before = items.len();
        items.retain(|entry| Path::new(&entry.path).exists());
        if items.len() != before {
            self.persist(items.as_slice());
        }

        let list: Vec<serde_json::Value> = items
            .iter()
            .filter(|entry| Path::new(&entry.path).exists())
            .map(|entry| {
                serde_json::json!({
                    "path": entry.path,
                    "name": name_of(&entry.path),
                    "kind": entry.kind,
                    "dir": entry.kind == "dir",
                })
            })
            .collect();

        serde_json::Value::Array(list)
    }

    /// Coloca o item no topo da lista (sem duplicar, sem diferenciar
    /// maiúsculas/minúsculas no Windows) e grava.
    pub(super) fn touch(&self, path: &Path, kind: &str) {
        let value = path.to_string_lossy().to_string();
        if value.is_empty() {
            return;
        }

        let mut items = self.items.lock().unwrap();
        items.retain(|entry| !same_path(&entry.path, &value));
        items.insert(
            0,
            Entry {
                path: value,
                kind: if kind == "file" { "file".into() } else { "dir".into() },
            },
        );
        items.truncate(LIMIT);

        self.persist(&items);
    }

    pub(super) fn remove(&self, path: &str) {
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        items.retain(|entry| !same_path(&entry.path, path));
        if items.len() != before {
            self.persist(&items);
        }
    }

    fn persist(&self, items: &[Entry]) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }

        let value = serde_json::json!({
            "version": 1,
            "items": items
                .iter()
                .map(|entry| serde_json::json!({ "path": entry.path, "kind": entry.kind }))
                .collect::<Vec<_>>(),
        });

        let Ok(text) = serde_json::to_string_pretty(&value) else {
            return;
        };
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

fn name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string())
}

fn same_path(a: &str, b: &str) -> bool {
    a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"))
}

fn load(path: &Path) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };

    let list = value
        .get("items")
        .and_then(|items| items.as_array())
        .cloned()
        .unwrap_or_default();

    let mut entries: Vec<Entry> = Vec::new();
    for item in list {
        let Some(path) = item.get("path").and_then(|path| path.as_str()) else {
            continue;
        };
        if path.trim().is_empty() {
            continue;
        }
        let kind = match item.get("kind").and_then(|kind| kind.as_str()) {
            Some("file") => "file",
            _ => "dir",
        };
        if entries.iter().any(|entry| same_path(&entry.path, path)) {
            continue;
        }
        entries.push(Entry {
            path: path.to_string(),
            kind: kind.into(),
        });
        if entries.len() >= LIMIT {
            break;
        }
    }

    entries
}
