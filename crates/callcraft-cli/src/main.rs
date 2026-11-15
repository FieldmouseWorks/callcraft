use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use callcraft_core::CallerEngine;
use callcraft_core::report;
use callcraft_core::scenario::Scenario;
use callcraft_core::scoring::{self, Evaluation, ItemStatus};
use callcraft_core::session::{self, Session, Speaker};
use callcraft_core::validate;
use clap::{Parser, Subcommand};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const GOOD_SCRIPT: &str = include_str!("../../../scripts/good-call.yaml");
const BAD_SCRIPT: &str = include_str!("../../../scripts/bad-call.yaml");

#[derive(Parser)]
#[command(name = "callcraft", version, about = "Callcraft reference runner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a scenario file against the schema and content policy.
    Validate {
        path: PathBuf,
        /// Treat content-policy warnings as errors.
        #[arg(long)]
        strict: bool,
    },
    /// Run the canned good-call and bad-call scripts.
    Demo { path: PathBuf },
    /// Run one trainee session, scripted or interactive.
    Sim {
        path: PathBuf,
        #[arg(long)]
        script: Option<PathBuf>,
        #[arg(long)]
        interactive: bool,
        /// Write the JSON report to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Serve the authoring UI and API on loopback.
    Serve {
        #[arg(long, default_value_t = 17817)]
        port: u16,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Validate { path, strict } => validate_file(&path, strict),
        Command::Demo { path } => demo(&path),
        Command::Sim {
            path,
            script,
            interactive,
            out,
        } => sim(&path, script.as_deref(), interactive, out.as_deref()),
        Command::Serve { port } => serve(port),
    }
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn load(path: &Path) -> Result<callcraft_core::Validated> {
    let text = read(path)?;
    validate::validate_yaml(&text, false).map_err(|error| anyhow!(error.to_string()))
}

fn validate_file(path: &Path, strict: bool) -> Result<()> {
    let text = read(path)?;
    match validate::validate_yaml(&text, strict) {
        Ok(validated) => {
            for warning in &validated.warnings {
                eprintln!("warning: {warning}");
            }
            println!(
                "OK: {} — {} ({} facts, {} rubric items)",
                validated.scenario.id,
                validated.scenario.title,
                validated.scenario.facts.len(),
                validated.scenario.rubric.items.len()
            );
            Ok(())
        }
        Err(error) => Err(anyhow!(error.to_string())),
    }
}

fn demo(path: &Path) -> Result<()> {
    let validated = load(path)?;
    let scenario = &validated.scenario;

    let mut good_passed = false;
    let mut bad_failed = false;

    for (name, yaml) in [("good-call", GOOD_SCRIPT), ("bad-call", BAD_SCRIPT)] {
        let script = session::parse_script(yaml).map_err(|error| anyhow!(error.to_string()))?;
        let session = session::run_scripted(scenario, &script, now_iso())
            .map_err(|error| anyhow!(error.to_string()))?;
        let evaluation = scoring::evaluate(scenario, &session);

        println!("=== {name} ===");
        print_summary(scenario, &session, &evaluation);

        if name == "good-call" {
            good_passed = evaluation.overall == ItemStatus::Pass;
        } else {
            bad_failed = evaluation.overall == ItemStatus::Fail;
        }
    }

    if !good_passed || !bad_failed {
        return Err(anyhow!(
            "demo sanity failed: good call did not pass or bad call did not fail"
        ));
    }

    Ok(())
}

fn sim(
    path: &Path,
    script_path: Option<&Path>,
    interactive: bool,
    out: Option<&Path>,
) -> Result<()> {
    let validated = load(path)?;
    let scenario = &validated.scenario;

    let session = if interactive {
        interactive_session(scenario)?
    } else {
        let script_path = script_path
            .ok_or_else(|| anyhow!("--script is required unless --interactive is set"))?;
        let script_yaml = read(script_path)?;
        let script =
            session::parse_script(&script_yaml).map_err(|error| anyhow!(error.to_string()))?;
        session::run_scripted(scenario, &script, now_iso())
            .map_err(|error| anyhow!(error.to_string()))?
    };

    let evaluation = scoring::evaluate(scenario, &session);
    print_summary(scenario, &session, &evaluation);

    let log = callcraft_core::to_jsonl(&session, scenario);
    let built = report::build(scenario, &session, evaluation, &log);

    if let Some(out) = out {
        std::fs::write(out, format!("{}\n", report::to_json(&built)))
            .with_context(|| format!("writing {}", out.display()))?;
        println!("report written: {}", out.display());
    }

    Ok(())
}

fn interactive_session(scenario: &Scenario) -> Result<Session> {
    println!("{} — {}", scenario.title, scenario.id);
    println!("{}", scenario.safety.disclaimer);
    println!("Type trainee lines. Commands: /event <name>, /quit");

    let mut session = Session::new(scenario, "interactive", now_iso());
    let mut engine = CallerEngine::new(scenario);
    let start = Instant::now();

    loop {
        print!("trainee> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "/quit" || line == "/exit" {
            break;
        }
        if let Some(event) = line.strip_prefix("/event ") {
            let event = event.trim();
            if !event.is_empty() {
                session.add_event(event.to_string());
                engine.note_event(event);
                println!("[event recorded: {event}]");
            }
            continue;
        }

        let t_ms = start.elapsed().as_millis() as u64;
        session.add_utterance(Speaker::Trainee, line.to_string(), t_ms, Vec::new());
        let reply = engine.respond(line, t_ms as f64 / 1000.0);
        println!("caller> {}", reply.text);
        session.add_utterance(
            Speaker::Caller,
            reply.text,
            t_ms.saturating_add(1),
            reply.disclosed,
        );
    }

    Ok(session)
}

fn serve(port: u16) -> Result<()> {
    let token = uuid::Uuid::new_v4().to_string();
    let (actual_port, handle) = callcraft_server::start(port, token.clone())?;
    println!("Listening on http://127.0.0.1:{actual_port}/?token={token}");
    println!("Loopback only. Press Ctrl-C to stop.");
    handle
        .join()
        .map_err(|_| anyhow!("server thread panicked"))?;
    Ok(())
}

fn print_summary(scenario: &Scenario, session: &Session, evaluation: &Evaluation) {
    println!();
    println!("Scenario: {} — {}", scenario.id, scenario.title);
    println!(
        "Mode: {} | turns: {} | events: {} | caller drift: {}",
        session.mode,
        session.transcript().count(),
        session.events().count(),
        session.drift_rejections
    );
    println!("Rubric:");
    for item in &evaluation.items {
        let label = match item.result {
            ItemStatus::Pass => "PASS",
            ItemStatus::Fail => "FAIL",
            ItemStatus::NeedsHuman => "NEEDS_HUMAN",
        };
        let evidence = if item.evidence.is_empty() {
            String::new()
        } else {
            format!(" evidence: {}", item.evidence.join(","))
        };
        println!(
            "  {:<12} {}{}{}",
            label,
            item.id,
            if item.critical { " [critical]" } else { "" },
            evidence
        );
    }
    if !evaluation.critical_failures.is_empty() {
        println!(
            "Critical failures: {}",
            evaluation.critical_failures.join(", ")
        );
    }
    println!(
        "Overall: {}",
        match evaluation.overall {
            ItemStatus::Pass => "PASS",
            ItemStatus::Fail => "FAIL",
            ItemStatus::NeedsHuman => "NEEDS_HUMAN",
        }
    );
}
