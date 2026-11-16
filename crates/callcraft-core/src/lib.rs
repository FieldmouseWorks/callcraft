//! Callcraft core.
//!
//! Pure logic only: no file IO, no network, no clock. The CLI, HTTP server and
//! Tauri shell are thin layers over this crate. That keeps replay and scoring
//! deterministic and testable.

pub mod caller;
pub mod eventlog;
pub mod pack;
pub mod report;
pub mod scenario;
pub mod scoring;
pub mod session;
pub mod validate;

pub use caller::{CallerEngine, CallerTurn};
pub use eventlog::{replay, to_jsonl};
pub use pack::{PackManifest, safe_id};
pub use report::Report;
pub use scenario::{FORMAT_ID, FORMAT_VERSION, Scenario};
pub use scoring::{Evaluation, ItemResult, ItemStatus, evaluate};
pub use session::{
    Script, ScriptTurn, Session, SessionError, SessionItem, Speaker, Utterance, parse_script,
    run_scripted,
};
pub use validate::{Validated, ValidationError, render_yaml, validate_value, validate_yaml};
