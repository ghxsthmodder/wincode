use regex::{NoExpand, Regex, RegexBuilder};
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
/// Substituir mexe em disco: o orçamento é maior, mas não infinito.
const REPLACE_BUDGET: Duration = Duration::from_secs(5);
/// Caracteres da linha mostrados na prévia.
const MAX_PREVIEW: usize = 200;

/// Como o texto é interpretado: busca literal (escapada) ou expressão regular,
/// com ou sem diferenciar maiúsculas.
#[derive(Clone, Copy, Default)]
pub(super) struct Options {
    pub(super) regex: bool,
    pub(super) case_sensitive: bool,
}

impl Options {
    pub(super) fn new(regex: bool, case_sensitive: bool) -> Self {
        Self {
            regex,
            case_sensitive,
        }
    }
}

/// Busca nos arquivos do projeto.
///
/// Percorre o mesmo índice da abertura rápida — herda os `SKIP_DIRS` e o teto de
/// arquivos — e devolve as ocorrências agrupadas por arquivo, na ordem alfabética
/// dos caminhos. Arquivos binários ou grandes são ignorados. O texto literal é
/// escapado antes de virar regex, então um `.` ou `(` digitado procura o
/// caractere, e não um padrão.
pub(super) fn run(
    root: &Path,
    query: &str,
    options: &Options,
) -> Result<serde_json::Value, String> {
    if query.trim().is_empty() {
        return Ok(serde_json::json!({ "files": [], "truncated": false }));
    }

    let matcher = compile(query, options)?;
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
            let Some(found) = matcher.find(line) else {
                continue;
            };

            // O limite só vale quando há mais uma ocorrência de fato.
            if hits.len() >= MAX_HITS_PER_FILE || total >= MAX_HITS {
                truncated = true;
                break;
            }

            hits.push(serde_json::json!({
                "line": index + 1,
                "column": line[..found.start()].chars().count() + 1,
                "text": line.trim().chars().take(MAX_PREVIEW).collect::<String>(),
            }));
            total += 1;
        }

        if !hits.is_empty() {
            groups.push((relative, hits));
        }
    }

    Ok(serde_json::json!({
        "files": groups
            .into_iter()
            .map(|(path, hits)| serde_json::json!({ "path": path, "hits": hits }))
            .collect::<Vec<_>>(),
        "truncated": truncated,
    }))
}

/// Substitui em todos os arquivos do índice, sem staging: é "substituir tudo".
///
/// Com busca literal a substituição também é literal (um `$1` digitado vira
/// `$1`, não uma referência); com `regex` ligada, `$1`/`${nome}` funcionam como
/// no padrão. Arquivos só são reescritos quando algo muda de fato.
pub(super) fn replace(
    root: &Path,
    query: &str,
    replacement: &str,
    options: &Options,
) -> Result<serde_json::Value, String> {
    if query.trim().is_empty() {
        return Err("digite o que procurar".into());
    }

    let matcher = compile(query, options)?;
    let started = Instant::now();
    let mut files = 0usize;
    let mut replacements = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut truncated = false;

    for relative in crate::index::list(root) {
        if started.elapsed() > REPLACE_BUDGET {
            truncated = true;
            break;
        }

        let path = root.join(&relative);
        let Some(text) = read_text(&path) else {
            continue;
        };
        if !matcher.is_match(&text) {
            continue;
        }

        let count = matcher.find_iter(&text).count();
        let next = if options.regex {
            matcher.replace_all(&text, replacement).into_owned()
        } else {
            matcher
                .replace_all(&text, NoExpand(replacement))
                .into_owned()
        };

        if next == text {
            continue;
        }

        match std::fs::write(&path, next) {
            Ok(()) => {
                files += 1;
                replacements += count;
            }
            Err(err) => errors.push(format!("{relative}: {err}")),
        }
    }

    Ok(serde_json::json!({
        "files": files,
        "replacements": replacements,
        "errors": errors,
        "truncated": truncated,
    }))
}

fn compile(query: &str, options: &Options) -> Result<Regex, String> {
    let trimmed = query.trim();
    let pattern = if options.regex {
        trimmed.to_string()
    } else {
        regex::escape(trimmed)
    };

    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|err| format!("padrão inválido: {err}"))
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
    use super::{replace, run, Options};

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

    fn paths(result: &serde_json::Value) -> Vec<String> {
        result["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["path"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn finds_case_insensitive_hits_grouped_by_file() {
        let dir = project("hits");
        let result = run(&dir, "ITEM", &Options::default()).unwrap();

        assert_eq!(paths(&result), vec!["README.md", "src/main.rs"]);
        let files = result["files"].as_array().unwrap();
        assert_eq!(files[0]["hits"][0]["line"], 1);
        assert_eq!(files[0]["hits"][1]["line"], 2);
        assert_eq!(files[1]["hits"][0]["line"], 2);
        assert_eq!(files[1]["hits"][0]["column"], 9);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_binary_and_ignored_directories() {
        let dir = project("skip");
        let result = run(&dir, "item", &Options::default()).unwrap();

        for path in paths(&result) {
            assert!(!path.contains("node_modules"), "ignorou node_modules");
            assert_ne!(path, "binario.dat", "ignorou binário");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let dir = project("empty");
        let result = run(&dir, "   ", &Options::default()).unwrap();
        assert!(result["files"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn column_points_at_the_match_in_the_original_line() {
        let dir = std::env::temp_dir().join(format!("wincode-search-col-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "        alvo\n").unwrap();

        let result = run(&dir, "alvo", &Options::default()).unwrap();
        let hit = &result["files"][0]["hits"][0];
        assert_eq!(hit["column"], 9);
        assert_eq!(hit["text"], "alvo");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn case_sensitive_only_matches_the_exact_case() {
        let dir = project("case");
        let result = run(&dir, "Item", &Options::new(false, true)).unwrap();
        assert_eq!(paths(&result), vec!["README.md", "src/main.rs"]);
        assert_eq!(result["files"][0]["hits"].as_array().unwrap().len(), 1);

        let none = run(&dir, "iTeM", &Options::new(false, true)).unwrap();
        assert!(none["files"].as_array().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn regex_matches_a_pattern_and_reports_the_column() {
        let dir = project("regex");
        let result = run(&dir, "let\\s+\\w+\\s+=", &Options::new(true, false)).unwrap();

        assert_eq!(paths(&result), vec!["src/main.rs"]);
        let hit = &result["files"][0]["hits"][0];
        assert_eq!(hit["line"], 2);
        assert_eq!(hit["column"], 5);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn literal_query_does_not_act_as_a_pattern() {
        let dir = project("literal");
        let result = run(&dir, "let.+=", &Options::default()).unwrap();
        assert!(result["files"].as_array().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_pattern_is_an_error_not_a_crash() {
        let dir = project("invalid");
        let error = run(&dir, "([a-z", &Options::new(true, false)).unwrap_err();
        assert!(error.contains("padrão inválido"), "{error}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_literal_keeps_dollar_signs_verbatim() {
        let dir = project("replace");
        let result = replace(&dir, "item", "$1 e texto", &Options::default()).unwrap();

        // `src/main.rs` também tem um "Item" e a busca não diferencia caixa.
        assert_eq!(result["files"], 2);
        assert_eq!(result["replacements"], 3);
        assert_eq!(
            std::fs::read_to_string(dir.join("README.md")).unwrap(),
            "$1 e texto um\n$1 e texto dois\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_with_regex_uses_capture_groups() {
        let dir = project("replace-regex");
        let result = replace(
            &dir,
            "let\\s+(\\w+)",
            "let mut $1",
            &Options::new(true, false),
        )
        .unwrap();

        assert_eq!(result["replacements"], 1);
        assert!(std::fs::read_to_string(dir.join("src").join("main.rs"))
            .unwrap()
            .contains("let mut Item"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
