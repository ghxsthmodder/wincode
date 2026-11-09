use std::path::PathBuf;

/// Pasta de configuração do aplicativo, ao lado do executável:
/// `wincodeconfig/`. Mantê-la junto do `.exe` deixa o editor portátil — leve a
/// pasta (ou o pendrive) para outra máquina e as fontes/recentes vão junto.
///
/// `WINCODE_CONFIG_DIR` tem prioridade, útil para desenvolvimento e testes sem
/// escrever ao lado do binário.
pub(super) fn dir() -> PathBuf {
    if let Some(custom) = std::env::var_os("WINCODE_CONFIG_DIR") {
        return PathBuf::from(custom);
    }

    let base = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    base.join("wincodeconfig")
}

/// Fontes adicionadas pelo usuário (`wincodeconfig/fonts`).
pub(super) fn fonts_dir() -> PathBuf {
    dir().join("fonts")
}
