(function () {
const libraryList = $("scenario-library");
const libraryPack = $("library-pack");
const libraryMeta = $("library-meta");
const packPicker = $("pack-picker");
const libraryExport = $("library-export");
const libraryImport = $("library-import");
const libraryRollback = $("library-rollback");
const importReview = $("import-review");
const libraryNewPack = $("library-new-pack");
const libraryPackSettings = $("library-pack-settings");
const packDialog = $("pack-dialog");
const packForm = $("pack-form");
const packDialogTitle = $("pack-dialog-title");
const packIdInput = $("pack-id");
const packNameInput = $("pack-name");
const packVisibilityInput = $("pack-visibility");
const packLicenseInput = $("pack-license");
const packOwnerInput = $("pack-owner");
const packContactInput = $("pack-contact");
const packError = $("pack-error");
const packSubmit = $("pack-submit");
let libraryPacks = [];
let libraryWarning = "";
let pendingImportBytes = null;
let pendingImportReport = null;
let packDialogMode = "create";

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

function selectedPack() {
  return libraryPacks.find((pack) => pack.id === libraryPack.value) || libraryPacks[0] || null;
}

function renderLibrary() {
  const pack = selectedPack();
  libraryExport.disabled = !pack;
  libraryPackSettings.disabled = !pack;
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

function openPackDialog(mode) {
  const pack = selectedPack();
  if (mode === "edit" && !pack) return;
  packDialogMode = mode;
  packError.hidden = true;
  packError.textContent = "";
  if (mode === "edit") {
    packDialogTitle.textContent = "Pack settings";
    packSubmit.textContent = "Save settings";
    packIdInput.value = pack.id;
    packIdInput.disabled = true;
    packNameInput.value = pack.name || "";
    packVisibilityInput.value = pack.visibility || "private";
    packLicenseInput.value = pack.license || "proprietary";
    packOwnerInput.value = pack.owner || "";
    packContactInput.value = pack.owner_contact || "";
  } else {
    packDialogTitle.textContent = "New pack";
    packSubmit.textContent = "Create pack";
    packIdInput.value = "";
    packIdInput.disabled = false;
    packNameInput.value = "";
    packVisibilityInput.value = "private";
    packLicenseInput.value = "proprietary";
    packOwnerInput.value = pack?.owner || "Local agency";
    packContactInput.value = "";
  }
  packDialog.hidden = false;
  (mode === "edit" ? packNameInput : packIdInput).focus();
}

function closePackDialog() {
  packDialog.hidden = true;
}

async function submitPackForm(event) {
  event.preventDefault();
  const pack = selectedPack();
  const id = packIdInput.value.trim();
  const payload = {
    name: packNameInput.value.trim(),
    visibility: packVisibilityInput.value,
    license: packLicenseInput.value.trim(),
    owner_agency: packOwnerInput.value.trim(),
    owner_contact: packContactInput.value.trim() || null,
  };
  if (!payload.name || !payload.license || !payload.owner_agency) {
    packError.textContent = "Name, license, and owner agency are required.";
    packError.hidden = false;
    return;
  }
  packSubmit.disabled = true;
  try {
    let data;
    if (packDialogMode === "edit") {
      data = await api(`/api/packs/${encodeURIComponent(pack.id)}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
    } else {
      if (!id) {
        throw new Error("Pack ID is required.");
      }
      data = await api("/api/packs", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id, ...payload }),
      });
    }
    closePackDialog();
    await loadLibrary();
    libraryPack.value = data.pack.id;
    renderLibrary();
    setStatus(packDialogMode === "edit" ? "Pack settings saved" : "Pack created", "ok");
  } catch (error) {
    packError.textContent = error.message;
    packError.hidden = false;
  }
  packSubmit.disabled = false;
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

libraryPack.addEventListener("change", renderLibrary);
libraryExport.addEventListener("click", exportPack);
libraryNewPack.addEventListener("click", () => openPackDialog("create"));
libraryPackSettings.addEventListener("click", () => openPackDialog("edit"));
$("pack-close").addEventListener("click", closePackDialog);
$("pack-cancel").addEventListener("click", closePackDialog);
packForm.addEventListener("submit", submitPackForm);
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

if (token) loadLibrary();
window.refreshLibrary = loadLibrary;
})();
