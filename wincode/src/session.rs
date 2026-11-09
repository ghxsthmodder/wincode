use std::path::PathBuf;

/// Teto do JSON aceito no POST: abas, cursores e pastas expandidas cabem com
/// folga, e o limite evita que a rota vire depósito de arquivos.
const MAX_BYTES: usize = 256 * 1024;

/// Sessão guardada por projeto em `wincodeconfig/sessions/<chave>.json`:
/// abas abertas, aba ativa, cursor de cada uma e pastas expandidas.
///
/// Fica junto do executável, como os recentes e as fontes — o JSON em si é
/// montado pelo front-end; aqui só há leitura e gravação com validação.
pub(super) struct Sessions {
    dir: PathBuf,
}

impl Sessions {
    pub(super) fn new() -> Self {
        Self {
            dir: crate::config::dir().join("sessions"),
        }
    }

    /// Sessão do projeto; `null` quando ainda não há nada salvo (ou o arquivo
    /// está corrompido — nesse caso é melhor abrir sem restaurar).
    pub(super) fn load(&self, root: &str) -> serde_json::Value {
        if root.trim().is_empty() {
            return serde_json::Value::Null;
        }

        let Ok(text) = std::fs::read_to_string(self.path_for(root)) else {
            return serde_json::Value::Null;
        };

        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) if value.is_object() => value,
            _ => serde_json::Value::Null,
        }
    }

    /// Grava a sessão do projeto (uma queda no meio da escrita não corrompe a
    /// anterior: grava em `.tmp` e renomeia).
    pub(super) fn save(&self, root: &str, body: &[u8]) -> Result<(), String> {
        if root.trim().is_empty() {
            return Err("projeto ausente".into());
        }
        if body.len() > MAX_BYTES {
            return Err(format!(
                "sessão muito grande (limite de {} KiB)",
                MAX_BYTES / 1024
            ));
        }

        let text = std::str::from_utf8(body)
            .map_err(|_| "sessão inválida: conteúdo não é UTF-8".to_string())?;
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|err| format!("sessão inválida: {err}"))?;
        if !value.is_object() {
            return Err("sessão inválida: esperado um objeto JSON".into());
        }

        std::fs::create_dir_all(&self.dir)
            .map_err(|err| format!("não foi possível criar {}: {err}", self.dir.display()))?;

        let path = self.path_for(root);
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into());
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &pretty)
            .map_err(|err| format!("não foi possível gravar {}: {err}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            format!("não foi possível salvar {}: {err}", path.display())
        })
    }

    fn path_for(&self, root: &str) -> PathBuf {
        self.dir.join(format!("{}.json", key(root)))
    }
}

/// FNV-1a de 64 bits sobre o caminho em minúsculas: nome de arquivo estável
/// entre versões (ao contrário do `DefaultHasher`) e sem caracteres inválidos
/// no Windows. `C:\Projetos\App` e `c:\projetos\app` caem no mesmo arquivo.
fn key(root: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in root.replace('\\', "/").to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::{key, Sessions};

    #[test]
    fn key_is_case_and_separator_insensitive() {
        assert_eq!(key("C:\\Projetos\\App"), key("c:/projetos/app"));
        assert_ne!(key("C:\\projetos\\app"), key("C:\\projetos\\outro"));
    }

    #[test]
    fn key_is_hex_and_stable() {
        let value = key("C:\\projetos\\wincode");
        assert_eq!(value.len(), 16);
        assert!(value.chars().all(|char| char.is_ascii_hexdigit()));
        assert_eq!(value, key("C:\\projetos\\wincode"));
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("wincode-session-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sessions = Sessions { dir: dir.clone() };

        assert!(sessions.load("C:\\projetos\\app").is_null());
        sessions
            .save(
                "C:\\projetos\\app",
                br#"{"tabs":[{"path":"C:\\a.rs"}],"active":"C:\\a.rs"}"#,
            )
            .unwrap();

        let loaded = sessions.load("c:/projetos/APP");
        assert_eq!(loaded["active"], "C:\\a.rs");
        assert_eq!(loaded["tabs"][0]["path"], "C:\\a.rs");
        assert!(sessions.load("C:\\projetos\\outro").is_null());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_invalid_payloads() {
        let dir = std::env::temp_dir().join(format!("wincode-session-bad-{}", std::process::id()));
        let sessions = Sessions { dir: dir.clone() };

        assert!(sessions.save("", b"{}").is_err());
        assert!(sessions.save("C:\\p", b"[1,2]").is_err());
        assert!(sessions.save("C:\\p", b"{nao e json").is_err());
        assert!(sessions.save("C:\\p", &[0xff, 0xfe]).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
