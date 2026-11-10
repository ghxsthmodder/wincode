use std::path::PathBuf;
use std::sync::Mutex;

/// Quantos comandos ficam guardados. O bastante para uma sessão de trabalho e
/// para lembrar de um comando raro semanas depois.
const MAX_ENTRIES: usize = 500;
/// Comando gigante (um `curl` colando script) não entra inteiro.
const MAX_ENTRY_CHARS: usize = 4_000;
/// Teto do arquivo aceito no POST (o front manda a lista completa).
const MAX_BYTES: usize = 256 * 1024;

/// Histórico do terminal entre execuções, em `wincodeconfig/history.json`.
///
/// O terminal é por projeto (o `cwd` muda), mas o histórico é da máquina: quem
/// digitou `cargo clippy --all-targets` ontem quer a seta para cima hoje, em
/// qualquer pasta. Fica ao lado do `.exe`, junto das preferências e das fontes.
pub(super) struct History {
    path: Option<PathBuf>,
    cached: Mutex<Option<Vec<String>>>,
}

impl History {
    pub(super) fn new() -> Self {
        Self {
            path: Some(crate::config::dir().join("history.json")),
            cached: Mutex::new(None),
        }
    }

    /// Lista atual, `[]` quando ainda não há nada (ou o arquivo está inválido).
    pub(super) fn load(&self) -> Vec<String> {
        let mut cached = self.cached.lock().unwrap();
        if cached.is_none() {
            *cached = Some(self.read_from_disk());
        }
        cached.clone().unwrap_or_default()
    }

    /// Substitui o histórico pelo que o front enviou, já saneado.
    pub(super) fn save(&self, body: &[u8]) -> Result<Vec<String>, String> {
        if body.len() > MAX_BYTES {
            return Err(format!(
                "histórico muito grande (limite de {} KiB)",
                MAX_BYTES / 1024
            ));
        }

        let parsed: serde_json::Value =
            serde_json::from_slice(body).map_err(|err| format!("histórico inválido: {err}"))?;
        let Some(items) = parsed.as_array() else {
            return Err("histórico inválido: esperado uma lista de comandos".into());
        };

        let entries = sanitize(items.iter().filter_map(|item| item.as_str()));
        self.write(&entries)?;
        *self.cached.lock().unwrap() = Some(entries.clone());
        Ok(entries)
    }

    fn write(&self, entries: &[String]) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|err| format!("não foi possível criar {}: {err}", dir.display()))?;
        }

        let text = serde_json::to_string_pretty(entries).map_err(|err| err.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)
            .map_err(|err| format!("não foi possível gravar {}: {err}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            format!("não foi possível salvar {}: {err}", path.display())
        })
    }

    fn read_from_disk(&self) -> Vec<String> {
        let Some(path) = &self.path else {
            return Vec::new();
        };

        let Ok(text) = std::fs::read_to_string(path) else {
            return Vec::new();
        };

        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Array(items)) => {
                sanitize(items.iter().filter_map(|item| item.as_str()))
            }
            _ => Vec::new(),
        }
    }
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
    use super::sanitize;

    fn run(entries: &[&str]) -> Vec<String> {
        sanitize(entries.iter().copied())
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
}
