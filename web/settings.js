// Provider settings. Manifests come from the server; keys are posted once to
// the loopback API and stored by the server in the OS keychain.
(function () {
  const token = new URLSearchParams(location.search).get("token") || "";
  const $ = (id) => document.getElementById(id);

  const select = $("provider-select");
  const urlInput = $("provider-url");
  const modelInput = $("provider-model");
  const keyInput = $("provider-key");
  const note = $("provider-note");
  const status = $("provider-status");
  const output = $("provider-output");

  let manifests = [];
  let active = null;
  let keyring = true;
  let loaded = false;

  async function api(path, options = {}) {
    const headers = { ...(options.headers || {}) };
    if (token) headers["X-Callcraft-Token"] = token;
    const response = await fetch(path, { ...options, headers });
    let payload;
    try {
      payload = await response.json();
    } catch {
      payload = { ok: false, error: `HTTP ${response.status}` };
    }
    if (!response.ok || payload.ok === false) {
      throw new Error(payload.error || `HTTP ${response.status}`);
    }
    return payload;
  }

  function setStatus(text, kind = "") {
    status.textContent = text;
    status.className = `meta ${kind}`;
  }

  function setOutput(text) {
    output.textContent = text || "";
  }

  function selectedManifest() {
    return manifests.find((manifest) => manifest.id === select.value) || null;
  }

  function applySelection(manifest) {
    if (!manifest) {
      urlInput.value = "";
      modelInput.value = "";
      keyInput.value = "";
      keyInput.disabled = true;
      note.textContent = "The deterministic caller needs no provider.";
      setStatus("Deterministic", "ok");
      return;
    }
    urlInput.value = manifest.default_base_url || "";
    modelInput.value = "";
    keyInput.value = "";
    keyInput.disabled = manifest.auth === "none";
    note.textContent = manifest.note;
    if (!keyring && manifest.auth !== "none") {
      note.textContent += " OS keychain is unavailable; a key cannot be saved here.";
    }
    if (active && active.provider_id === manifest.id) {
      urlInput.value = active.base_url || urlInput.value;
      modelInput.value = active.model || "";
      setStatus(`Active: ${manifest.label}`, "ok");
    } else {
      setStatus(manifest.label);
    }
  }

  function renderSelect() {
    select.innerHTML = "";
    const deterministic = document.createElement("option");
    deterministic.value = "deterministic";
    deterministic.textContent = "Deterministic caller (no LLM)";
    select.appendChild(deterministic);

    for (const manifest of manifests) {
      const option = document.createElement("option");
      option.value = manifest.id;
      option.textContent = manifest.label;
      select.appendChild(option);
    }

    const activeId = active && active.enabled ? active.provider_id : "deterministic";
    select.value = activeId || "deterministic";
    applySelection(selectedManifest());
  }

  async function loadProviders() {
    if (loaded) return;
    setStatus("Loading providers...");
    try {
      const data = await api("/api/providers");
      manifests = data.providers || [];
      active = data.active || null;
      keyring = Boolean(data.keyring);
      loaded = true;
      renderSelect();
      if (active && active.enabled) {
        setStatus(`Active: ${active.provider_id} · ${active.model}`, "ok");
      }
    } catch (error) {
      setStatus(error.message, "bad");
    }
  }

  function bodyFromForm() {
    return {
      provider_id: select.value,
      base_url: urlInput.value.trim(),
      model: modelInput.value.trim(),
      api_key: keyInput.value.trim() || null,
    };
  }

  async function testConnection() {
    if (select.value === "deterministic") {
      setOutput("Deterministic caller is always available; no connection to test.");
      return;
    }
    setStatus("Testing...");
    setOutput("");
    $("provider-test").disabled = true;
    try {
      const result = await api("/api/providers/test", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(bodyFromForm()),
      });
      if (result.ok) {
        setStatus(`Test passed in ${result.latency_ms} ms`, "ok");
        setOutput(
          [
            `provider: ${result.provider_id}`,
            `model: ${result.model}`,
            `latency: ${result.latency_ms} ms`,
            `decision: ${JSON.stringify(result.fact_ids)} · ${result.style || "neutral"}`,
            `rendered caller: ${result.caller}`,
          ].join("\n")
        );
      } else {
        setStatus("Test failed", "bad");
        setOutput(`error: ${result.error}${result.latency_ms ? `\nlatency: ${result.latency_ms} ms` : ""}`);
      }
    } catch (error) {
      setStatus("Test failed", "bad");
      setOutput(error.message);
    }
    $("provider-test").disabled = false;
  }

  async function saveProvider() {
    if (select.value === "deterministic") {
      await resetProvider();
      return;
    }
    setStatus("Saving...");
    try {
      const data = await api("/api/providers/configure", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(bodyFromForm()),
      });
      active = data.active;
      keyInput.value = "";
      setStatus(`Saved: ${active.provider_id} · ${active.model}`, "ok");
      setOutput(JSON.stringify(active, null, 2));
      if (window.refreshVoiceStatus) window.refreshVoiceStatus();
    } catch (error) {
      setStatus("Save failed", "bad");
      setOutput(error.message);
    }
  }

  async function resetProvider() {
    setStatus("Switching to deterministic...");
    try {
      const data = await api("/api/providers/reset", { method: "POST" });
      active = data.active;
      select.value = "deterministic";
      applySelection(null);
      setStatus("Deterministic caller", "ok");
      setOutput(JSON.stringify(active, null, 2));
      if (window.refreshVoiceStatus) window.refreshVoiceStatus();
    } catch (error) {
      setStatus("Reset failed", "bad");
      setOutput(error.message);
    }
  }

  select.addEventListener("change", () => {
    const manifest = selectedManifest();
    applySelection(manifest);
    if (active && manifest && active.provider_id === manifest.id) {
      urlInput.value = active.base_url || urlInput.value;
      modelInput.value = active.model || "";
    }
  });
  $("provider-test").addEventListener("click", testConnection);
  $("provider-save").addEventListener("click", saveProvider);
  $("provider-reset").addEventListener("click", resetProvider);

  document.querySelectorAll(".tab").forEach((button) => {
    button.addEventListener("click", () => {
      if (button.dataset.tab === "settings-tab") loadProviders();
    });
  });

  if (!token) {
    setStatus("Missing session token", "bad");
  }
})();
