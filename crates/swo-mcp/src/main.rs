//! `swo-mcp` — MCP server over the Observatory's local cache, plus a built-in
//! driver for a local language model (Ollama).
//!
//! Default cache PATH is the app's own:
//!   macOS   ~/Library/Application Support/SpaceWeatherObservatory/cache/observatory.sqlite3
//!   Windows %APPDATA%\SpaceWeatherObservatory\cache\observatory.sqlite3
//! `SWO_DB` overrides the default; `--db` overrides both. Reports are saved in
//! `reports/` beside the cache directory (`SWO_REPORTS` / `--reports`).
//!
//! In `serve` mode stdout carries the MCP protocol, so all diagnostics go to stderr.

use rmcp::{transport::stdio, ServiceExt};
use std::path::PathBuf;
use std::time::Duration;
use swo_mcp::{ollama, SwoServer};

const USAGE: &str = "\
usage: swo-mcp [COMMAND] [OPTIONS]

commands:
  serve                 run the MCP server on stdio (default)
  report                have the local model write the plain-language report, if out of date
  ask \"QUESTION\"        ask the local model about the dashboard or any reading
  dashboard             print the current dashboard readings as JSON

options:
  --db PATH             cache to read (default: the desktop app's cache; env SWO_DB)
  --reports DIR         where reports are saved (default: reports/ beside the cache; env SWO_REPORTS)
  --model NAME          Ollama model (default: env SWO_MODEL, else picked from installed models)
  --watch [SECONDS]     report: keep running and rewrite the report whenever the data
                        changes or it grows stale (checks every 300 s by default)
  --force               report: rewrite even if the saved report is current
  -h, --help            show this help

The local model is reached at OLLAMA_HOST (default http://127.0.0.1:11434).";

struct Args {
    command: String,
    question: Option<String>,
    db: Option<PathBuf>,
    reports: Option<PathBuf>,
    model: Option<String>,
    watch: Option<u64>,
    force: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        command: "serve".into(),
        question: None,
        db: std::env::var_os("SWO_DB").map(PathBuf::from),
        reports: std::env::var_os("SWO_REPORTS").map(PathBuf::from),
        model: None,
        watch: None,
        force: false,
    };
    let mut args = std::env::args().skip(1).peekable();
    let mut positional = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => a.db = Some(args.next().map(PathBuf::from).ok_or("--db needs a path")?),
            "--reports" => {
                a.reports = Some(
                    args.next()
                        .map(PathBuf::from)
                        .ok_or("--reports needs a directory")?,
                )
            }
            "--model" => a.model = Some(args.next().ok_or("--model needs a name")?),
            "--watch" => {
                // The interval is optional: take the next argument only if it is a number.
                let secs = args.peek().and_then(|s| s.parse::<u64>().ok());
                if secs.is_some() {
                    args.next();
                }
                a.watch = Some(secs.unwrap_or(300).max(30));
            }
            "--force" => a.force = true,
            "-h" | "--help" => {
                eprintln!("{USAGE}");
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option {other:?}\n\n{USAGE}"))
            }
            _ => positional.push(arg),
        }
    }
    let mut positional = positional.into_iter();
    if let Some(command) = positional.next() {
        a.command = command;
    }
    let rest: Vec<String> = positional.collect();
    if !rest.is_empty() {
        a.question = Some(rest.join(" "));
    }
    Ok(a)
}

fn default_db() -> Option<PathBuf> {
    // Same resolution as `app_dirs()` in src-tauri/src/lib.rs.
    dirs::data_dir().map(|d| {
        d.join("SpaceWeatherObservatory")
            .join("cache")
            .join("observatory.sqlite3")
    })
}

async fn run() -> Result<(), String> {
    let args = parse_args()?;
    let db = args
        .db
        .or_else(default_db)
        .ok_or("no per-user data directory; pass --db")?;
    // <data>/SpaceWeatherObservatory/cache/observatory.sqlite3 -> <data>/SpaceWeatherObservatory/reports
    let reports = args.reports.unwrap_or_else(|| {
        db.parent()
            .and_then(|cache| cache.parent())
            .unwrap_or(std::path::Path::new("."))
            .join("reports")
    });
    let server = SwoServer::open(&db)?.with_reports_dir(&reports);

    match args.command.as_str() {
        "serve" => {
            eprintln!(
                "swo-mcp: serving {} (cache read-only; reports in {})",
                db.display(),
                reports.display()
            );
            let running = server
                .serve(stdio())
                .await
                .map_err(|e| format!("MCP startup failed: {e}"))?;
            running
                .waiting()
                .await
                .map_err(|e| format!("MCP server stopped: {e}"))?;
        }
        "dashboard" => {
            let json =
                serde_json::to_string_pretty(&server.dashboard()?).map_err(|e| e.to_string())?;
            println!("{json}");
        }
        "ask" => {
            let question = args
                .question
                .ok_or("ask needs a question, e.g. swo-mcp ask \"What does Bz mean?\"")?;
            let mut llm = ollama::Ollama::new(args.model)?;
            eprintln!("swo-mcp: asking {} at {}", llm.model().await?, llm.host());
            println!("{}", ollama::ask(&server, &mut llm, &question).await?);
        }
        "report" => {
            let mut llm = ollama::Ollama::new(args.model)?;
            eprintln!(
                "swo-mcp: using {} at {}; reports in {}",
                llm.model().await?,
                llm.host(),
                reports.display()
            );
            let mut force = args.force;
            loop {
                match ollama::refresh_report(&server, &mut llm, force).await {
                    Ok(Some(reasons)) => {
                        eprintln!("swo-mcp: report rewritten ({})", reasons.join("; "))
                    }
                    Ok(None) => eprintln!("swo-mcp: report is current"),
                    // While watching, a failed attempt (model busy, Ollama restarting) is retried next round.
                    Err(e) if args.watch.is_some() => eprintln!("swo-mcp: {e}; will retry"),
                    Err(e) => return Err(e),
                }
                force = false;
                let Some(secs) = args.watch else {
                    // One-shot: show the report on stdout.
                    if let Some(report) = server.report_store()?.latest()? {
                        println!("{}", report.markdown);
                    }
                    break;
                };
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
                    _ = tokio::signal::ctrl_c() => break,
                }
            }
        }
        other => return Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("swo-mcp: {e}");
        std::process::exit(1);
    }
}
