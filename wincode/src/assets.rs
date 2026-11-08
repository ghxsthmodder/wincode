use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use tao::window::Icon;

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

/// Assets do front-end (HTML, ícones e Monaco).
///
/// Por padrão vêm embutidos no binário, o que permite distribuir um único `.exe`.
/// Se `WINCODE_ASSET_DIR` apontar para uma pasta existente, ela tem
/// prioridade — útil para editar `index.html`/Monaco sem recompilar.
pub(super) struct Assets {
    disk_root: Option<PathBuf>,
}

impl Assets {
    pub(super) fn new() -> Self {
        Self {
            disk_root: disk_root_override(),
        }
    }

    pub(super) fn get(&self, rel: &str) -> Option<Cow<'static, [u8]>> {
        let rel = rel.trim_start_matches('/');

        if rel.is_empty() || rel.split('/').any(|part| part == "..") {
            return None;
        }

        if let Some(root) = &self.disk_root {
            if let Ok(bytes) = std::fs::read(root.join(rel)) {
                return Some(Cow::Owned(bytes));
            }
        }

        embedded().get(rel).map(|bytes| Cow::Borrowed(*bytes))
    }
}

fn embedded() -> &'static HashMap<&'static str, &'static [u8]> {
    static MAP: OnceLock<HashMap<&'static str, &'static [u8]>> = OnceLock::new();
    MAP.get_or_init(|| EMBEDDED_ASSETS.iter().copied().collect())
}

fn disk_root_override() -> Option<PathBuf> {
    let dir = std::env::var("WINCODE_ASSET_DIR").ok()?;
    let path = PathBuf::from(dir);
    path.is_dir().then_some(path)
}

/// Ícone da janela e da barra de tarefas, decodificado do `.ico` que é embutido
/// no binário — assim o `.exe` continua funcionando sozinho, sem a pasta assets.
/// `WINCODE_ASSET_DIR` permite substituir o arquivo em desenvolvimento.
pub(super) fn icon() -> Option<Icon> {
    let bytes: Cow<'static, [u8]> = match disk_root_override() {
        Some(root) => Cow::Owned(std::fs::read(root.join("icons/wincode.ico")).ok()?),
        None => Cow::Borrowed(*embedded().get("icons/wincode.ico")?),
    };

    let dir = ico::IconDir::read(std::io::Cursor::new(bytes.as_ref())).ok()?;
    let entry = dir
        .entries()
        .iter()
        .filter(|entry| entry.width() >= 32)
        .max_by_key(|entry| entry.width())?;
    let image = entry.decode().ok()?;
    Icon::from_rgba(image.rgba_data().to_vec(), image.width(), image.height()).ok()
}
