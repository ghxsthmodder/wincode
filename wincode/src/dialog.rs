use std::path::PathBuf;

/// Abre o diálogo nativo de pastas. Retorna `Ok(None)` quando o usuário cancela.
///
/// O diálogo é modal: ele bloqueia a thread da interface enquanto estiver aberto,
/// o que é o comportamento esperado nesse caso.
#[cfg(target_os = "windows")]
pub(super) fn pick_folder() -> Result<Option<PathBuf>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Abrir pasta")
        .pick_folder())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn pick_folder() -> Result<Option<PathBuf>, String> {
    Err("diálogo nativo de pastas está disponível apenas no build para Windows".into())
}

/// Abre o diálogo nativo de arquivos. Retorna `Ok(None)` quando o usuário cancela.
#[cfg(target_os = "windows")]
pub(super) fn pick_file() -> Result<Option<PathBuf>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Abrir arquivo")
        .pick_file())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn pick_file() -> Result<Option<PathBuf>, String> {
    Err("diálogo nativo de arquivos está disponível apenas no build para Windows".into())
}
