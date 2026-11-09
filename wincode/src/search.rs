use std::path::Path;
use std::time::{Duration, Instant};

/// Teto de ocorrências devolvidas: refinar a busca é digitar mais.
const MAX_HITS: usize = 200;
/// Teto por arquivo: um arquivo com mil ocorrências não monopoliza a lista.
const MAX_HITS_PER_FILE: usize = 20;
/// Arquivos maiores não entram na busca (minificados, logs, assets).
const MAX_FILE_BYTES: u64 = 1_000_000;
/// Orçamento de tempo: a rota responde o que coube e o front avisa do truncamento.
const BUDGET: Duration = Duration::from_millis(1500);
/// Caracteres da linha mostrados na prévia.
const MAX_PREVIEW: usize = 200;

/// Busca textual (sem diferenciar maiúsculas) nos arquivos do projeto.
///
/// Percorre o mesmo índice da abertura rápida — herda os `SKIP_DIRS` e o teto de
/// arquivos — e devolve as ocorrências agrupadas por arquivo, na ordem alfabética
/// dos caminhos. Arquivos binários ou grandes são ignorados.
pub(super) fn run(root: &Path, query: &str) -> serde_json::Value {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return serde_json::json!({ "files": [], "truncated": false });
    }

    let started = Instant::now();
    let mut groups: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;

    for relative in crate::index::list(root) {
        if total >= MAX_HITS || started.elapsed() > BUDGET {
            truncated = true;
            break;
        }

        let Some(text) = read_text(&root.join(&relative)) else {
            continue;
        };

        let mut hits: Vec<serde_json::Value> = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let Some(hit) = find_in_line(line, &needle) else {
                continue;
            };

            // O limite só vale quando há mais uma ocorrência de fato.
            if hits.len() >= MAX_HITS_PER_FILE || total >= MAX_HITS {
                truncated = true;
                break;
            }

            hits.push(serde_json::json!({
                "line": index + 1,
                "column": hit.column,
                "text": hit.preview,
            }));
            total += 1;
        }

        if !hits.is_empty() {
            groups.push((relative, hits));
        }
    }

    serde_json::json!({
        "files": groups
            .into_iter()
            .map(|(path, hits)| serde_json::json!({ "path": path, "hits": hits }))
            .collect::<Vec<_>>(),
        "truncated": truncated,
    })
}

struct Hit {
    column: usize,
    preview: String,
}

/// Posição (1-based, na linha original) e prévia da primeira ocorrência.
///
/// A coluna serve para o Monaco posicionar o cursor, então conta desde o começo
/// da linha — enquanto a prévia é o texto sem a indentação, para caber na lista.
fn find_in_line(line: &str, needle: &str) -> Option<Hit> {
    let lowered = line.to_lowercase();
    let position = lowered.find(needle)?;

    Some(Hit {
        column: lowered[..position].chars().count() + 1,
        preview: line.trim().chars().take(MAX_PREVIEW).collect(),
    })
}

fn read_text(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }

    let bytes = std::fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }

    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::run;

    fn project(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("wincode-search-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src").join("main.rs"),
            "fn main() {\n    let Item = 1;\n}\n",
        )
        .unwrap();
        std::fs::write(dir.join("README.md"), "Item um\nitem dois\n").unwrap();
        std::fs::write(dir.join("binario.dat"), b"\x00\x01item").unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::write(dir.join("node_modules").join("x.js"), "item\n").unwrap();
        dir
    }

    #[test]
    fn finds_case_insensitive_hits_grouped_by_file() {
        let dir = project("hits");
        let result = run(&dir, "ITEM");

        let files = result["files"].as_array().unwrap();
        let paths: Vec<&str> = files
            .iter()
            .map(|file| file["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["README.md", "src/main.rs"]);
        assert_eq!(files[0]["hits"][0]["line"], 1);
        assert_eq!(files[0]["hits"][1]["line"], 2);
        assert_eq!(files[1]["hits"][0]["line"], 2);
        assert_eq!(files[1]["hits"][0]["column"], 9);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_binary_and_ignored_directories() {
        let dir = project("skip");
        let result = run(&dir, "item");
        let files = result["files"].as_array().unwrap();

        for file in files {
            let path = file["path"].as_str().unwrap();
            assert!(!path.contains("node_modules"), "ignorou node_modules");
            assert_ne!(path, "binario.dat", "ignorou binário");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let dir = project("empty");
        assert!(run(&dir, "   ")["files"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn column_points_at_the_match_in_the_original_line() {
        let dir = std::env::temp_dir().join(format!("wincode-search-col-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "        alvo\n").unwrap();

        let result = run(&dir, "alvo");
        let hit = &result["files"][0]["hits"][0];
        assert_eq!(hit["column"], 9);
        assert_eq!(hit["text"], "alvo");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
