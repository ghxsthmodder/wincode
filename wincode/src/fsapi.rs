use std::path::{Path, PathBuf};

/// Estado compartilhado entre as requisições do protocolo `editor://`.
pub(super) struct FsState {
    /// Raiz do projeto aberto. Nenhum caminho fora dela é acessível.
    pub(super) root: PathBuf,
    /// Indica se a pasta foi escolhida pela CLI ou pela tela de seleção.
    /// Sem ela, o front-end mostra a tela de boas-vindas.
    pub(super) selected: bool,
    /// Diretório de trabalho do terminal integrado.
    pub(super) cwd: PathBuf,
}

impl FsState {
    pub(super) fn new(root: Option<PathBuf>) -> Self {
        match root {
            Some(dir) => Self {
                cwd: dir.clone(),
                root: dir,
                selected: true,
            },
            None => {
                let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                Self {
                    cwd: current.clone(),
                    root: current,
                    selected: false,
                }
            }
        }
    }

    pub(super) fn set_root(&mut self, root: PathBuf) {
        self.cwd = root.clone();
        self.root = root;
        self.selected = true;
    }

    /// Resolve um caminho vindo da interface, impedindo sair da raiz do projeto.
    pub(super) fn resolve(&self, requested: &str) -> Result<PathBuf, String> {
        resolve_within(&self.root, requested)
    }

    /// Raiz canônica: usada para bloquear operações destrutivas na própria pasta aberta.
    pub(super) fn canonical_root(&self) -> Result<PathBuf, String> {
        std::fs::canonicalize(&self.root).map_err(|e| format!("pasta inválida: {e}"))
    }

    /// Resolve o caminho de um item que ainda não existe (criar ou renomear).
    pub(super) fn resolve_new(&self, requested: &str) -> Result<PathBuf, String> {
        let base = self.canonical_root()?;
        let target = self.resolve(requested)?;

        if target == base {
            return Err("a pasta do projeto não pode ser usada como destino".into());
        }

        let name = target
            .file_name()
            .ok_or("caminho inválido")?
            .to_string_lossy()
            .to_string();
        validate_name(&name)?;

        if target.exists() {
            return Err(format!("já existe um item chamado {name}"));
        }

        Ok(target)
    }

    /// O diretório do terminal acompanha renomeações feitas dentro da raiz.
    pub(super) fn remap_cwd(&mut self, from: &Path, to: &Path) {
        if let Ok(rest) = self.cwd.strip_prefix(from) {
            self.cwd = to.join(rest);
        }
    }

    /// Se o terminal estava dentro do item removido, volta para a raiz.
    pub(super) fn reset_cwd_if_inside(&mut self, removed: &Path) {
        if self.cwd.starts_with(removed) {
            self.cwd = self.root.clone();
        }
    }
}

/// Valida nomes digitados na interface antes de tocar no sistema de arquivos.
pub(super) fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("o nome não pode ficar vazio".into());
    }
    if name == "." || name == ".." {
        return Err("nome inválido".into());
    }
    if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0']) {
        return Err("o nome não pode conter / \\ : * ? \" < > |".into());
    }
    if name.chars().any(char::is_control) {
        return Err("o nome não pode conter caracteres de controle".into());
    }
    Ok(())
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
