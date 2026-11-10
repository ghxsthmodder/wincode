use std::path::Path;
use std::process::{Command, Stdio};

/// Resumo do repositório para a interface: nada é feito em background — o front
/// pede o status quando abixa o painel ou muda arquivos, e a resposta vale só
/// para aquele instante.
pub(super) fn status(root: &Path) -> serde_json::Value {
    let Some(output) = git(root, &["status", "--porcelain=v1", "-b"]) else {
        return serde_json::json!({ "repo": false });
    };

    // Fora de um repositório o git sai com erro e sem nada de útil na saída.
    if !output.ok {
        return serde_json::json!({ "repo": false });
    }

    let mut value = parse_status(&output.stdout);
    if let Some(object) = value.as_object_mut() {
        object.insert("repo".into(), serde_json::json!(true));
    }
    value
}

/// `git add -A` + `git commit -m <mensagem>`: o painel trabalha com "tudo", sem
/// staging por arquivo. A saída do git volta para a interface mostrar o que
/// aconteceu (inclusive "nothing to commit").
pub(super) fn commit(root: &Path, message: &str) -> Result<serde_json::Value, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("escreva a mensagem do commit".into());
    }
    if message.len() > 8_000 {
        return Err("mensagem de commit muito grande".into());
    }

    let added = git(root, &["add", "-A"]).ok_or("git não encontrado")?;
    if !added.ok {
        return Err(first_line(
            &added.stderr,
            "não foi possível preparar as alterações",
        ));
    }

    let committed = git(root, &["commit", "-m", message]).ok_or("git não encontrado")?;
    let text = format!("{}{}", committed.stdout, committed.stderr);

    Ok(serde_json::json!({
        "ok": committed.ok,
        "output": text.trim(),
    }))
}

struct Output {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Roda o git com a janela escondida (`CREATE_NO_WINDOW`, como o terminal) e
/// devolve `None` só quando o executável não existe.
fn git(root: &Path, args: &[&str]) -> Option<Output> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["-c", "color.status=false", "-c", "core.quotepath=false"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().ok()?;
    Some(Output {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
    })
}

fn first_line(text: &str, fallback: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

/// Interpreta `git status --porcelain=v1 -b`.
///
/// A primeira linha é o cabeçalho do branch (`## main...origin/main [ahead 1,
/// behind 2]`), e cada linha seguinte começa com dois caracteres de estado:
/// o primeiro é o índice e o segundo a árvore de trabalho. Renomeações vêm como
/// `R  antigo -> novo`, então o caminho mostrado é o atual.
fn parse_status(text: &str) -> serde_json::Value {
    let mut branch = String::new();
    let mut detached = false;
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut changes = Vec::new();

    for line in text.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            let (name, counts) = match header.split_once(" [") {
                Some((name, counts)) => (name, counts.trim_end_matches(']')),
                None => (header, ""),
            };

            for part in counts.split(',') {
                let part = part.trim();
                if let Some(value) = part.strip_prefix("ahead ") {
                    ahead = value.trim().parse().unwrap_or(0);
                } else if let Some(value) = part.strip_prefix("behind ") {
                    behind = value.trim().parse().unwrap_or(0);
                }
            }

            let name = name.strip_prefix("No commits yet on ").unwrap_or(name);
            if name.starts_with("HEAD (no branch)") || name.is_empty() {
                detached = true;
            } else {
                // `main...origin/main` — o nome local é o que interessa.
                branch = name.split("...").next().unwrap_or(name).trim().to_string();
            }
            continue;
        }

        if line.len() < 3 {
            continue;
        }

        let index = line.as_bytes()[0] as char;
        let worktree = line.as_bytes()[1] as char;
        let mut path = line[3..].to_string();
        if let Some((_, next)) = path.rsplit_once(" -> ") {
            path = next.to_string();
        }
        if path.is_empty() {
            continue;
        }

        changes.push(serde_json::json!({
            "path": path,
            "code": code_for(index, worktree),
        }));
    }

    changes.sort_by(|a, b| {
        let left = a["path"].as_str().unwrap_or("");
        let right = b["path"].as_str().unwrap_or("");
        left.to_lowercase().cmp(&right.to_lowercase())
    });

    serde_json::json!({
        "branch": branch,
        "detached": detached,
        "ahead": ahead,
        "behind": behind,
        "changes": changes,
    })
}

/// Uma letra por arquivo: a árvore de trabalho manda, porque é o que o editor
/// acabou de mudar; sem ela, vale o estado do índice.
fn code_for(index: char, worktree: char) -> &'static str {
    match worktree {
        'M' => "M",
        'D' => "D",
        'R' => "R",
        'C' => "C",
        'U' => "U",
        '?' => "?",
        'A' => "A",
        _ => match index {
            'M' => "M",
            'A' => "A",
            'D' => "D",
            'R' => "R",
            'C' => "C",
            'U' => "U",
            '!' => "?",
            _ => "",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn reads_branch_and_ahead_behind() {
        let status = parse_status("## main...origin/main [ahead 1, behind 2]\n");
        assert_eq!(status["branch"], "main");
        assert_eq!(status["ahead"], 1);
        assert_eq!(status["behind"], 2);
        assert_eq!(status["detached"], false);
        assert!(status["changes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn reads_detached_and_unborn_branches() {
        assert_eq!(parse_status("## HEAD (no branch)\n")["detached"], true);
        let unborn = parse_status("## No commits yet on develop\n");
        assert_eq!(unborn["branch"], "develop");
        assert_eq!(unborn["detached"], false);
    }

    #[test]
    fn lists_changes_with_one_letter_and_current_path() {
        let status = parse_status(
            "## main\n M src/main.rs\n?? novo.txt\nR  velho.txt -> novo2.txt\nA  Cargo.toml\n",
        );
        let changes = status["changes"].as_array().unwrap();

        let pairs: Vec<(String, String)> = changes
            .iter()
            .map(|change| {
                (
                    change["path"].as_str().unwrap().to_string(),
                    change["code"].as_str().unwrap().to_string(),
                )
            })
            .collect();

        assert_eq!(
            pairs,
            vec![
                ("Cargo.toml".to_string(), "A".to_string()),
                ("novo.txt".to_string(), "?".to_string()),
                ("novo2.txt".to_string(), "R".to_string()),
                ("src/main.rs".to_string(), "M".to_string()),
            ]
        );
    }

    #[test]
    fn worktree_state_wins_over_index_state() {
        let status = parse_status("## main\nMD apagado.txt\nAM novo.txt\n");
        let changes = status["changes"].as_array().unwrap();
        assert_eq!(changes[0]["code"], "D");
        assert_eq!(changes[1]["code"], "M");
    }
}
