use std::path::PathBuf;
use std::sync::Mutex;

/// Quantos comandos ficam guardados por projeto. O bastante para uma sessão de
/// trabalho e para lembrar de um comando raro semanas depois.
const MAX_ENTRIES: usize = 500;
/// Comando gigante (um `curl` colando script) não entra inteiro.
const MAX_ENTRY_CHARS: usize = 4_000;
/// Teto do arquivo aceito no POST (o front manda a lista completa).
const MAX_BYTES: usize = 256 * 1024;
/// Projetos guardados. Cada um tem a sua lista; mais que isso vira arquivo grande
/// por projetos que ninguém abriu de novo.
const MAX_PROJECTS: usize = 20;

/// O que fica no disco: uma lista compartilhada (comandos usados sem projeto) e
/// uma lista por pasta de projeto.
#[derive(Default)]
struct Store {
    shared: Vec<String>,
    /// Na ordem de uso: o último salvo é o último da lista (LRU de verdade).
    projects: Vec<(String, Vec<String>)>,
}

/// Histórico do terminal entre execuções, em `wincodeconfig/history.json`.
///
/// O terminal é por projeto, então o histórico também: quem passa o dia em dois
/// repositórios não quer a seta para cima alternando entre eles. Comandos dados
/// sem projeto aberto ficam na lista compartilhada, que também serve de semente
/// para um projeto novo.
pub(super) struct History {
    path: Option<PathBuf>,
    cached: Mutex<Option<Store>>,
}

impl History {
    pub(super) fn new() -> Self {
        Self {
            path: Some(crate::config::dir().join("history.json")),
            cached: Mutex::new(None),
        }
    }

    /// Lista do projeto; sem projeto (ou projeto novo) devolve a compartilhada.
    pub(super) fn load(&self, root: &str) -> Vec<String> {
        let mut cached = self.cached.lock().unwrap();
        if cached.is_none() {
            *cached = Some(self.read_from_disk());
        }

        let store = cached.as_ref().expect("cache preenchido acima");
        let key = project_key(root);
        if !key.is_empty() {
            if let Some((_, entries)) = store.projects.iter().find(|(name, _)| *name == key) {
                return entries.clone();
            }
        }
        store.shared.clone()
    }

    /// Substitui a lista do projeto pelo que o front enviou, já saneada.
    pub(super) fn save(&self, body: &[u8]) -> Result<Vec<String>, String> {
        if body.len() > MAX_BYTES {
            return Err(format!(
                "histórico muito grande (limite de {} KiB)",
                MAX_BYTES / 1024
            ));
        }

        let parsed: serde_json::Value =
            serde_json::from_slice(body).map_err(|err| format!("histórico inválido: {err}"))?;

        // Formato novo: `{ "root": "c:\\proj", "entries": [...] }`. Uma lista pura
        // ainda é aceita e vai para a compartilhada (compatível com o arquivo e
        // com qualquer chamada antiga).
        let (root, entries) = match &parsed {
            serde_json::Value::Array(_) => ("", &parsed),
            other => {
                let list = other
                    .get("entries")
                    .ok_or("histórico inválido: falta a lista `entries`")?;
                (
                    other.get("root").and_then(|v| v.as_str()).unwrap_or(""),
                    list,
                )
            }
        };

        let Some(items) = entries.as_array() else {
            return Err("histórico inválido: esperado uma lista de comandos".into());
        };
        let entries = sanitize(items.iter().filter_map(|item| item.as_str()));

        let mut cached = self.cached.lock().unwrap();
        if cached.is_none() {
            *cached = Some(self.read_from_disk());
        }
        let store = cached.as_mut().expect("cache preenchido acima");

        let key = project_key(root);
        if key.is_empty() {
            store.shared = entries.clone();
        } else {
            store.projects.retain(|(name, _)| *name != key);
            store.projects.push((key, entries.clone()));
            if store.projects.len() > MAX_PROJECTS {
                let excess = store.projects.len() - MAX_PROJECTS;
                store.projects.drain(..excess);
            }
        }

        self.write(store)?;
        Ok(entries)
    }

    fn write(&self, store: &Store) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|err| format!("não foi possível criar {}: {err}", dir.display()))?;
        }

        let projects: Vec<serde_json::Value> = store
            .projects
            .iter()
            .map(|(root, entries)| serde_json::json!({ "root": root, "entries": entries }))
            .collect();
        let value = serde_json::json!({
            "version": 2,
            "shared": store.shared,
            "projects": projects,
        });

        let text = serde_json::to_string_pretty(&value).map_err(|err| err.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)
            .map_err(|err| format!("não foi possível gravar {}: {err}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            format!("não foi possível salvar {}: {err}", path.display())
        })
    }

    fn read_from_disk(&self) -> Store {
        let Some(path) = &self.path else {
            return Store::default();
        };

        let Ok(text) = std::fs::read_to_string(path) else {
            return Store::default();
        };

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Store::default();
        };

        // Formato v1 (lista pura) e a lista compartilhada do v2.
        if let serde_json::Value::Array(items) = &value {
            return Store {
                shared: sanitize(items.iter().filter_map(|item| item.as_str())),
                projects: Vec::new(),
            };
        }

        let shared = value
            .get("shared")
            .and_then(|shared| shared.as_array())
            .map(|items| sanitize(items.iter().filter_map(|item| item.as_str())))
            .unwrap_or_default();

        let mut projects: Vec<(String, Vec<String>)> = Vec::new();
        if let Some(items) = value.get("projects").and_then(|v| v.as_array()) {
            for item in items {
                let Some(root) = item.get("root").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(entries) = item.get("entries").and_then(|v| v.as_array()) else {
                    continue;
                };
                let key = project_key(root);
                if key.is_empty() || projects.iter().any(|(name, _)| *name == key) {
                    continue;
                }
                let entries = sanitize(entries.iter().filter_map(|item| item.as_str()));
                if !entries.is_empty() {
                    projects.push((key, entries));
                }
            }
        }
        if projects.len() > MAX_PROJECTS {
            let excess = projects.len() - MAX_PROJECTS;
            projects.drain(..excess);
        }

        Store { shared, projects }
    }
}

/// Chave de comparação da raiz: `C:\Proj` e `c:/proj/` são o mesmo projeto.
fn project_key(root: &str) -> String {
    let trimmed = root.trim().trim_end_matches(['\\', '/']);
    trimmed.replace('/', "\\").to_lowercase()
}

/// Tira vazios e repetidos em sequência (a seta para cima pulando 10x o mesmo
/// comando é pior do que não ter histórico) e corta o que passou do teto.
fn sanitize<'a>(entries: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    for entry in entries {
        let trimmed = entry.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        if out.last().map(|last| last == trimmed).unwrap_or(false) {
            continue;
        }
        out.push(trimmed.chars().take(MAX_ENTRY_CHARS).collect());
    }

    let skip = out.len().saturating_sub(MAX_ENTRIES);
    out.into_iter().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::{project_key, sanitize, History};
    use serde_json::json;

    fn run(entries: &[&str]) -> Vec<String> {
        sanitize(entries.iter().copied())
    }

    fn history() -> History {
        History {
            path: None,
            cached: Default::default(),
        }
    }

    fn seed(store: &History, entries: Vec<String>) {
        *store.cached.lock().unwrap() = Some(super::Store {
            shared: entries,
            projects: Vec::new(),
        });
    }

    #[test]
    fn keeps_order_and_drops_empty_and_consecutive_duplicates() {
        assert_eq!(
            run(&["cargo build", "", "cargo build", "git status", "  "]),
            vec!["cargo build".to_string(), "git status".to_string()]
        );
    }

    #[test]
    fn non_consecutive_repeats_are_kept() {
        assert_eq!(
            run(&["dir", "cls", "dir"]),
            vec!["dir".to_string(), "cls".to_string(), "dir".to_string()]
        );
    }

    #[test]
    fn caps_the_list_at_the_last_entries() {
        let many: Vec<String> = (0..600).map(|index| format!("eco {index}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        let out = run(&refs);

        assert_eq!(out.len(), 500);
        assert_eq!(out.first().unwrap(), "eco 100");
        assert_eq!(out.last().unwrap(), "eco 599");
    }

    #[test]
    fn trims_long_commands() {
        let long = "x".repeat(5_000);
        let out = run(&[long.as_str()]);
        assert_eq!(out[0].chars().count(), 4_000);
    }

    #[test]
    fn project_key_ignores_case_and_slashes() {
        assert_eq!(project_key("C:\\Proj\\"), project_key("c:/proj"));
        assert_eq!(project_key("  "), "");
    }

    #[test]
    fn keeps_histories_apart_per_project() {
        let store = history();

        store
            .save(
                json!({ "root": "C:\\um", "entries": ["cargo run"] })
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        store
            .save(
                json!({ "root": "C:\\dois", "entries": ["npm test"] })
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();

        assert_eq!(store.load("c:\\um\\"), vec!["cargo run".to_string()]);
        assert_eq!(store.load("C:\\dois"), vec!["npm test".to_string()]);
        // Projeto desconhecido cai na lista compartilhada, ainda vazia.
        assert!(store.load("C:\\tres").is_empty());
    }

    #[test]
    fn shared_list_is_the_seed_for_a_new_project() {
        let store = history();
        store
            .save(json!({ "entries": ["clippy"] }).to_string().as_bytes())
            .unwrap();
        assert_eq!(store.load("C:\\novo"), vec!["clippy".to_string()]);
    }

    #[test]
    fn old_array_format_still_loads_as_shared() {
        let store = history();
        seed(&store, vec!["dir".to_string(), "cls".to_string()]);
        assert_eq!(store.load(""), vec!["dir".to_string(), "cls".to_string()]);
    }

    #[test]
    fn drops_the_least_recently_used_projects() {
        let store = history();
        for index in 0..(super::MAX_PROJECTS + 5) {
            let root = format!("C:\\p{index}");
            store
                .save(
                    json!({ "root": root, "entries": [format!("eco {index}")] })
                        .to_string()
                        .as_bytes(),
                )
                .unwrap();
        }

        // Os cinco primeiros saíram; o último continua lá.
        assert!(store.load("C:\\p0").is_empty());
        assert_eq!(
            store.load(&format!("C:\\p{}", super::MAX_PROJECTS + 4)),
            vec![format!("eco {}", super::MAX_PROJECTS + 4)]
        );
    }
}
