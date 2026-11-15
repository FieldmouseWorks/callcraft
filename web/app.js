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
const briefTitle = $("brief-title");
const briefSubtitle = $("brief-subtitle");
const briefId = $("brief-id");
const briefIncident = $("brief-incident");
const briefDifficulty = $("brief-difficulty");
const briefDuration = $("brief-duration");
const briefVoice = $("brief-voice");
const briefCounts = $("brief-counts");
const sessionScenario = $("session-scenario");
const sessionApproval = $("session-approval");
const libraryList = $("scenario-library");
const libraryPack = $("library-pack");
const libraryMeta = $("library-meta");
const packPicker = $("pack-picker");
const libraryExport = $("library-export");
const libraryImport = $("library-import");
const libraryRollback = $("library-rollback");
const importReview = $("import-review");

let sessionId = null;
let scenarioId = null;
let recordedEvents = new Set();
let report = null;
let finished = false;
let recorder = null;
let chunks = [];
let sessionStartedAt = null;
let briefTimer = null;
let briefRequest = 0;
let libraryPacks = [];
let libraryWarning = "";
let pendingImportBytes = null;
let pendingImportReport = null;

function setStatus(text, kind = "") {
  statusEl.textContent = text;
  statusEl.className = `status ${kind}`;
  const dot = document.querySelector(".status-dot");
  if (dot) dot.className = `status-dot ${kind}`;
}

function setSessionStatus(text, kind = "") {
  const el = $("session-meta");
  el.textContent = text;
  el.className = `meta ${kind}`;
}

function yamlScalar(key) {
  const match = scenarioInput.value.match(new RegExp(`^${key}:\\s*(.+)$`, "m"));
  if (!match) return "";
  return match[1]
    .replace(/\s+#.*$/, "")
    .trim()
    .replace(/^["']|["']$/g, "");
}

function updateBrief() {
  const text = scenarioInput.value.trim();
  const title = yamlScalar("title") || (text ? "Untitled scenario" : "No scenario loaded");
  briefTitle.textContent = title;
  briefSubtitle.textContent = text
    ? title === "Untitled scenario"
      ? "YAML present. Add a title before authoring."
      : "Ready to start the session."
    : "Load a file or the example to see the brief.";
  briefId.textContent = yamlScalar("id") || "—";
  briefIncident.textContent = yamlScalar("incident_type") || "—";
  briefDifficulty.textContent = yamlScalar("difficulty") || "—";
  const duration = yamlScalar("duration_minutes");
  briefDuration.textContent = duration ? `${duration} min` : "—";
  $("start").disabled = !text;
  $("scenario-meta").textContent = text ? "Checking..." : "";
  scheduleBrief();
}

function scheduleBrief() {
  clearTimeout(briefTimer);
  briefTimer = setTimeout(refreshBrief, 350);
}

async function refreshBrief() {
  const text = scenarioInput.value.trim();
  const request = ++briefRequest;
  if (!text) {
    briefCounts.textContent = "";
    $("scenario-meta").textContent = "";
    return;
  }
  try {
    const data = await api("/api/scenario/parse", {
      method: "POST",
      headers: { "Content-Type": "text/plain; charset=utf-8" },
      body: text,
    });
    if (request !== briefRequest) return;
    const scenario = data.scenario || {};
    const facts = (scenario.facts || []).length;
    const items = scenario.rubric?.items || [];
    const critical = items.filter((item) => item.critical).length;
    briefTitle.textContent = scenario.title || "Untitled scenario";
    briefSubtitle.textContent = "Ready to start the session.";
    briefId.textContent = scenario.id || "—";
    briefIncident.textContent = scenario.incident_type || "—";
    briefDifficulty.textContent = scenario.difficulty || "—";
    briefDuration.textContent = scenario.duration_minutes
      ? `${scenario.duration_minutes} min`
      : "—";
    briefCounts.textContent = `${facts} facts · ${items.length} rubric items · ${critical} critical`;
    const warnings = data.warnings || [];
    $("scenario-meta").textContent = warnings.length ? `${warnings.length} warning(s)` : "Valid";
    $("start").disabled = false;
  } catch (error) {
    if (request !== briefRequest) return;
    briefCounts.textContent = "";
    $("scenario-meta").textContent = "Invalid";
    $("start").disabled = true;
  }
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

function elapsedLogTime() {
  if (!sessionStartedAt) return "00:00";
  const total = Math.floor((Date.now() - sessionStartedAt) / 1000);
  const minutes = String(Math.floor(total / 60)).padStart(2, "0");
  const seconds = String(total % 60).padStart(2, "0");
  return `${minutes}:${seconds}`;
}

function formatUtteranceTime(ms) {
  const total = Math.floor((ms || 0) / 1000);
  return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
}

function appendLogRow(kind, text, time, utteranceId) {
  const row = document.createElement("li");
  row.className = `log-row ${kind}`;
  if (utteranceId) row.dataset.utteranceId = utteranceId;
  const timeEl = document.createElement("span");
  timeEl.className = "log-time";
  timeEl.textContent = time || "";
  const speaker = document.createElement("span");
  speaker.className = "log-speaker";
  speaker.textContent = kind === "trainee" ? "Trainee" : kind === "caller" ? "Caller" : "Note";
  const body = document.createElement("p");
  body.className = "log-text";
  body.textContent = text;
  row.append(timeEl, speaker, body);
  transcriptEl.appendChild(row);
  transcriptEl.scrollTop = transcriptEl.scrollHeight;
}

function bubble(kind, text, time, utteranceId) {
  appendLogRow(kind, text, time ?? elapsedLogTime(), utteranceId);
}

function renderTranscript(utterances) {
  transcriptEl.replaceChildren();
  for (const utterance of utterances || []) {
    appendLogRow(
      utterance.speaker,
      utterance.text,
      formatUtteranceTime(utterance.t_ms),
      utterance.id
    );
  }
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
        button.classList.add("recorded");
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
    updateBrief();
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
    beginSession(data);
  } catch (error) {
    $("start").disabled = !scenarioInput.value.trim();
    setStatus(error.message, "bad");
    scenarioOutput.textContent = error.message;
  }
}

async function startPackScenario(packId, scenarioId) {
  setStatus("Starting saved scenario...");
  try {
    const data = await api("/api/sessions", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ pack_id: packId, scenario_id: scenarioId }),
    });
    beginSession(data);
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

function beginSession(data) {
  sessionId = data.session_id;
  scenarioId = data.scenario.id;
  recordedEvents = new Set();
  report = null;
  finished = false;
  sessionStartedAt = Date.now();
  document.body.classList.add("session-live");
  document.body.classList.remove("session-review");

  transcriptEl.replaceChildren();
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
  $("session-stage-label").textContent = "Run / Live call";
  const approval = data.approval || "none";
  sessionApproval.textContent = approval === "none" ? "UNAPPROVED" : approval.toUpperCase();
  sessionApproval.className = `approval-tag ${approval === "approved" ? "approved" : "unapproved"}`;
  sessionScenario.innerHTML = `
    <strong>${escapeHtml(data.scenario.title)}</strong>
    <p class="meta mono">${escapeHtml(data.scenario.id)}</p>
    <p>${escapeHtml(data.scenario.disclaimer)}</p>
  `;
  sessionPanel.hidden = false;

  renderEvents(data.events || []);
  bubble("system", data.scenario.disclaimer, "00:00");
  for (const warning of data.warnings || []) bubble("system", `warning: ${warning}`, "00:00");

  setStatus("Session running", "ok");
  setSessionStatus("In progress");
  turnInput.focus();
}

function selectedPack() {
  return libraryPacks.find((pack) => pack.id === libraryPack.value) || libraryPacks[0] || null;
}

function renderLibrary() {
  const pack = selectedPack();
  libraryExport.disabled = !pack;
  packPicker.hidden = libraryPacks.length <= 1;
  if (!pack) {
    libraryMeta.textContent = "";
    libraryList.innerHTML = `
      <div class="library-empty">
        <strong>No saved scenarios yet.</strong>
        Author a scenario and choose Save to pack. Load example gives you a synthetic call to start from.
      </div>
    `;
    return;
  }
  if (pack.error) {
    libraryMeta.textContent = "Pack error";
    libraryList.innerHTML = `<div class="library-empty"><strong>${escapeHtml(pack.name)}</strong>${escapeHtml(pack.error)}</div>`;
    return;
  }

  const scenarios = pack.scenarios || [];
  libraryMeta.textContent = `${pack.name} · ${scenarios.length} scenario${scenarios.length === 1 ? "" : "s"}`;
  if (libraryWarning) {
    libraryMeta.textContent = `${libraryWarning} · ${libraryMeta.textContent}`;
    libraryMeta.className = "meta warn";
  } else {
    libraryMeta.className = "meta";
  }
  if (!scenarios.length) {
    libraryList.innerHTML = `
      <div class="library-empty">
        <strong>${escapeHtml(pack.name)} is empty.</strong>
        Author a scenario and choose Save to pack, or load the example and test drive it.
      </div>
    `;
    return;
  }

  libraryList.innerHTML = scenarios
    .map((scenario) => {
      const difficulty = scenario.difficulty || "—";
      const duration = scenario.duration_minutes ? `${scenario.duration_minutes} min` : "—";
      const status = scenario.valid ? (scenario.status || "draft") : "invalid";
      const approved = status === "approved";
      const error = scenario.error
        ? `<p class="library-warning">${escapeHtml(scenario.error)}</p>`
        : "";
      const warnings = scenario.warnings?.length
        ? `<p class="library-warning">${scenario.warnings.length} warning(s)</p>`
        : "";
      const start = approved
        ? `<button class="primary" type="button" data-start-scenario="${escapeHtml(scenario.id)}">Start</button>`
        : `<button class="secondary" type="button" data-test-scenario="${escapeHtml(scenario.id)}">Test drive</button>`;
      const approvalAction = scenario.valid && !approved
        ? `<button class="secondary" type="button" data-approve-scenario="${escapeHtml(scenario.id)}">Approve</button>`
        : "";
      const retireAction = approved
        ? `<button class="secondary" type="button" data-retire-scenario="${escapeHtml(scenario.id)}">Retire</button>`
        : "";
      return `
        <article class="library-card">
          <div class="library-card-top">
            <div>
              <h4 class="library-title">${escapeHtml(scenario.title || scenario.id)}</h4>
              <p class="library-id">${escapeHtml(scenario.id)} · v${escapeHtml(scenario.version || "—")}</p>
            </div>
            <span class="library-status ${scenario.valid ? status : "bad"}">${escapeHtml(status)}</span>
          </div>
          <p class="library-facts">${escapeHtml(difficulty)} · ${escapeHtml(duration)} · ${scenario.facts} facts · ${scenario.rubric_items} rubric · ${scenario.critical_items} critical</p>
          ${error}${warnings}
          <div class="library-actions">
            ${start}
            <button class="secondary" type="button" data-open-scenario="${escapeHtml(scenario.id)}">Open</button>
            ${approvalAction}${retireAction}
          </div>
        </article>
      `;
    })
    .join("");

  for (const button of libraryList.querySelectorAll("[data-start-scenario]")) {
    button.addEventListener("click", () =>
      startPackScenario(pack.id, button.dataset.startScenario)
    );
  }
  for (const button of libraryList.querySelectorAll("[data-test-scenario]")) {
    button.addEventListener("click", () =>
      testDrivePackScenario(pack.id, button.dataset.testScenario)
    );
  }
  for (const button of libraryList.querySelectorAll("[data-open-scenario]")) {
    button.addEventListener("click", () => openPackScenario(pack.id, button.dataset.openScenario));
  }
  for (const button of libraryList.querySelectorAll("[data-approve-scenario]")) {
    button.addEventListener("click", () =>
      setScenarioApproval(pack.id, button.dataset.approveScenario, "approve")
    );
  }
  for (const button of libraryList.querySelectorAll("[data-retire-scenario]")) {
    button.addEventListener("click", () =>
      setScenarioApproval(pack.id, button.dataset.retireScenario, "retire")
    );
  }
}

async function testDrivePackScenario(packId, scenarioId) {
  try {
    const data = await api(
      `/api/packs/${encodeURIComponent(packId)}/scenarios/${encodeURIComponent(scenarioId)}`
    );
    scenarioInput.value = data.yaml;
    updateBrief();
    setStatus("Unapproved test drive", "warn");
    await startSession();
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

async function setScenarioApproval(packId, scenarioId, action) {
  try {
    const data = await api(
      `/api/packs/${encodeURIComponent(packId)}/scenarios/${encodeURIComponent(scenarioId)}/${action}`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ by: "local trainer" }),
      }
    );
    await loadLibrary();
    setStatus(action === "approve" ? "Scenario approved" : "Scenario retired", "ok");
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

async function openPackScenario(packId, scenarioId) {
  try {
    const data = await api(
      `/api/packs/${encodeURIComponent(packId)}/scenarios/${encodeURIComponent(scenarioId)}`
    );
    scenarioInput.value = data.yaml;
    updateBrief();
    setStatus(`Loaded ${data.scenario.title} from ${packId}`, "ok");
    document.querySelector(".source-sheet")?.scrollIntoView({ block: "start" });
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

function downloadBlob(blob, filename) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

async function exportPack() {
  const pack = selectedPack();
  if (!pack) return;
  setStatus("Exporting pack...");
  try {
    const response = await fetch(`/api/packs/${encodeURIComponent(pack.id)}/export`, {
      method: "POST",
      headers: { "X-Callcraft-Token": token },
    });
    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error(data.error || `HTTP ${response.status}`);
    }
    const blob = await response.blob();
    const disposition = response.headers.get("Content-Disposition") || "";
    const match = /filename="?([^";]+)"?/.exec(disposition);
    downloadBlob(blob, match?.[1] || `${pack.id}.callcraftpack.zip`);
    setStatus("Pack exported", "ok");
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

async function inspectPackImport(file) {
  setStatus("Inspecting pack...");
  try {
    const bytes = await file.arrayBuffer();
    const response = await fetch("/api/packs/import/inspect", {
      method: "POST",
      headers: {
        "X-Callcraft-Token": token,
        "Content-Type": "application/zip",
      },
      body: bytes,
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok || data.ok === false) {
      throw new Error(data.error || `HTTP ${response.status}`);
    }
    pendingImportBytes = bytes;
    pendingImportReport = data.report;
    renderImportReview(data.report);
    setStatus("Pack checked; review before import", "ok");
  } catch (error) {
    pendingImportBytes = null;
    pendingImportReport = null;
    importReview.hidden = false;
    importReview.innerHTML = `<div class="import-errors">Import check failed: ${escapeHtml(error.message)}</div>`;
    setStatus("Import check failed", "bad");
  }
}

function renderImportReview(report) {
  const rows = (report.scenarios || [])
    .map((scenario) => {
      const notes = [
        ...(scenario.errors || []),
        ...(scenario.warnings || []).map((warning) => `warning: ${warning}`),
      ].join("; ");
      return `<tr>
        <td><span class="import-state ${escapeHtml(scenario.state)}">${escapeHtml(scenario.state)}</span></td>
        <td>${escapeHtml(scenario.id)}</td>
        <td>${escapeHtml(scenario.title)}</td>
        <td>${escapeHtml(scenario.version)}</td>
        <td>${scenario.critical_items || 0}</td>
        <td>${escapeHtml(notes || "—")}</td>
      </tr>`;
    })
    .join("");
  const errors = (report.errors || []).length
    ? `<div class="import-errors">${report.errors.map(escapeHtml).join("<br>")}</div>`
    : "";
  const localOnly = (report.local_only || []).length
    ? `<p class="library-warning">Replace removes local scenarios: ${escapeHtml(report.local_only.join(", "))}</p>`
    : "";
  const approvals = report.approvals || [];
  const approvalBlock = approvals.length
    ? `<div class="import-approvals">
        <strong>${approvals.length} imported approval(s)</strong> — accepted only when checked.
        <label><input id="accept-approvals" type="checkbox"> Accept imported approvals</label>
      </div>`
    : "";
  const actionLabel = report.action === "install" ? "Install pack" : "Replace pack";
  importReview.innerHTML = `
    <div class="import-head">
      <div>
        <h4>${escapeHtml(report.name)}</h4>
        <p>${escapeHtml(report.pack_id)} · v${escapeHtml(report.version)} · ${escapeHtml(report.visibility)} · ${escapeHtml(report.license)}</p>
      </div>
      <span class="import-state ${escapeHtml(report.action)}">${escapeHtml(report.action)}</span>
    </div>
    <p class="meta">checksum: ${escapeHtml(report.checksum)} · ${report.scenarios.length} scenario(s)</p>
    ${errors}${localOnly}${approvalBlock}
    <table class="import-table">
      <thead><tr><th>State</th><th>ID</th><th>Title</th><th>Version</th><th>Critical</th><th>Notes</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
    <div class="import-actions">
      <button id="import-confirm" class="primary" type="button" ${report.can_import ? "" : "disabled"}>${actionLabel}</button>
      <button id="import-cancel" class="secondary" type="button">Cancel</button>
    </div>
  `;
  importReview.hidden = false;
}

async function confirmPackImport() {
  if (!pendingImportBytes || !pendingImportReport) return;
  const report = pendingImportReport;
  setStatus(report.action === "install" ? "Installing pack..." : "Replacing pack...");
  try {
    const response = await fetch(
      `/api/packs/import?mode=${encodeURIComponent(report.action)}${
        document.getElementById("accept-approvals")?.checked ? "&accept_approvals=true" : ""
      }`,
      {
        method: "POST",
        headers: {
          "X-Callcraft-Token": token,
          "Content-Type": "application/zip",
        },
        body: pendingImportBytes,
      }
    );
    const data = await response.json().catch(() => ({}));
    if (!response.ok || data.ok === false) {
      throw new Error(data.error || `HTTP ${response.status}`);
    }
    pendingImportBytes = null;
    pendingImportReport = null;
    importReview.hidden = true;
    libraryRollback.hidden = false;
    await loadLibrary();
    setStatus(report.action === "install" ? "Pack installed" : "Pack replaced", "ok");
  } catch (error) {
    importReview.innerHTML += `<div class="import-errors">${escapeHtml(error.message)}</div>`;
    setStatus("Import failed", "bad");
  }
}

function cancelPackImport() {
  pendingImportBytes = null;
  pendingImportReport = null;
  importReview.hidden = true;
  importReview.innerHTML = "";
  setStatus("Import cancelled");
}

async function rollbackPack() {
  const pack = selectedPack();
  if (!pack) return;
  if (!window.confirm(`Restore the last pre-import version of ${pack.name}?`)) return;
  setStatus("Restoring backup...");
  try {
    const response = await fetch(`/api/packs/${encodeURIComponent(pack.id)}/rollback`, {
      method: "POST",
      headers: { "X-Callcraft-Token": token },
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok || data.ok === false) {
      throw new Error(data.error || `HTTP ${response.status}`);
    }
    libraryRollback.hidden = true;
    await loadLibrary();
    setStatus("Backup restored", "ok");
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

async function loadLibrary() {
  try {
    const data = await api("/api/packs");
    libraryPacks = data.packs || [];
    const previous = libraryPack.value;
    libraryPack.innerHTML = libraryPacks
      .map(
        (pack) =>
          `<option value="${escapeHtml(pack.id)}">${escapeHtml(pack.name)}</option>`
      )
      .join("");
    if (libraryPacks.some((pack) => pack.id === previous)) {
      libraryPack.value = previous;
    } else if (libraryPacks.some((pack) => pack.id === data.default_pack_id)) {
      libraryPack.value = data.default_pack_id;
    }
    libraryWarning = data.warning || "";
    renderLibrary();
  } catch (error) {
    libraryMeta.textContent = error.message;
    libraryMeta.className = "meta bad";
  }
}

async function sendTurn() {
  const text = turnInput.value.trim();
  if (!text || !sessionId || finished) return;
  turnInput.value = "";

  try {
    const data = await api(`/api/sessions/${sessionId}/turns`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text }),
    });
    if (Array.isArray(data.transcript)) {
      renderTranscript(data.transcript);
    } else {
      bubble("trainee", text);
      bubble("caller", data.caller);
    }
    setSessionStatus("In progress");
  } catch (error) {
    turnInput.value = text;
    setSessionStatus(error.message, "bad");
  }
  turnInput.focus();
}

async function refreshVoiceStatus() {
  try {
    const status = await api("/api/voice/status");
    const callerMode = status.caller === "llm" ? " · LLM caller" : "";
    if (status.stt) {
      recordButton.disabled = finished;
      voiceMeta.textContent = `Voice ready${callerMode}`;
      voiceMeta.className = "meta ok";
      briefVoice.textContent = `Speech-to-text ready${callerMode}. Record turns from the live call.`;
      briefVoice.className = "meta ok";
    } else {
      recordButton.disabled = true;
      voiceMeta.textContent = `Voice not configured — text only${callerMode}`;
      voiceMeta.className = "meta";
      briefVoice.textContent = `Text only${callerMode}. Configure speech in Settings when needed.`;
      briefVoice.className = "meta";
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
    recordButton.classList.add("recording");
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
  recordButton.classList.remove("recording");
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
    document.body.classList.add("session-review");
    $("session-stage-label").textContent = "Review / Debrief";
    renderScorecard(report);
    $("download").disabled = false;
    setStatus("Scored", "ok");
    setSessionStatus(
      report.evaluation.overall.toUpperCase(),
      report.evaluation.overall === "pass" ? "ok" : "bad"
    );
    window.scrollTo({
      top: window.innerWidth > 900 ? 0 : Math.max(0, scorecard.offsetTop - 72),
    });
  } catch (error) {
    setStatus(error.message, "bad");
  }
}

function renderScorecard(data) {
  const evaluation = data.evaluation;
  const items = evaluation.items || [];
  const passed = items.filter((item) => item.result === "pass").length;
  const failed = items.filter((item) => item.result === "fail").length;
  const human = items.filter((item) => item.result === "needs_human").length;
  const overall = String(evaluation.overall || "needs_human").toUpperCase();
  const rows = items
    .map((item) => {
      const label =
        item.result === "pass" ? "PASS" : item.result === "fail" ? "FAIL" : "NEEDS HUMAN";
      const evidence = (item.evidence || [])
        .map(
          (entry) =>
            `<button type="button" class="evidence-link" data-evidence="${escapeHtml(entry)}">${escapeHtml(entry)}</button>`
        )
        .join(" ");
      return `<tr>
        <td class="result ${item.result}">${label}</td>
        <td class="item"><strong>${escapeHtml(item.id)}</strong>${item.description ? `<span class="item-description">${escapeHtml(item.description)}</span>` : ""}${item.critical ? ' <span class="critical">critical</span>' : ""}</td>
        <td>${evidence || "—"}</td>
      </tr>`;
    })
    .join("");

  const failures = evaluation.critical_failures.length
    ? `<p class="failures">Critical failures: ${escapeHtml(evaluation.critical_failures.join(", "))}</p>`
    : `<p class="clean">No critical failures.</p>`;
  const latency = data.latency_ms?.p50_ms;
  const meta = [
    `Session ${escapeHtml(data.session_id.slice(0, 12))}`,
    `cost $${Number(data.cost?.total_usd || 0).toFixed(2)}`,
    `drift ${evaluation.drift_rejections}`,
    `adapter faults ${evaluation.adapter_failures || 0}`,
    `approval ${data.approval || "none"}`,
    `scenario ${String(data.scenario_sha256 || "").slice(0, 12)}`,
    latency ? `p50 ${latency} ms` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  scorecard.innerHTML = `
    <div class="score-summary">
      <div class="score-stat"><span>Overall</span><strong class="result ${evaluation.overall}">${overall}</strong></div>
      <div class="score-stat"><span>Checklist</span><strong>${passed}/${items.length}</strong></div>
      <div class="score-stat"><span>Failed</span><strong>${failed}</strong></div>
      <div class="score-stat"><span>Human review</span><strong>${human}</strong></div>
    </div>
    <h3>Debrief</h3>
    <table>
      <thead><tr><th>Result</th><th>Item</th><th>Evidence</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
    ${failures}
    <p class="meta">${meta}</p>
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

scorecard.addEventListener("click", (event) => {
  const link = event.target.closest("[data-evidence]");
  if (!link) return;
  const evidence = link.dataset.evidence;
  const target = [...transcriptEl.querySelectorAll(".log-row")].find(
    (row) => row.dataset.utteranceId === evidence
  );
  if (!target) return;
  target.scrollIntoView({ block: "center" });
  target.classList.add("highlight");
  setTimeout(() => target.classList.remove("highlight"), 1600);
});

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
scenarioInput.addEventListener("input", updateBrief);
libraryPack.addEventListener("change", renderLibrary);
libraryExport.addEventListener("click", exportPack);
libraryImport.addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  event.target.value = "";
  if (file) await inspectPackImport(file);
});
importReview.addEventListener("click", (event) => {
  if (event.target.id === "import-confirm") confirmPackImport();
  if (event.target.id === "import-cancel") cancelPackImport();
});
libraryRollback.addEventListener("click", rollbackPack);
$("start").addEventListener("click", startSession);
$("send").addEventListener("click", sendTurn);
$("record").addEventListener("click", toggleRecording);
$("finish").addEventListener("click", finishSession);
$("download").addEventListener("click", downloadReport);
$("back-to-brief").addEventListener("click", () => {
  if (!finished && sessionId && !window.confirm("Leave this session without scoring?")) return;
  document.body.classList.remove("session-live", "session-review");
  $("session-stage-label").textContent = "Run / Live call";
  sessionPanel.hidden = true;
  sessionId = null;
  report = null;
  finished = false;
  sessionStartedAt = null;
  recordButton.classList.remove("recording");
  scorecard.hidden = true;
  scorecard.innerHTML = "";
  $("download").disabled = true;
  setStatus("Ready");
  setSessionStatus("");
  updateBrief();
  window.scrollTo({ top: 0 });
});

$("file").addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  if (!file) return;
  scenarioInput.value = await file.text();
  updateBrief();
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

window.refreshVoiceStatus = refreshVoiceStatus;

updateBrief();
if (token) {
  refreshVoiceStatus();
  loadLibrary();
}
window.refreshLibrary = loadLibrary;

if (!token) {
  setStatus("Missing session token — open the URL printed by `callcraft serve`.", "bad");
  $("start").disabled = true;
}
