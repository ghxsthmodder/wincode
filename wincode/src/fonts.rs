use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use serde_json::Value;

/// Fontes de código adicionadas pelo usuário.
///
/// Basta jogar um `.ttf`/`.otf` em `wincodeconfig/fonts` — o nome da família é
/// lido da tabela `name` do próprio arquivo. Um `.json` de mesmo nome permite
/// ajustar rótulo, peso e estilo (ou apontar para outro arquivo):
///
/// ```json
/// { "file": "JetBrainsMono-Regular.ttf", "label": "JetBrains Mono (local)", "weight": "400" }
/// ```
pub(super) struct Fonts {
    dir: PathBuf,
}

impl Fonts {
    pub(super) fn new() -> Self {
        Self {
            dir: crate::config::fonts_dir(),
        }
    }

    /// Lista as fontes disponíveis (arquivos soltos + descritores JSON).
    /// Nunca falha: pasta ausente ou ilegível vira uma lista vazia.
    pub(super) fn list(&self) -> Value {
        let mut specs: Vec<Value> = Vec::new();
        let mut used: Vec<String> = Vec::new();
        let mut font_files: BTreeMap<String, String> = BTreeMap::new(); // minúsculo → nome real
        let mut spec_files: Vec<String> = Vec::new();

        if let Ok(read_dir) = std::fs::read_dir(&self.dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                match extension(&name).as_deref() {
                    Some("json") => spec_files.push(name),
                    Some(ext) if is_font_ext(ext) => {
                        font_files.insert(name.to_lowercase(), name);
                    }
                    _ => {}
                }
            }
        }

        for spec_name in &spec_files {
            let Ok(text) = std::fs::read_to_string(self.dir.join(spec_name)) else {
                continue;
            };
            let Ok(spec) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if !spec.is_object() {
                continue;
            }

            let explicit = spec.get("file").and_then(|file| file.as_str());
            let file = match explicit {
                Some(file) => file.to_string(),
                None => {
                    let stem = spec_name.trim_end_matches(".json");
                    match font_files
                        .values()
                        .find(|name| name.eq_ignore_ascii_case(stem) || stem_of(name).eq_ignore_ascii_case(stem))
                    {
                        Some(name) => name.clone(),
                        None => continue,
                    }
                }
            };

            if !is_font_file_name(&file) {
                continue;
            }

            let Some(real) = font_files.get(&file.to_lowercase()) else {
                continue;
            };

            let family = spec
                .get("family")
                .and_then(|family| family.as_str())
                .map(str::to_string)
                .or_else(|| read_family(&self.dir.join(real)))
                .unwrap_or_else(|| stem_of(real));

            specs.push(font_json(
                &family,
                spec.get("label").and_then(|label| label.as_str()),
                real,
                spec.get("weight"),
                spec.get("style").and_then(|style| style.as_str()),
                &self.dir,
            ));
            used.push(real.to_lowercase());
        }

        for (lower, name) in &font_files {
            if used.iter().any(|used| used == lower) {
                continue;
            }
            let family = read_family(&self.dir.join(name)).unwrap_or_else(|| stem_of(name));
            specs.push(font_json(&family, None, name, None, None, &self.dir));
        }

        specs.sort_by(|a, b| {
            let left = a.get("label").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            let right = b.get("label").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            left.cmp(&right)
        });

        serde_json::json!({
            "dir": self.dir.to_string_lossy(),
            "fonts": specs,
        })
    }

    /// Conteúdo de um arquivo de fonte, restrito à pasta `fonts`.
    pub(super) fn read(&self, name: &str) -> Option<Vec<u8>> {
        let decoded = urlencoding::decode(name)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| name.to_string());
        let name = decoded.trim();

        if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
            return None;
        }
        if !is_font_file_name(name) {
            return None;
        }

        std::fs::read(self.dir.join(name)).ok()
    }
}

fn font_json(
    family: &str,
    label: Option<&str>,
    file: &str,
    weight: Option<&Value>,
    style: Option<&str>,
    dir: &Path,
) -> Value {
    let weight = match weight {
        Some(Value::String(weight)) if !weight.trim().is_empty() && weight.len() <= 20 => {
            weight.trim().to_string()
        }
        Some(Value::Number(weight)) => weight.to_string(),
        _ => "400".into(),
    };
    let style = match style {
        Some("italic") => "italic",
        Some("oblique") => "oblique",
        _ => "normal",
    };

    serde_json::json!({
        "family": family.trim(),
        "label": label.map(str::trim).filter(|label| !label.is_empty()).unwrap_or(family),
        "file": file,
        "weight": weight,
        "style": style,
        // O `?v=` acompanha o mtime: trocar o arquivo invalida o cache do WebView.
        "url": format!("/fonts/{}", urlencoding::encode(file)),
        "version": modified_ms(&dir.join(file)),
    })
}

fn modified_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|age| age.as_millis() as u64)
        .unwrap_or(0)
}

fn read_family(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    family_name(&bytes)
}

fn extension(name: &str) -> Option<String> {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .filter(|ext| !ext.is_empty())
}

fn stem_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| name.to_string())
}

fn is_font_ext(ext: &str) -> bool {
    matches!(ext, "ttf" | "otf" | "woff" | "woff2")
}

fn is_font_file_name(name: &str) -> bool {
    extension(name).map(|ext| is_font_ext(&ext)).unwrap_or(false)
}

/// Lê a família tipográfica da tabela `name` de um SFNT (TTF/OTF).
///
/// Dá preferência ao nome tipográfico (nameID 16) e à codificação UTF-16 do
/// Windows; nomes de coleções `.ttc` e formatos compactados (WOFF) ficam de
/// fora — para esses, use o JSON descritor com `"family"`.
fn family_name(bytes: &[u8]) -> Option<String> {
    if bytes.get(0..4)? == b"ttcf" {
        return None;
    }

    let num_tables = u16_at(bytes, 4)? as usize;
    let mut name_table = None;
    for index in 0..num_tables {
        let record = 12 + index * 16;
        if bytes.get(record..record + 4)? == b"name" {
            name_table = Some(u32_at(bytes, record + 8)? as usize);
            break;
        }
    }

    let base = name_table?;
    let count = u16_at(bytes, base + 2)? as usize;
    let strings = base + u16_at(bytes, base + 4)? as usize;

    let mut best: Option<(u8, String)> = None;
    for index in 0..count {
        let record = base + 6 + index * 12;
        let platform = u16_at(bytes, record)?;
        let encoding = u16_at(bytes, record + 2)?;
        let name_id = u16_at(bytes, record + 6)?;
        if name_id != 16 && name_id != 1 {
            continue;
        }

        let length = u16_at(bytes, record + 8)? as usize;
        let offset = u16_at(bytes, record + 10)? as usize;
        let data = bytes.get(strings + offset..strings + offset + length)?;

        let text = match platform {
            // 0 = Unicode, 3 = Windows: UTF-16 big-endian.
            0 | 3 => {
                let units: Vec<u16> = data
                    .chunks_exact(2)
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                    .collect();
                String::from_utf16_lossy(&units)
            }
            // 1 = Macintosh: os nomes das fontes de código são ASCII.
            1 => data.iter().map(|byte| *byte as char).collect(),
            _ => continue,
        };

        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }

        // Nome tipográfico vale mais que o legado; Windows mais que os outros.
        let score = u8::from(name_id == 16) * 2
            + match (platform, encoding) {
                (3, 10) | (3, 1) => 2,
                (0, _) => 1,
                _ => 0,
            };

        if best.as_ref().map(|(best, _)| score > *best).unwrap_or(true) {
            best = Some((score, text));
        }
    }

    best.map(|(_, family)| family)
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SFNT mínimo: só o cabeçalho com a tabela `name` e um nome UTF-16.
    fn sfnt(family: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0, 1, 0, 0]); // versão 1.0
        bytes.extend_from_slice(&1u16.to_be_bytes()); // uma tabela
        bytes.extend_from_slice(&[0; 6]); // busca/intervalo
        bytes.extend_from_slice(b"name");
        bytes.extend_from_slice(&0u32.to_be_bytes()); // checksum
        bytes.extend_from_slice(&28u32.to_be_bytes()); // offset da tabela
        bytes.extend_from_slice(&0u32.to_be_bytes()); // tamanho (não conferido)

        // Tabela name: formato 0, 1 registro, strings após 6 + 12 bytes.
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&18u16.to_be_bytes());
        bytes.extend_from_slice(&3u16.to_be_bytes()); // Windows
        bytes.extend_from_slice(&1u16.to_be_bytes()); // BMP
        bytes.extend_from_slice(&0x0409u16.to_be_bytes()); // en-US
        bytes.extend_from_slice(&1u16.to_be_bytes()); // nameID família
        bytes.extend_from_slice(&((family.len() as u16) * 2).to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        for unit in family.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }

        bytes
    }

    #[test]
    fn le_a_familia_da_tabela_name() {
        assert_eq!(family_name(&sfnt("JetBrains Mono")).as_deref(), Some("JetBrains Mono"));
    }

    #[test]
    fn ignora_arquivo_que_nao_e_fonte() {
        assert_eq!(family_name(b"nao e uma fonte"), None);
    }

    #[test]
    fn extensoes_aceitas() {
        assert!(is_font_file_name("a.TTF"));
        assert!(is_font_file_name("a.woff2"));
        assert!(!is_font_file_name("a.txt"));
        assert!(!is_font_file_name("a"));
    }
}
