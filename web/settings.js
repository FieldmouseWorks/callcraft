// Provider settings for the caller LLM, speech-to-text, and text-to-speech.
// Manifests come from the server; keys are posted once to the loopback API and
// stored by the server in the OS keychain.
(function () {
  const token = new URLSearchParams(location.search).get("token") || "";
  const $ = (id) => document.getElementById(id);

  let manifests = [];
  let active = null;
  let keyring = true;
  let sttManifests = [];
  let ttsManifests = [];
  let speechActive = { stt: null, tts: null };
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

  function setMeta(element, text, kind = "") {
    element.textContent = text;
    element.className = `meta ${kind}`;
  }

  function setOutput(element, text) {
    element.textContent = text || "";
  }

  function clean(value) {
    return String(value || "").trim();
  }

  // ---- caller LLM -------------------------------------------------------

  const callerSelect = $("provider-select");
  const callerUrl = $("provider-url");
  const callerModel = $("provider-model");
  const callerKey = $("provider-key");
  const callerNote = $("provider-note");
  const callerStatus = $("provider-status");
  const callerOutput = $("provider-output");

  function selectedManifest() {
    return manifests.find((manifest) => manifest.id === callerSelect.value) || null;
  }

  function applyCallerSelection(manifest) {
    if (!manifest) {
      callerUrl.value = "";
      callerModel.value = "";
      callerKey.value = "";
      callerKey.disabled = true;
      callerNote.textContent = "The deterministic caller needs no provider.";
      setMeta(callerStatus, "Deterministic", "ok");
      return;
    }
    callerUrl.value = manifest.default_base_url || "";
    callerModel.value = "";
    callerKey.value = "";
    callerKey.disabled = manifest.auth === "none";
    callerNote.textContent = manifest.note;
    if (!keyring && manifest.auth !== "none") {
      callerNote.textContent += " OS keychain is unavailable; a key cannot be saved here.";
    }
    if (active && active.provider_id === manifest.id) {
      callerUrl.value = active.base_url || callerUrl.value;
      callerModel.value = active.model || "";
      setMeta(callerStatus, `Active: ${manifest.label}`, "ok");
    } else {
      setMeta(callerStatus, manifest.label);
    }
  }

  function renderCallerSelect() {
    callerSelect.innerHTML = "";
    const deterministic = document.createElement("option");
    deterministic.value = "deterministic";
    deterministic.textContent = "Deterministic caller (no LLM)";
    callerSelect.appendChild(deterministic);
    for (const manifest of manifests) {
      const option = document.createElement("option");
      option.value = manifest.id;
      option.textContent = manifest.label;
      callerSelect.appendChild(option);
    }
    callerSelect.value = active && active.enabled ? active.provider_id : "deterministic";
    applyCallerSelection(selectedManifest());
  }

  function callerBody() {
    return {
      provider_id: callerSelect.value,
      base_url: clean(callerUrl.value),
      model: clean(callerModel.value),
      api_key: clean(callerKey.value) || null,
    };
  }

  async function testCaller() {
    if (callerSelect.value === "deterministic") {
      setOutput(callerOutput, "Deterministic caller is always available; no connection to test.");
      return;
    }
    setMeta(callerStatus, "Testing...");
    setOutput(callerOutput, "");
    $("provider-test").disabled = true;
    try {
      const result = await api("/api/providers/test", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(callerBody()),
      });
      if (result.ok) {
        setMeta(callerStatus, `Test passed in ${result.latency_ms} ms`, "ok");
        setOutput(
          callerOutput,
          [
            `provider: ${result.provider_id}`,
            `model: ${result.model}`,
            `latency: ${result.latency_ms} ms`,
            `decision: ${JSON.stringify(result.fact_ids)} · ${result.style || "neutral"}`,
            `rendered caller: ${result.caller}`,
          ].join("\n")
        );
      } else {
        setMeta(callerStatus, "Test failed", "bad");
        setOutput(callerOutput, `error: ${result.error}${result.latency_ms ? `\nlatency: ${result.latency_ms} ms` : ""}`);
      }
    } catch (error) {
      setMeta(callerStatus, "Test failed", "bad");
      setOutput(callerOutput, error.message);
    }
    $("provider-test").disabled = false;
  }

  async function saveCaller() {
    if (callerSelect.value === "deterministic") {
      await resetCaller();
      return;
    }
    setMeta(callerStatus, "Saving...");
    try {
      const data = await api("/api/providers/configure", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(callerBody()),
      });
      active = data.active;
      callerKey.value = "";
      setMeta(callerStatus, `Saved: ${active.provider_id} · ${active.model}`, "ok");
      setOutput(callerOutput, JSON.stringify(active, null, 2));
      if (window.refreshVoiceStatus) window.refreshVoiceStatus();
    } catch (error) {
      setMeta(callerStatus, "Save failed", "bad");
      setOutput(callerOutput, error.message);
    }
  }

  async function resetCaller() {
    setMeta(callerStatus, "Switching to deterministic...");
    try {
      const data = await api("/api/providers/reset", { method: "POST" });
      active = data.active;
      callerSelect.value = "deterministic";
      applyCallerSelection(null);
      setMeta(callerStatus, "Deterministic caller", "ok");
      setOutput(callerOutput, JSON.stringify(active, null, 2));
      if (window.refreshVoiceStatus) window.refreshVoiceStatus();
    } catch (error) {
      setMeta(callerStatus, "Reset failed", "bad");
      setOutput(callerOutput, error.message);
    }
  }

  // ---- speech providers ------------------------------------------------

  function speechEls(kind) {
    const prefix = kind;
    return {
      select: $(`${prefix}-provider`),
      url: $(`${prefix}-url`),
      model: $(`${prefix}-model`),
      language: kind === "stt" ? $(`${prefix}-language`) : null,
      voice: kind === "tts" ? $(`${prefix}-voice`) : null,
      key: $(`${prefix}-key`),
      note: $(`${prefix}-note`),
      status: $(`${prefix}-status`),
      output: $(`${prefix}-output`),
      test: $(`${prefix}-test`),
      save: $(`${prefix}-save`),
      reset: $(`${prefix}-reset`),
    };
  }

  function speechManifests(kind) {
    return kind === "stt" ? sttManifests : ttsManifests;
  }

  function selectedSpeechManifest(kind) {
    return speechManifests(kind).find((manifest) => manifest.id === speechEls(kind).select.value) || null;
  }

  function applySpeechSelection(kind) {
    const e = speechEls(kind);
    const manifest = selectedSpeechManifest(kind);
    const activeForKind = speechActive[kind];
    if (!manifest) {
      e.url.value = "";
      e.model.value = "";
      if (e.language) e.language.value = "";
      if (e.voice) e.voice.value = "";
      e.key.value = "";
      e.key.disabled = true;
      e.note.textContent = "Not configured.";
      setMeta(e.status, "Not configured");
      return;
    }
    e.url.value = manifest.default_base_url || "";
    e.model.value = manifest.default_model || "";
    if (e.language) e.language.value = "en";
    if (e.voice) e.voice.value = manifest.default_voice || "";
    e.key.value = "";
    e.key.disabled = manifest.auth === "none";
    e.note.textContent = manifest.note;
    if (!keyring && manifest.auth !== "none") {
      e.note.textContent += " OS keychain is unavailable; a key cannot be saved here.";
    }
    if (activeForKind && activeForKind.provider_id === manifest.id) {
      e.url.value = activeForKind.base_url || e.url.value;
      e.model.value = activeForKind.model || "";
      if (e.language && activeForKind.language) e.language.value = activeForKind.language;
      if (e.voice && activeForKind.voice) e.voice.value = activeForKind.voice;
      setMeta(e.status, `Active: ${manifest.label}`, "ok");
    } else {
      setMeta(e.status, manifest.label);
    }
  }

  function renderSpeechSelect(kind) {
    const e = speechEls(kind);
    e.select.innerHTML = "";
    const none = document.createElement("option");
    none.value = "";
    none.textContent = "Not configured";
    e.select.appendChild(none);
    for (const manifest of speechManifests(kind)) {
      const option = document.createElement("option");
      option.value = manifest.id;
      option.textContent = manifest.label;
      e.select.appendChild(option);
    }
    const activeForKind = speechActive[kind];
    e.select.value = activeForKind ? activeForKind.provider_id : "";
    applySpeechSelection(kind);
  }

  function speechBody(kind) {
    const e = speechEls(kind);
    return {
      kind,
      provider_id: e.select.value,
      base_url: clean(e.url.value),
      model: clean(e.model.value),
      language: e.language ? clean(e.language.value) : null,
      voice: e.voice ? clean(e.voice.value) : null,
      api_key: clean(e.key.value) || null,
    };
  }

  async function testSpeech(kind) {
    const e = speechEls(kind);
    if (!e.select.value) {
      setOutput(e.output, "Select a provider first.");
      return;
    }
    setMeta(e.status, "Testing...");
    setOutput(e.output, "");
    e.test.disabled = true;
    try {
      const result = await api("/api/speech/test", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(speechBody(kind)),
      });
      if (!result.ok) {
        setMeta(e.status, "Test failed", "bad");
        setOutput(e.output, `error: ${result.error}${result.latency_ms ? `\nlatency: ${result.latency_ms} ms` : ""}`);
      } else if (kind === "stt") {
        setMeta(e.status, `Test passed in ${result.latency_ms} ms`, "ok");
        setOutput(
          e.output,
          [`provider: ${result.provider_id}`, `model: ${result.model}`, `latency: ${result.latency_ms} ms`, `transcript: ${result.transcript}`].join("\n")
        );
      } else {
        setMeta(e.status, `Test passed in ${result.latency_ms} ms`, "ok");
        setOutput(
          e.output,
          [
            `provider: ${result.provider_id}`,
            `model: ${result.model}`,
            `voice: ${result.voice || "default"}`,
            `latency: ${result.latency_ms} ms`,
            `audio: ${result.bytes} bytes (${result.audio_mime})`,
          ].join("\n")
        );
        if (result.audio_base64) {
          const audio = new Audio(`data:${result.audio_mime || "audio/wav"};base64,${result.audio_base64}`);
          audio.play().catch(() => {});
        }
      }
    } catch (error) {
      setMeta(e.status, "Test failed", "bad");
      setOutput(e.output, error.message);
    }
    e.test.disabled = false;
  }

  async function saveSpeech(kind) {
    const e = speechEls(kind);
    if (!e.select.value) {
      await resetSpeech(kind);
      return;
    }
    setMeta(e.status, "Saving...");
    try {
      const data = await api("/api/speech/configure", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(speechBody(kind)),
      });
      speechActive = data.active;
      e.key.value = "";
      setMeta(e.status, `Saved: ${speechActive[kind].provider_id}`, "ok");
      setOutput(e.output, JSON.stringify(speechActive[kind], null, 2));
      if (window.refreshVoiceStatus) window.refreshVoiceStatus();
    } catch (error) {
      setMeta(e.status, "Save failed", "bad");
      setOutput(e.output, error.message);
    }
  }

  async function resetSpeech(kind) {
    const e = speechEls(kind);
    setMeta(e.status, "Clearing...");
    try {
      const data = await api("/api/speech/reset", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ kind }),
      });
      speechActive = data.active;
      e.select.value = "";
      applySpeechSelection(kind);
      setMeta(e.status, "Cleared", "ok");
      setOutput(e.output, JSON.stringify(speechActive[kind], null, 2));
      if (window.refreshVoiceStatus) window.refreshVoiceStatus();
    } catch (error) {
      setMeta(e.status, "Clear failed", "bad");
      setOutput(e.output, error.message);
    }
  }

  // ---- loading and events ----------------------------------------------

  async function loadProviders() {
    if (loaded) return;
    setMeta(callerStatus, "Loading providers...");
    try {
      const [callerData, speechData] = await Promise.all([
        api("/api/providers"),
        api("/api/speech/providers"),
      ]);
      manifests = callerData.providers || [];
      active = callerData.active || null;
      keyring = Boolean(callerData.keyring);
      sttManifests = speechData.stt || [];
      ttsManifests = speechData.tts || [];
      speechActive = speechData.active || { stt: null, tts: null };
      loaded = true;
      renderCallerSelect();
      renderSpeechSelect("stt");
      renderSpeechSelect("tts");
      if (active && active.enabled) {
        setMeta(callerStatus, `Active: ${active.provider_id} · ${active.model}`, "ok");
      }
    } catch (error) {
      setMeta(callerStatus, error.message, "bad");
    }
  }

  callerSelect.addEventListener("change", () => {
    const manifest = selectedManifest();
    applyCallerSelection(manifest);
    if (active && manifest && active.provider_id === manifest.id) {
      callerUrl.value = active.base_url || callerUrl.value;
      callerModel.value = active.model || "";
    }
  });
  $("provider-test").addEventListener("click", testCaller);
  $("provider-save").addEventListener("click", saveCaller);
  $("provider-reset").addEventListener("click", resetCaller);

  for (const kind of ["stt", "tts"]) {
    const e = speechEls(kind);
    e.select.addEventListener("change", () => applySpeechSelection(kind));
    e.test.addEventListener("click", () => testSpeech(kind));
    e.save.addEventListener("click", () => saveSpeech(kind));
    e.reset.addEventListener("click", () => resetSpeech(kind));
  }

  document.querySelectorAll(".tab").forEach((button) => {
    button.addEventListener("click", () => {
      if (button.dataset.tab === "settings-tab") loadProviders();
    });
  });

  if (!token) {
    setMeta(callerStatus, "Missing session token", "bad");
  }
})();
