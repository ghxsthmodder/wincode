use std::path::{Path, PathBuf};

/// Pastas que ficam fora do índice de abertura rápida: caches, dependências e
/// saídas de build. São as mesmas que qualquer projeto gera em quantidade e que
/// ninguém abre pelo Ctrl+P.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".vs",
    ".gradle",
    ".idea",
    ".turbo",
    ".parcel-cache",
    "node_modules",
    "bower_components",
    "vendor",
    "Pods",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    ".nuxt",
    ".svelte-kit",
    "obj",
];

/// Quantos arquivos o índice devolve. Um projeto gigante responde rápido com o
/// teto; o Ctrl+P raramente precisa de mais do que isso.
const MAX_FILES: usize = 8000;

/// Profundidade máxima: evita varrer árvores patológicas (e links que apontam
/// para cima).
const MAX_DEPTH: usize = 16;

/// Caminhos relativos à raiz, com `/` e em ordem alfabética — o mesmo formato
/// que o front-end usa para montar o caminho absoluto ao abrir.
pub(super) fn list(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    walk(root, root, 0, &mut files);
    files.sort();
    files
}

fn walk(root: &Path, dir: &Path, depth: usize, files: &mut Vec<String>) {
    if depth > MAX_DEPTH || files.len() >= MAX_FILES {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut subdirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // `file_type` não segue links: uma junção/link para a raiz não vira loop.
        let Ok(kind) = entry.file_type() else {
            continue;
        };

        if kind.is_dir() {
            if !is_skipped(&name) {
                subdirs.push(entry.path());
            }
        } else if kind.is_file() {
            if let Ok(relative) = entry.path().strip_prefix(root) {
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
            if files.len() >= MAX_FILES {
                return;
            }
        }
    }

    for subdir in subdirs {
        walk(root, &subdir, depth + 1, files);
        if files.len() >= MAX_FILES {
            return;
        }
    }
}

fn is_skipped(name: &str) -> bool {
    SKIP_DIRS.iter().any(|skip| skip.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::list;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("wincode-index-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_relative_paths_and_skips_heavy_dirs() {
        let dir = temp_dir("tree");
        std::fs::create_dir_all(dir.join("src").join("nested")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules").join("pacote")).unwrap();
        std::fs::create_dir_all(dir.join("target").join("debug")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(dir.join("src").join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.join("src").join("nested").join("mod.rs"), "").unwrap();
        std::fs::write(dir.join("node_modules").join("pacote").join("index.js"), "").unwrap();
        std::fs::write(dir.join("target").join("debug").join("app.exe"), "").unwrap();

        let files = list(&dir);
        assert_eq!(
            files,
            vec!["Cargo.toml", "src/main.rs", "src/nested/mod.rs"]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skip_list_is_case_insensitive() {
        let dir = temp_dir("case");
        std::fs::create_dir_all(dir.join("Node_Modules").join("x")).unwrap();
        std::fs::write(dir.join("Node_Modules").join("x").join("a.js"), "").unwrap();
        std::fs::write(dir.join("ok.txt"), "").unwrap();

        assert_eq!(list(&dir), vec!["ok.txt"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_root_is_empty_not_panic() {
        assert!(list(std::path::Path::new("C:\\pasta\\que\\nao\\existe")).is_empty());
    }
}
