mod app;
mod config;
mod curl;
mod highlight;
mod http;
mod keymap;
mod model;
mod openapi;
mod theme;
mod ui;
mod vars;
mod vimext;
mod workspace;

use anyhow::{Result, bail};
use app::{App, Msg};
use clap::{Parser, Subcommand};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyEventKind,
};
use std::path::PathBuf;
use std::time::Duration;
use workspace::{Workspace, registry, slug};

#[derive(Parser)]
#[command(name = "turly", version, about = "A terminal API client with Vim keys")]
struct Cli {
    /// Workspace directory to open
    path: Option<PathBuf>,
    /// Import an OpenAPI/Swagger spec (file or URL) into a throwaway workspace and open it
    #[arg(short, long, value_name = "SPEC", conflicts_with = "path")]
    import: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create an empty workspace
    Init {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Build a workspace from an OpenAPI 3.x / Swagger 2.0 spec (file or URL)
    Import {
        spec: String,
        /// Target directory (default: ./<api-title>)
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let rt = tokio::runtime::Runtime::new()?;
    match cli.cmd {
        Some(Cmd::Init { path, name }) => {
            let name = name
                .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "workspace".into());
            let ws = Workspace::init(&path, &name)?;
            registry::add(&ws.root)?;
            println!("Created workspace '{name}' at {}", path.display());
            Ok(())
        }
        Some(Cmd::Import { spec, dir }) => rt.block_on(import_spec(&spec, dir, true)).map(|_| ()),
        None => {
            let Some(spec) = cli.import else {
                return run_tui(rt, cli.path);
            };
            let tmp = std::env::temp_dir().join(format!("turly-{}", std::process::id()));
            let res = rt
                .block_on(import_spec(&spec, Some(tmp.clone()), false))
                .and_then(|_| run_tui(rt, Some(tmp.clone())));
            let _ = std::fs::remove_dir_all(&tmp);
            res
        }
    }
}

/// Import `spec`; `persist` registers the workspace and reports it.
async fn import_spec(spec: &str, dir: Option<PathBuf>, persist: bool) -> Result<PathBuf> {
    let imp = openapi::import(&openapi::fetch(spec).await?)?;
    let dir = dir.unwrap_or_else(|| PathBuf::from(slug(&imp.title)));
    let ws = openapi::build_workspace(&dir, &imp)?;
    if persist {
        registry::add(&ws.root)?;
        println!(
            "Imported '{}': {} requests, {} environments -> {}",
            imp.title,
            imp.requests.len(),
            imp.envs.len(),
            dir.display()
        );
    }
    Ok(dir)
}

/// Workspace to open: explicit path, else cwd if it is one, else the most recently registered.
fn pick_workspace(path: Option<PathBuf>) -> Result<Option<Workspace>> {
    if let Some(p) = path {
        if !Workspace::is_workspace(&p) {
            bail!(
                "{} is not a Turly workspace (try `turly init {0}`)",
                p.display()
            );
        }
        return Ok(Some(Workspace::open(&p)?));
    }
    if let Ok(cwd) = std::env::current_dir()
        && Workspace::is_workspace(&cwd)
    {
        return Ok(Some(Workspace::open(&cwd)?));
    }
    Ok(registry::load()
        .last()
        .and_then(|p| Workspace::open(p).ok()))
}

fn run_tui(rt: tokio::runtime::Runtime, path: Option<PathBuf>) -> Result<()> {
    let ws = pick_workspace(path)?;
    let (cfg, cfg_err) = match config::Config::load() {
        Ok(c) => (c, None),
        Err(e) => (Default::default(), Some(format!("{e:#}"))),
    };
    let mouse = cfg.mouse.unwrap_or(true);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Msg>();
    let mut app = App::new(ws, cfg, rt.handle().clone(), tx);
    if let Some(e) = cfg_err {
        app.err(e);
    }

    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stdout(), EnableBracketedPaste)?;
    if mouse {
        crossterm::execute!(std::io::stdout(), EnableMouseCapture)?;
    }
    let result = run_loop(&mut terminal, &mut app, &mut rx);
    if mouse {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    }
    let _ = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    result
}

fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Msg>,
) -> Result<()> {
    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                Event::Paste(s) => app.on_paste(&s),
                Event::Mouse(m) => app.on_mouse(m),
                _ => {}
            }
        }
        while let Ok(m) = rx.try_recv() {
            app.on_msg(m);
        }
        app.run_system_editor(terminal)?;
    }
    Ok(())
}
