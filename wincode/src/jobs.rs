use crate::term::Decoder;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

struct Job {
    output: Arc<Mutex<String>>,
    finished: Arc<Mutex<Option<i32>>>,
}

/// Comandos do terminal rodam em threads próprias: o handler do protocolo
/// responde na thread da interface, então um `cargo build` não congela a janela.
pub(super) struct Jobs {
    inner: Mutex<HashMap<u64, Job>>,
    next_id: Mutex<u64>,
}

impl Jobs {
    pub(super) fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            next_id: Mutex::new(0),
        }
    }

    pub(super) fn spawn(&self, cmd: &str, cwd: &Path) -> Result<u64, String> {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return Err("comando vazio".into());
        }

        let mut child = shell(cmd)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("falha ao executar: {e}"))?;

        let output = Arc::new(Mutex::new(String::new()));
        let finished = Arc::new(Mutex::new(None));

        let mut readers = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            let buffer = output.clone();
            readers.push(thread::spawn(move || read_stream(&mut stdout, buffer)));
        }
        if let Some(mut stderr) = child.stderr.take() {
            let buffer = output.clone();
            readers.push(thread::spawn(move || read_stream(&mut stderr, buffer)));
        }

        let done = finished.clone();
        thread::spawn(move || {
            let code = child.wait().ok().and_then(|status| status.code()).unwrap_or(-1);
            for reader in readers {
                let _ = reader.join();
            }
            *done.lock().unwrap() = Some(code);
        });

        let mut next = self.next_id.lock().unwrap();
        *next += 1;
        let id = *next;
        drop(next);

        self.inner.lock().unwrap().insert(
            id,
            Job {
                output,
                finished,
            },
        );

        Ok(id)
    }

    /// Devolve a saída produzida desde o último poll e o código de saída (se terminou).
    pub(super) fn poll(&self, id: u64) -> Result<(String, Option<i32>), String> {
        let mut jobs = self.inner.lock().unwrap();

        let (out, code) = match jobs.get(&id) {
            Some(job) => {
                let out = std::mem::take(&mut *job.output.lock().unwrap());
                let code = *job.finished.lock().unwrap();
                (out, code)
            }
            None => return Err("comando não encontrado".into()),
        };

        if code.is_some() {
            jobs.remove(&id);
        }

        Ok((out, code))
    }
}

fn read_stream<R: Read>(stream: &mut R, buffer: Arc<Mutex<String>>) {
    let mut chunk = [0u8; 8192];
    let mut decoder = Decoder::new();

    // O texto decodificado entra cru: `\r` (barras de progresso) e `\n` são
    // interpretados pela interface, que reescreve a linha em vez de empilhar
    // caracteres soltos.
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let text = decoder.push(&chunk[..n]);
                if !text.is_empty() {
                    buffer.lock().unwrap().push_str(&text);
                }
            }
            Err(_) => break,
        }
    }

    let tail = decoder.finish();
    if !tail.is_empty() {
        buffer.lock().unwrap().push_str(&tail);
    }
}

#[cfg(target_os = "windows")]
fn shell(cmd: &str) -> Command {
    use std::os::windows::process::CommandExt;

    // O editor é um executável GUI (sem console próprio). Sem esta flag, cada
    // comando do terminal abriria uma janela de `cmd.exe` sobre a interface;
    // CREATE_NO_WINDOW mantém o processo sem janela e a saída ainda é capturada
    // pelos pipes de stdout/stderr.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // `CREATE_NO_WINDOW` cria um console invisível, então `chcp 65001` vale para
    // a saída do próprio cmd e das ferramentas que consultam a página de código
    // do console (PowerShell 5.1, Python): o texto chega em UTF-8, com acentos.
    // Se o console não existir, o `chcp` falha em silêncio e o decodificador
    // (`term.rs`) cai para Latin-1.
    let mut command = Command::new("cmd.exe");
    command
        .arg("/C")
        .arg(format!("chcp 65001>nul 2>nul & {cmd}"));
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(target_os = "windows"))]
fn shell(cmd: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(cmd);
    command
}
