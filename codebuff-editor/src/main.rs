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

/// Pasta aberta: `CODEBUFF_EDITOR_ROOT`, primeiro argumento ou diretório atual.
fn root_from_args() -> PathBuf {
    if let Ok(dir) = std::env::var("CODEBUFF_EDITOR_ROOT") {
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

fn main() -> wry::Result<()> {
    let root = root_from_args();

    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("CodeBuff Editor")
        .with_inner_size(wry::application::dpi::LogicalSize::new(1280.0, 800.0))
        .build(&event_loop)
        .expect("não foi possível criar a janela");

    let assets = Arc::new(assets::Assets::new());
    let state = Arc::new(Mutex::new(fsapi::FsState::new(root)));
    let jobs = Arc::new(jobs::Jobs::new());

    let _webview = WebViewBuilder::new(window)?
        .with_custom_protocol("editor".into(), {
            let assets = assets.clone();
            let state = state.clone();
            let jobs = jobs.clone();
            move |request| protocol::serve(&assets, &state, &jobs, request)
        })
        .with_url("editor://localhost/index.html")?
        .build()?;

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
