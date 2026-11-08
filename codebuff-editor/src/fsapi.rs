use std::path::{Path, PathBuf};

/// Estado compartilhado entre as requisições do protocolo `editor://`.
pub(super) struct FsState {
    /// Raiz do projeto aberto. Nenhum caminho fora dela é acessível.
    pub(super) root: PathBuf,
    /// Diretório de trabalho do terminal integrado.
    pub(super) cwd: PathBuf,
}

impl FsState {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            cwd: root.clone(),
            root,
        }
    }

    /// Resolve um caminho vindo da interface, impedindo sair da raiz do projeto.
    pub(super) fn resolve(&self, requested: &str) -> Result<PathBuf, String> {
        resolve_within(&self.root, requested)
    }
}

pub(super) fn resolve_within(root: &Path, requested: &str) -> Result<PathBuf, String> {
    let base = std::fs::canonicalize(root).map_err(|e| format!("pasta inválida: {e}"))?;
    let requested = requested.trim();

    let candidate = if requested.is_empty() {
        base.clone()
    } else {
        let path = Path::new(requested);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            base.join(path)
        }
    };

    // Arquivos novos ainda não existem, então canonicalizamos o pai.
    let target = if candidate.exists() {
        std::fs::canonicalize(&candidate).map_err(|e| e.to_string())?
    } else {
        let parent = candidate.parent().ok_or("caminho inválido")?;
        let parent = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
        let name = candidate.file_name().ok_or("caminho inválido")?;
        parent.join(name)
    };

    if target.starts_with(&base) {
        Ok(target)
    } else {
        Err("acesso fora da pasta do projeto".into())
    }
}
