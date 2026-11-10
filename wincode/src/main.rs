// No Windows o executável é uma aplicação gráfica: nenhuma janela de console é criada.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod assets;
mod config;
mod dialog;
mod fonts;
mod fsapi;
mod index;
mod jobs;
mod protocol;
mod recent;
mod search;
mod session;
mod settings;
mod term;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use wry::application::event::{Event, StartCause, WindowEvent};
use wry::application::event_loop::{ControlFlow, EventLoop};
use wry::application::window::WindowBuilder;
use wry::webview::WebViewBuilder;
#[cfg(target_os = "windows")]
use wry::webview::WebViewBuilderExtWindows;

/// Pasta (e arquivo, quando o argumento é um arquivo) inicial: `WINCODE_ROOT`,
/// primeiro argumento ou tela de seleção.
fn root_from_args() -> Option<(PathBuf, Option<PathBuf>)> {
    if let Ok(dir) = std::env::var("WINCODE_ROOT") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return Some((path, None));
        }
    }

    if let Some(arg) = std::env::args().nth(1) {
        // `absolute` normaliza sem resolver links e sem o prefixo `\\?\` do Windows.
        let path = std::path::absolute(&arg).unwrap_or_else(|_| PathBuf::from(&arg));
        if path.is_dir() {
            return Some((path, None));
        }
        if path.is_file() {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(|parent| parent.to_path_buf())
                .or_else(|| std::env::current_dir().ok())?;
            return Some((parent, Some(path)));
        }
    }

    None
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
    let opened = root_from_args();
    let (root, file) = match opened {
        Some((root, file)) => (Some(root), file),
        None => (None, None),
    };

    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Wincode")
        .with_inner_size(wry::application::dpi::LogicalSize::new(1280.0, 800.0))
        .with_window_icon(assets::icon())
        .build(&event_loop)
        .map_err(|err| format!("não foi possível criar a janela: {err}"))?;

    let assets = Arc::new(assets::Assets::new());
    let state = Arc::new(Mutex::new(fsapi::FsState::new(root, file)));
    let jobs = Arc::new(jobs::Jobs::new());
    let settings = Arc::new(settings::Settings::new());
    let fonts = Arc::new(fonts::Fonts::new());
    let recent = Arc::new(recent::Recent::new());
    let sessions = Arc::new(session::Sessions::new());

    // O que veio pela CLI também entra na lista de recentes da tela inicial.
    {
        let guard = state.lock().unwrap();
        if guard.selected {
            match &guard.open_file {
                Some(file) => recent.touch(file, "file"),
                None => recent.touch(&guard.root, "dir"),
            }
        }
    }

    let builder = WebViewBuilder::new(window)
        .map_err(|err| format!("não foi possível inicializar o WebView: {err}"))?
        .with_custom_protocol("editor".into(), {
            let assets = assets.clone();
            let state = state.clone();
            let jobs = jobs.clone();
            let settings = settings.clone();
            let fonts = fonts.clone();
            let recent = recent.clone();
            let sessions = sessions.clone();
            move |request| {
                protocol::serve(
                    &assets, &state, &jobs, &settings, &fonts, &recent, &sessions, request,
                )
            }
        });

    // Sem atalhos do navegador: Ctrl+= / Ctrl+- / Ctrl+0 ficam com o editor em vez
    // de darem zoom (ou abrirem o menu do Edge) por cima da interface.
    #[cfg(target_os = "windows")]
    let builder = builder.with_browser_accelerator_keys(false);

    let _webview = builder
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
