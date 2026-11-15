const token = new URLSearchParams(location.search).get("token") || "";
const $ = (id) => document.getElementById(id);

const scenarioInput = $("scenario");
const scenarioOutput = $("scenario-output");
const statusEl = $("status");
const sessionPanel = $("session-panel");
const transcriptEl = $("transcript");
const eventBar = $("event-bar");
const turnInput = $("turn-input");
const recordButton = $("record");
const voiceMeta = $("voice-meta");
const callerAudio = $("caller-audio");
const scorecard = $("scorecard");

let sessionId = null;
let scenarioId = null;
let recordedEvents = new Set();
let report = null;
let finished = false;
let recorder = null;
let chunks = [];

function setStatus(text, kind = "") {
  statusEl.textContent = text;
  statusEl.className = `status ${kind}`;
}

function setSessionStatus(text, kind = "") {
  const el = $("session-meta");
  el.textContent = text;
  el.className = `meta ${kind}`;
}

async function api(path, options = {}) {
  const headers = { ...(options.headers || {}) };
  if (token) headers["X-Callcraft-Token"] = token;
  const response = await fetch(path, { ...options, headers });
  let data;
  try {
    data = await response.json();
  } catch {
    data = { ok: false, error: `HTTP ${response.status}` };
  }
  if (!response.ok || data.ok === false) {
    throw new Error(data.error || `HTTP ${response.status}`);
  }
  return data;
}

function bubble(kind, text) {
  const div = document.createElement("div");
  div.className = `bubble ${kind}`;
  const label = document.createElement("span");
  label.className = "speaker";
  label.textContent = kind === "trainee" ? "Trainee" : kind === "caller" ? "Caller" : "System";
  const body = document.createElement("div");
  body.textContent = text;
  div.append(label, body);
  transcriptEl.appendChild(div);
  transcriptEl.scrollTop = transcriptEl.scrollHeight;
}

function renderEvents(events) {
  eventBar.innerHTML = "";
  for (const name of events) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "chip";
    button.textContent = name;
    button.addEventListener("click", async () => {
      if (finished || recordedEvents.has(name)) return;
      button.disabled = true;
      try {
        await api(`/api/sessions/${sessionId}/events`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ name }),
        });
        recordedEvents.add(name);
        bubble("system", `event recorded: ${name}`);
        setSessionStatus("Event recorded", "ok");
      } catch (error) {
        button.disabled = false;
        setSessionStatus(error.message, "bad");
      }
    });
    eventBar.appendChild(button);
  }
}

async function loadExample() {
  setStatus("Loading example...");
  try {
    const data = await api("/api/example");
    scenarioInput.value = data.yaml;
    scenarioOutput.textContent = "";
    setStatus("Example loaded", "ok");
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

async function startSession() {
  if (!scenarioInput.value.trim()) {
    setStatus("Paste a scenario first", "bad");
    return;
  }
  setStatus("Starting session...");
  $("start").disabled = true;

  try {
    const data = await api("/api/sessions", {
      method: "POST",
      headers: { "Content-Type": "text/plain; charset=utf-8" },
      body: scenarioInput.value,
    });

    sessionId = data.session_id;
    scenarioId = data.scenario.id;
    recordedEvents = new Set();
    report = null;
    finished = false;

    transcriptEl.innerHTML = "";
    scorecard.hidden = true;
    scorecard.innerHTML = "";
    $("download").disabled = true;
    $("finish").disabled = false;
    $("send").disabled = false;
    turnInput.disabled = false;
    recordButton.disabled = true;
    recordButton.textContent = "Record turn";
    refreshVoiceStatus();
    $("session-title").textContent = data.scenario.title;
    sessionPanel.hidden = false;

    renderEvents(data.events || []);
    bubble("system", data.scenario.disclaimer);
    bubble("system", `Session ${sessionId.slice(0, 8)} started.`);
    for (const warning of data.warnings || []) bubble("system", `warning: ${warning}`);

    setStatus("Session running", "ok");
    setSessionStatus("In progress");
    turnInput.focus();
  } catch (error) {
    $("start").disabled = false;
    setStatus(error.message, "bad");
    scenarioOutput.textContent = error.message;
  }
}

async function sendTurn() {
  const text = turnInput.value.trim();
  if (!text || !sessionId || finished) return;
  turnInput.value = "";
  bubble("trainee", text);

  try {
    const data = await api(`/api/sessions/${sessionId}/turns`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text }),
    });
    bubble("caller", data.caller);
    setSessionStatus("In progress");
  } catch (error) {
    setSessionStatus(error.message, "bad");
  }
  turnInput.focus();
}

async function refreshVoiceStatus() {
  try {
    const status = await api("/api/voice/status");
    if (status.stt) {
      recordButton.disabled = finished;
      voiceMeta.textContent = "Voice ready";
      voiceMeta.className = "meta ok";
    } else {
      recordButton.disabled = true;
      voiceMeta.textContent = "Voice not configured — text only";
      voiceMeta.className = "meta";
    }
  } catch (error) {
    recordButton.disabled = true;
    voiceMeta.textContent = error.message;
    voiceMeta.className = "meta bad";
  }
}

async function toggleRecording() {
  if (finished) return;
  if (recorder && recorder.state === "recording") {
    recorder.stop();
    return;
  }
  if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
    voiceMeta.textContent = "Recording is unavailable in this webview";
    voiceMeta.className = "meta bad";
    return;
  }
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    chunks = [];
    recorder = new MediaRecorder(stream);
    recorder.ondataavailable = (event) => {
      if (event.data.size) chunks.push(event.data);
    };
    recorder.onstop = async () => {
      stream.getTracks().forEach((track) => track.stop());
      const blob = new Blob(chunks, { type: recorder.mimeType || "audio/webm" });
      await sendVoice(blob);
    };
    recorder.start();
    recordButton.textContent = "Stop recording";
    voiceMeta.textContent = "Recording...";
    voiceMeta.className = "meta warn";
  } catch (error) {
    voiceMeta.textContent = `microphone: ${error.name}: ${error.message}`;
    voiceMeta.className = "meta bad";
  }
}

async function sendVoice(blob) {
  recordButton.disabled = true;
  voiceMeta.textContent = "Transcribing...";
  voiceMeta.className = "meta";
  try {
    const response = await fetch(`/api/sessions/${sessionId}/voice-turn`, {
      method: "POST",
      headers: {
        "Content-Type": blob.type || "audio/webm",
        "X-Callcraft-Token": token,
      },
      body: blob,
    });
    let data;
    try {
      data = await response.json();
    } catch {
      data = { ok: false, error: `HTTP ${response.status}` };
    }
    if (!response.ok || data.ok === false) {
      throw new Error(data.error || `HTTP ${response.status}`);
    }
    bubble("trainee", data.transcript);
    bubble("caller", data.caller);
    if (data.audio_base64) {
      callerAudio.hidden = false;
      callerAudio.src = `data:${data.audio_mime || "audio/wav"};base64,${data.audio_base64}`;
      callerAudio.play().catch(() => {});
    }
    voiceMeta.textContent = data.audio_error ? "Reply text only (TTS failed)" : "Voice ready";
    voiceMeta.className = data.audio_error ? "meta warn" : "meta ok";
  } catch (error) {
    voiceMeta.textContent = error.message;
    voiceMeta.className = "meta bad";
  }
  recordButton.textContent = "Record turn";
  recordButton.disabled = finished;
  recorder = null;
}

async function finishSession() {
  if (!sessionId || finished) return;
  setStatus("Scoring...");
  try {
    const data = await api(`/api/sessions/${sessionId}/finish`, { method: "POST" });
    report = data.report;
    finished = true;
    if (recorder && recorder.state === "recording") recorder.stop();
    $("finish").disabled = true;
    $("send").disabled = true;
    $("record").disabled = true;
    turnInput.disabled = true;
    document.querySelectorAll(".chip").forEach((chip) => {
      chip.disabled = true;
    });
    renderScorecard(report);
    $("download").disabled = false;
    setStatus("Scored", "ok");
    setSessionStatus(
      report.evaluation.overall.toUpperCase(),
      report.evaluation.overall === "pass" ? "ok" : "bad"
    );
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

function renderScorecard(data) {
  const evaluation = data.evaluation;
  const rows = evaluation.items
    .map((item) => {
      const label =
        item.result === "pass" ? "PASS" : item.result === "fail" ? "FAIL" : "NEEDS HUMAN";
      const evidence = (item.evidence || []).join(", ");
      return `<tr>
        <td class="result ${item.result}">${label}</td>
        <td>${escapeHtml(item.id)}${item.critical ? ' <span class="critical">critical</span>' : ""}</td>
        <td>${escapeHtml(evidence)}</td>
      </tr>`;
    })
    .join("");

  const failures = evaluation.critical_failures.length
    ? `<p class="failures">Critical failures: ${escapeHtml(evaluation.critical_failures.join(", "))}</p>`
    : `<p class="clean">No critical failures.</p>`;

  scorecard.innerHTML = `
    <h3>Overall: <span class="result ${evaluation.overall}">${evaluation.overall.toUpperCase()}</span></h3>
    <table>
      <thead><tr><th>Result</th><th>Item</th><th>Evidence</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
    ${failures}
    <p class="meta">Session ${escapeHtml(data.session_id.slice(0, 12))} · cost $${data.cost.total_usd.toFixed(2)} · drift ${evaluation.drift_rejections}</p>
  `;
  scorecard.hidden = false;
}

function escapeHtml(value) {
  return String(value ?? "").replace(
    /[&<>"']/g,
    (character) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[character])
  );
}

function downloadReport() {
  if (!report) return;
  const blob = new Blob([JSON.stringify(report, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${scenarioId || "callcraft"}-${report.session_id.slice(0, 8)}.report.json`;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

$("load-example").addEventListener("click", loadExample);
$("start").addEventListener("click", startSession);
$("send").addEventListener("click", sendTurn);
$("record").addEventListener("click", toggleRecording);
$("finish").addEventListener("click", finishSession);
$("download").addEventListener("click", downloadReport);

$("file").addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  if (!file) return;
  scenarioInput.value = await file.text();
  setStatus(`Loaded ${file.name}`, "ok");
});

turnInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    sendTurn();
  }
});

const diagnosticsOutput = $("diagnostics-output");
const diagnosticsMeta = $("diagnostics-meta");

async function runDiagnostics() {
  diagnosticsMeta.textContent = "Running...";
  diagnosticsMeta.className = "meta";
  diagnosticsOutput.textContent = "";

  const lines = [];
  lines.push(`origin: ${location.origin}`);
  lines.push(`secure context: ${window.isSecureContext}`);
  lines.push(`media devices: ${Boolean(navigator.mediaDevices?.getUserMedia)}`);
  lines.push(`user agent: ${navigator.userAgent}`);

  if (window.__TAURI__?.core?.invoke) {
    try {
      const info = await window.__TAURI__.core.invoke("app_info");
      lines.push(`tauri ipc: ${info}`);
    } catch (error) {
      lines.push(`tauri ipc error: ${error}`);
    }
  } else {
    lines.push("tauri ipc: not in desktop shell (browser mode)");
  }

  if (navigator.mediaDevices?.getUserMedia) {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const tracks = stream.getAudioTracks().map(
        (track) => `${track.label || "unnamed"} (${track.readyState})`
      );
      lines.push(`microphone: granted${tracks.length ? ` — ${tracks.join(", ")}` : ""}`);
      stream.getTracks().forEach((track) => track.stop());
    } catch (error) {
      lines.push(`microphone: ${error.name}: ${error.message}`);
    }
  } else {
    lines.push("microphone: unavailable in this webview");
  }

  diagnosticsOutput.textContent = lines.join("\n");
  diagnosticsMeta.textContent = "Complete";
  diagnosticsMeta.className = "meta ok";
}

$("run-diagnostics").addEventListener("click", runDiagnostics);

if (!token) {
  setStatus("Missing session token — open the URL printed by `callcraft serve`.", "bad");
  $("start").disabled = true;
}
