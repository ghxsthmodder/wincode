use std::path::PathBuf;
use std::sync::Mutex;

/// Teto de tamanho aceito no POST: preferências são pequenas e um limite evita
/// que a rota vire um depósito de arquivos arbitrários.
const MAX_BYTES: usize = 64 * 1024;

/// Preferências do editor (tema, fonte, indentação, interface…) em JSON.
///
/// Ficam no diretório de configuração do usuário, **fora** da pasta do projeto:
/// são preferências da máquina, não conteúdo do repositório aberto.
pub(super) struct Settings {
    path: Option<PathBuf>,
    cached: Mutex<Option<String>>,
}

impl Settings {
    pub(super) fn new() -> Self {
        Self {
            path: settings_path(),
            cached: Mutex::new(None),
        }
    }

    /// JSON atual; `{}` quando nada foi salvo ainda ou o arquivo está inválido.
    pub(super) fn load(&self) -> String {
        let mut cached = self.cached.lock().unwrap();
        if cached.is_none() {
            *cached = Some(self.read_from_disk());
        }
        cached.clone().unwrap_or_else(|| "{}".into())
    }

    /// Valida e grava o JSON recebido pelo front-end.
    pub(super) fn save(&self, body: &[u8]) -> Result<(), String> {
        if body.len() > MAX_BYTES {
            return Err(format!(
                "configuração muito grande (limite de {} KiB)",
                MAX_BYTES / 1024
            ));
        }

        let text = std::str::from_utf8(body)
            .map_err(|_| "configuração inválida: conteúdo não é UTF-8".to_string())?;

        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|err| format!("configuração inválida: {err}"))?;
        if !value.is_object() {
            return Err("configuração inválida: esperado um objeto JSON".into());
        }

        let path = self
            .path
            .as_ref()
            .ok_or("não foi possível localizar a pasta de configuração do usuário")?;

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|err| format!("não foi possível criar {}: {err}", dir.display()))?;
        }

        // Grava em um temporário e renomeia: uma queda no meio da escrita não
        // deixa o arquivo de configuração corrompido.
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into());
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &pretty)
            .map_err(|err| format!("não foi possível gravar {}: {err}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            format!("não foi possível salvar {}: {err}", path.display())
        })?;

        *self.cached.lock().unwrap() = Some(pretty);
        Ok(())
    }

    fn read_from_disk(&self) -> String {
        let Some(path) = &self.path else {
            return "{}".into();
        };

        let Ok(text) = std::fs::read_to_string(path) else {
            return "{}".into();
        };

        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) if value.is_object() => value.to_string(),
            _ => "{}".into(),
        }
    }
}

/// `%APPDATA%\Wincode\settings.json` no Windows;
/// `$XDG_CONFIG_HOME/wincode/settings.json` (ou `~/.config/...`) nos demais.
fn settings_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }?;

    Some(base.join("Wincode").join("settings.json"))
}
