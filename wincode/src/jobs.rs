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
    /// Processo do `cmd.exe`: é o que permite interromper um comando em execução.
    pid: u32,
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

        let child = shell(cmd)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("falha ao executar: {e}"))?;

        Ok(self.register(child))
    }

    /// Roda um programa direto, sem shell, e devolve o mesmo `id` de poll do
    /// terminal. O painel do git usa isto para `fetch`/`pull`/`push`: a rede pode
    /// demorar, e a thread da interface não pode ficar presa esperando.
    pub(super) fn spawn_program(
        &self,
        program: &str,
        args: &[&str],
        cwd: &Path,
    ) -> Result<u64, String> {
        let mut command = Command::new(program);
        command
            .current_dir(cwd)
            .args(args)
            // Sem um console para perguntar, o git ficaria pendurado esperando
            // credencial; o erro dele é melhor do que uma janela travada.
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let child = command
            .spawn()
            .map_err(|e| format!("falha ao executar {program}: {e}"))?;

        Ok(self.register(child))
    }

    fn register(&self, mut child: std::process::Child) -> u64 {
        let pid = child.id();
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
            let code = child
                .wait()
                .ok()
                .and_then(|status| status.code())
                .unwrap_or(-1);
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
                pid,
            },
        );

        id
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

    /// Interrompe o comando (e o que ele tiver aberto). O `poll` seguinte já
    /// devolve `done` com o código de saída do processo morto.
    pub(super) fn kill(&self, id: u64) -> Result<(), String> {
        let pid = {
            let jobs = self.inner.lock().unwrap();
            jobs.get(&id)
                .map(|job| job.pid)
                .ok_or("comando não encontrado")?
        };

        kill_tree(pid)
    }
}

/// `taskkill /T` leva junto os processos filhos: `cargo build` deixa
/// compiladores rodando, e matar só o `cmd.exe` os deixaria órfãos.
#[cfg(target_os = "windows")]
fn kill_tree(pid: u32) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|err| format!("não foi possível interromper: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("o processo já havia terminado".into())
    }
}

#[cfg(not(target_os = "windows"))]
fn kill_tree(pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| format!("não foi possível interromper: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("o processo já havia terminado".into())
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
