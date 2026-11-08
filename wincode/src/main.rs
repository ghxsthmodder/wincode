// No Windows o executável é uma aplicação gráfica: nenhuma janela de console é criada.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod assets;
mod dialog;
mod fsapi;
mod jobs;
mod protocol;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use wry::application::event::{Event, StartCause, WindowEvent};
use wry::application::event_loop::{ControlFlow, EventLoop};
use wry::application::window::WindowBuilder;
use wry::webview::WebViewBuilder;

/// Pasta aberta: `WINCODE_ROOT`, primeiro argumento ou diretório atual.
fn root_from_args() -> PathBuf {
    if let Ok(dir) = std::env::var("WINCODE_ROOT") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return path;
        }
    }

    if let Some(arg) = std::env::args().nth(1) {
        let path = PathBuf::from(arg);
        if path.is_dir() {
            return path;
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn main() {
    if let Err(message) = run() {
        report_fatal(&message);
        std::process::exit(1);
    }
}

/// Sem console no Windows, um `eprintln!` se perderia: mostra um diálogo nativo.
fn report_fatal(message: &str) {
    #[cfg(target_os = "windows")]
    {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Wincode")
            .set_description(message)
            .show();
    }

    #[cfg(not(target_os = "windows"))]
    eprintln!("erro fatal: {message}");
}

fn run() -> Result<(), String> {
    let root = root_from_args();

    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Wincode")
        .with_inner_size(wry::application::dpi::LogicalSize::new(1280.0, 800.0))
        .build(&event_loop)
        .map_err(|err| format!("não foi possível criar a janela: {err}"))?;

    let assets = Arc::new(assets::Assets::new());
    let state = Arc::new(Mutex::new(fsapi::FsState::new(root)));
    let jobs = Arc::new(jobs::Jobs::new());

    let _webview = WebViewBuilder::new(window)
        .map_err(|err| format!("não foi possível inicializar o WebView: {err}"))?
        .with_custom_protocol("editor".into(), {
            let assets = assets.clone();
            let state = state.clone();
            let jobs = jobs.clone();
            move |request| protocol::serve(&assets, &state, &jobs, request)
        })
        .with_url("editor://localhost/index.html")
        .map_err(|err| format!("URL inicial inválida: {err}"))?
        .build()
        .map_err(|err| format!("não foi possível criar o WebView: {err}"))?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {}
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => (),
        }
    })
}
