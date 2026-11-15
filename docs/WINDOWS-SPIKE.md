# Windows spike checklist

Purpose: prove the desktop shell works on the pilot's target platform before
voice work. This is a technical spike, not the pilot. Record what actually
happens; a failed check with a clear error is a successful spike.

## Exit criteria

All of these must pass on a real Windows 10/11 machine:

1. The app launches, and its webview loads the loopback UI.
2. Diagnostics reports a secure context.
3. Tauri IPC returns the app version from the loopback page.
4. The microphone permission prompt appears, and granting it yields a live
   track (which is immediately stopped).
5. Denying the microphone produces a clear, non-crashing error.
6. A session runs, scores, and downloads a report.
7. The authoring form loads, validates, renders, and feeds the runner.
8. No Windows firewall prompt appears.

## Build options

**A. CI artifact (preferred).** The Windows CI job runs
`cargo test --workspace --exclude callcraft-app`, builds `callcraft-app` in release mode,
and uploads `target/release/callcraft-desktop.exe` as the `callcraft-windows` artifact.

**B. Local build.** Requires stable Rust, Visual Studio Build Tools with the
C++ workload, and the WebView2 runtime:
`cargo build --release -p callcraft-app`

The executable is `target\release\callcraft-desktop.exe`. It is unsigned; expect
SmartScreen or AppLocker friction. That is a result to record, not a bug.

## Preflight on the machine

- Windows version (`winver`) and architecture.
- WebView2 runtime version:
  ```bat
  reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv
  ```
- Whether the machine has AppLocker or WDAC policy (ask IT; do not disable it).
- Microphone privacy setting: Settings → Privacy & security → Microphone.

## Run

1. Copy `callcraft-desktop.exe` to the machine and run it.
   Record: SmartScreen / antivirus / AppLocker behavior, exact dialog text.
2. Confirm the window opens and the UI loads (pane title "Open Dispatch
   Trainer", scenario panel visible).
3. Open **Diagnostics** → **Run checks**. Record every line:
   - origin (expect `http://127.0.0.1:<port>`)
   - secure context (expect `true`)
   - tauri ipc (expect `Callcraft 0.1.0 (windows)`)
   - microphone (expect `granted — <device label> (live)` after allowing)
   - user agent (record the WebView2/Edge version)
4. Restart the app, attempt the microphone check, and **deny** the prompt.
   Confirm the output is a named error, not a crash.
5. Load the example, start a session, send five turns, record an event, finish,
   download the report, and open the JSON. Confirm evidence fields are set.
6. Author tab: load the example, change the title, validate, render, **Use in
   runner**, start a session, and confirm the new title appears.
7. Close the app. Confirm the process exits and no orphan `callcraft-desktop.exe` or
   listener on the loopback port remains.

## Results

| Check | Expected | Observed | Pass |
|---|---|---|---|
| Launch | Window opens | | |
| UI loads | Scenario panel visible | | |
| Secure context | true | | |
| Tauri IPC | version string | | |
| Mic allow | live track, stopped | | |
| Mic deny | named error | | |
| Session score | PASS/FAIL rendered | | |
| Report download | JSON file opens | | |
| Author round-trip | title changes in session | | |
| Firewall | no prompt | | |

## Known failure modes and fixes

- **SmartScreen blocks the unsigned exe.** For the spike only: More info → Run
  anyway. Production requires an Authenticode certificate; budget it now.
- **AppLocker/WDAC blocks the exe.** Do not work around it. The agency must
  allow-list a signed binary, or the pilot uses `callcraft serve` in a browser.
- **WebView2 runtime missing.** Install the Evergreen runtime (per-machine
  install may need admin; per-user is an option). Record which was used.
- **Microphone denied by Windows privacy settings.** The webview error may be
  unhelpful; enable the OS setting and retest.
- **Tauri IPC denied from the loopback origin.** Check the `remote` capability
  in `crates/callcraft-app/capabilities/default.json` and record the exact error.
- **Firewall prompt.** The server binds `127.0.0.1` only; a prompt means
  something is wrong. Record it.
- **Antivirus quarantine.** Record the vendor, signature, and detection name.

## Not covered by this spike

- Installer packaging and signing (`bundle.active` is false).
- Voice latency, cost, and caller-model behavior (no voice adapter yet).
- Hosted or multi-user deployment.
- The real pilot's authoring-time and grading-agreement measurements.
