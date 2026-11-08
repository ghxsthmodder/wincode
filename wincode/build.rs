use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let assets_dir = manifest_dir.join("assets");
    println!("cargo:rerun-if-changed={}", assets_dir.display());

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    collect(&assets_dir, &assets_dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));

    // Gera uma tabela `&[(&str, &[u8])]` com include_bytes!, o que embute
    // index.html e o Monaco dentro do próprio .exe (distribuição single file).
    let mut code = String::with_capacity(files.len() * 128);
    code.push_str("// Gerado por build.rs — não editar.\n");
    code.push_str("pub(super) static EMBEDDED_ASSETS: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in &files {
        code.push_str("    (");
        code.push_str(&format!("{:?}", rel));
        code.push_str(", include_bytes!(");
        code.push_str(&format!("{:?}", abs.to_string_lossy()));
        code.push_str(")),\n");
    }
    code.push_str("];\n");

    std::fs::write(out_dir.join("embedded_assets.rs"), code).expect("escrever embedded_assets.rs");

    if files.is_empty() {
        println!(
            "cargo:warning=nenhum asset em assets/: rode `node scripts/prepare-monaco.js` para baixar o Monaco."
        );
    } else {
        println!(
            "cargo:warning={} arquivos de assets/ embutidos no executável (single .exe).",
            files.len()
        );
    }

    // Usa o alvo real da compilação (e não o host), então cross-compile também informa certo.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        println!("cargo:warning=Alvo Windows: interface no WebView2 do sistema (já presente no Windows 10/11).");
        embed_exe_icon(&manifest_dir);
    } else {
        println!(
            "cargo:warning=Alvo {target_os}: o Wry usa WebKitGTK (pkg-config, libgtk-3-dev, libwebkit2gtk-4.1-dev)."
        );
    }
}

/// Ícone do `.exe` no Explorador do Windows. Precisa rodar no host Windows,
/// onde o `rc.exe` do toolchain MSVC está disponível; em cross-compile é ignorado.
fn embed_exe_icon(manifest_dir: &Path) {
    let icon = manifest_dir.join("assets/icons/wincode.ico");
    println!("cargo:rerun-if-changed={}", icon.display());

    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(&icon.to_string_lossy());
        if let Err(err) = resource.compile() {
            println!("cargo:warning=não foi possível embutir o ícone do .exe: {err}");
        }
    }

    #[cfg(not(windows))]
    let _ = icon;
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            println!("cargo:rerun-if-changed={}", path.display());
            collect(root, &path, out);
        } else if path.is_file() {
            let rel = match path.strip_prefix(root) {
                Ok(rel) => rel,
                Err(_) => continue,
            };
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");

            // Metadados do pacote do Monaco não são usados em runtime.
            if rel.ends_with(".md") || rel.ends_with(".d.ts") {
                continue;
            }

            out.push((rel, path));
        }
    }
}
