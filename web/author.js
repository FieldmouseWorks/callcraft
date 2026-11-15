// Scenario authoring form. The scenario JSON is the single source of truth;
// every field edit mutates it, then validation runs against the same schema
// the runner uses. Rendering to YAML happens server-side.
(function () {
  const token = new URLSearchParams(location.search).get("token") || "";
  const $ = (id) => document.getElementById(id);

  const formEl = $("author-form");
  const outputEl = $("author-output");
  const metaEl = $("author-meta");
  const scenarioInput = $("scenario");
  const statusEl = $("status");

  let data = null;
  let validateTimer = null;

  function setMeta(text, kind = "") {
    metaEl.textContent = text;
    metaEl.className = `meta ${kind}`;
  }

  function setOutput(text) {
    outputEl.textContent = text || "";
  }

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

  function el(tag, props = {}, children = []) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(props)) {
      if (key === "class") {
        node.className = value;
      } else if (key === "text") {
        node.textContent = value;
      } else if (key.startsWith("on") && typeof value === "function") {
        node.addEventListener(key.slice(2), value);
      } else if (key in node) {
        node[key] = value;
      } else {
        node.setAttribute(key, value);
      }
    }
    for (const child of [].concat(children)) {
      if (child !== null && child !== undefined) node.append(child);
    }
    return node;
  }

  function field(labelText, control) {
    return el("label", { class: "field" }, [
      el("span", { class: "field-label", text: labelText }),
      control,
    ]);
  }

  function textInput(value, onInput, placeholder = "") {
    const input = el("input", {
      class: "input",
      type: "text",
      value: value ?? "",
      placeholder,
    });
    input.addEventListener("input", () => onInput(input.value));
    return input;
  }

  function numberInput(value, onInput) {
    const input = el("input", {
      class: "input",
      type: "number",
      step: "1",
      value: value ?? "",
    });
    input.addEventListener("input", () =>
      onInput(input.value === "" ? null : Number(input.value))
    );
    return input;
  }

  function checkboxInput(checked, onChange) {
    const input = el("input", { type: "checkbox", checked: Boolean(checked) });
    input.addEventListener("change", () => onChange(input.checked));
    return input;
  }

  function selectInput(options, value, onChange) {
    const node = el(
      "select",
      { class: "input" },
      options.map((option) => el("option", { value: option, text: option }))
    );
    node.value = value;
    node.addEventListener("change", () => onChange(node.value));
    return node;
  }

  function splitList(text) {
    return text
      .split(",")
      .map((part) => part.trim())
      .filter(Boolean);
  }

  function section(title, children) {
    return el("section", { class: "form-section" }, [el("h3", { text: title }), ...children]);
  }

  function humanReviewRequired(value) {
    return (
      value === true ||
      (typeof value === "string" && value.toLowerCase() === "required")
    );
  }

  function normalize(scenario) {
    scenario.authors = scenario.authors || [];
    scenario.tags = scenario.tags || [];
    scenario.persona = scenario.persona || { display_name: "" };
    scenario.facts = scenario.facts || [];
    scenario.rubric = scenario.rubric || { items: [] };
    scenario.rubric.items = scenario.rubric.items || [];
    scenario.safety = scenario.safety || { disclaimer: "" };

    for (const item of scenario.rubric.items) {
      const anchors = item.anchors || {};
      anchors.any_of = anchors.any_of || [];
      anchors.all_of = anchors.all_of || [];
      const direct = {};
      for (const key of ["transcript_regex", "utterance_pair", "event"]) {
        if (anchors[key] !== undefined) {
          direct[key] = anchors[key];
          delete anchors[key];
        }
      }
      if (Object.keys(direct).length) anchors.any_of.unshift(direct);
      item.anchors = anchors;
    }
    return scenario;
  }

  function renderForm() {
    formEl.replaceChildren();
    if (!data) {
      formEl.append(
        el("p", { class: "meta", text: "Load a scenario file or the example to start authoring." })
      );
      return;
    }

    const metadata = el("div", { class: "grid" });
    metadata.append(
      field("ID", textInput(data.id, (value) => { data.id = value; scheduleValidate(); })),
      field("Title", textInput(data.title, (value) => { data.title = value; scheduleValidate(); })),
      field("Version", textInput(data.version, (value) => { data.version = value; scheduleValidate(); })),
      field("License", textInput(data.license, (value) => { data.license = value; scheduleValidate(); })),
      field("Language", textInput(data.language, (value) => { data.language = value; scheduleValidate(); })),
      field("Incident type", selectInput(["medical", "fire", "police", "other"], data.incident_type || "other", (value) => { data.incident_type = value; scheduleValidate(); })),
      field("Difficulty", selectInput(["basic", "intermediate", "advanced"], data.difficulty || "basic", (value) => { data.difficulty = value; scheduleValidate(); })),
      field("Duration (minutes)", numberInput(data.duration_minutes ?? null, (value) => { if (value === null) delete data.duration_minutes; else data.duration_minutes = value; scheduleValidate(); })),
      field("Authors (comma separated)", textInput((data.authors || []).join(", "), (value) => { data.authors = splitList(value); scheduleValidate(); })),
      field("Tags (comma separated)", textInput((data.tags || []).join(", "), (value) => { data.tags = splitList(value); scheduleValidate(); }))
    );

    const safety = el("div", { class: "grid" });
    safety.append(
      field("Disclaimer", textInput(data.safety?.disclaimer || "", (value) => { data.safety = data.safety || {}; data.safety.disclaimer = value; scheduleValidate(); })),
      field("Content warnings (comma separated)", textInput((data.safety?.content_warnings || []).join(", "), (value) => { data.safety = data.safety || {}; data.safety.content_warnings = splitList(value); scheduleValidate(); }))
    );

    const persona = data.persona;
    const personaGrid = el("div", { class: "grid" });
    personaGrid.append(
      field("Display name", textInput(persona.display_name || "", (value) => { persona.display_name = value; scheduleValidate(); })),
      field("Age", numberInput(persona.age ?? null, (value) => { if (value === null) delete persona.age; else persona.age = value; scheduleValidate(); })),
      field("Emotional state", textInput(persona.emotional_state || "", (value) => { persona.emotional_state = value; scheduleValidate(); })),
      field("Impairments (comma separated)", textInput((persona.impairments || []).join(", "), (value) => { persona.impairments = splitList(value); scheduleValidate(); }))
    );

    formEl.append(section("Metadata", [metadata]));
    formEl.append(section("Safety", [safety]));
    formEl.append(section("Persona", [personaGrid]));
    formEl.append(renderFacts());
    formEl.append(renderRubric());
  }

  function renderFacts() {
    const list = el("div", { class: "card-list" });
    data.facts.forEach((fact, index) => list.append(factCard(fact, index)));
    if (!data.facts.length) {
      list.append(el("p", { class: "meta", text: "No facts yet." }));
    }
    const add = el("button", { class: "secondary", type: "button", text: "Add fact" });
    add.addEventListener("click", () => {
      data.facts.push({
        id: `fact_${data.facts.length + 1}`,
        value: "",
        disclosure: "on_question",
        triggers: [],
      });
      renderForm();
      scheduleValidate();
    });
    return section("Facts", [list, add]);
  }

  function factCard(fact, index) {
    const grid = el("div", { class: "grid" });
    grid.append(
      field("Fact id", textInput(fact.id, (value) => { fact.id = value; scheduleValidate(); })),
      field("Value", textInput(fact.value ?? "", (value) => { fact.value = value; scheduleValidate(); })),
      field("Disclosure", selectInput(["volunteer", "on_question", "if_asked", "never_unless", "after_event"], fact.disclosure || "on_question", (value) => { fact.disclosure = value; scheduleValidate(); })),
      field("Triggers (comma separated)", textInput((fact.triggers || []).join(", "), (value) => { fact.triggers = splitList(value); scheduleValidate(); })),
      field("Requires event", textInput(fact.requires_event ?? "", (value) => { if (value.trim()) fact.requires_event = value.trim(); else delete fact.requires_event; scheduleValidate(); })),
      field("Verifiable", checkboxInput(!!fact.verifiable, (value) => { fact.verifiable = value; scheduleValidate(); }))
    );

    const remove = el("button", { class: "danger", type: "button", text: "Remove fact" });
    remove.addEventListener("click", () => {
      data.facts.splice(index, 1);
      renderForm();
      scheduleValidate();
    });

    return el("div", { class: "card" }, [
      el("div", { class: "card-head" }, [
        el("strong", { text: fact.id || "untitled fact" }),
        remove,
      ]),
      grid,
    ]);
  }

  function renderRubric() {
    const list = el("div", { class: "card-list" });
    data.rubric.items.forEach((item, index) => list.append(rubricCard(item, index)));
    if (!data.rubric.items.length) {
      list.append(el("p", { class: "meta", text: "No rubric items yet." }));
    }
    const add = el("button", { class: "secondary", type: "button", text: "Add rubric item" });
    add.addEventListener("click", () => {
      data.rubric.items.push({
        id: `item_${data.rubric.items.length + 1}`,
        description: "",
        critical: false,
        anchors: { any_of: [], all_of: [] },
      });
      renderForm();
      scheduleValidate();
    });
    return section("Rubric", [list, add]);
  }

  function rubricCard(item, index) {
    const grid = el("div", { class: "grid" });
    grid.append(
      field("Item id", textInput(item.id, (value) => { item.id = value; scheduleValidate(); })),
      field("Description", textInput(item.description, (value) => { item.description = value; scheduleValidate(); })),
      field("Critical", checkboxInput(item.critical, (value) => { item.critical = value; scheduleValidate(); })),
      field("Human review required", checkboxInput(humanReviewRequired(item.human_review), (value) => { item.human_review = value; scheduleValidate(); })),
      field("Evidence required", checkboxInput(!!item.evidence_required, (value) => { item.evidence_required = value; scheduleValidate(); }))
    );

    const anchors = el("div", { class: "anchors" });
    renderAnchors(item, anchors);

    const remove = el("button", { class: "danger", type: "button", text: "Remove item" });
    remove.addEventListener("click", () => {
      data.rubric.items.splice(index, 1);
      renderForm();
      scheduleValidate();
    });

    return el("div", { class: "card" }, [
      el("div", { class: "card-head" }, [
        el("strong", { text: item.id || "untitled item" }),
        remove,
      ]),
      grid,
      anchors,
    ]);
  }

  function renderAnchors(item, container) {
    item.anchors = item.anchors || { any_of: [], all_of: [] };
    item.anchors.any_of = item.anchors.any_of || [];
    item.anchors.all_of = item.anchors.all_of || [];

    let group = container.dataset.group;
    if (!group) {
      group = item.anchors.any_of.length
        ? "any_of"
        : item.anchors.all_of.length
          ? "all_of"
          : "any_of";
    }
    container.dataset.group = group;
    container.replaceChildren();

    const add = el("button", { class: "secondary", type: "button", text: "Add anchor" });
    add.addEventListener("click", () => {
      item.anchors[container.dataset.group].push({ event: "" });
      renderAnchors(item, container);
      scheduleValidate();
    });

    container.append(
      el("div", { class: "anchor-head" }, [
        el("span", { class: "field-label", text: "Anchor group" }),
        selectInput(["any_of", "all_of"], group, (value) => {
          container.dataset.group = value;
          renderAnchors(item, container);
        }),
        add,
      ])
    );

    const rows = item.anchors[group];
    if (!rows.length) {
      container.append(
        el("p", { class: "meta", text: "No anchors in this group; validation will fail." })
      );
    }
    rows.forEach((anchor, index) =>
      container.append(anchorRow(anchor, index, item, container, group))
    );
  }

  function anchorRow(anchor, index, item, container, group) {
    let type = "event";
    if (anchor.transcript_regex !== undefined) type = "transcript_regex";
    else if (anchor.utterance_pair !== undefined) type = "utterance_pair";

    const typeSelect = selectInput(
      ["transcript_regex", "utterance_pair", "event"],
      type,
      (value) => {
        delete anchor.transcript_regex;
        delete anchor.utterance_pair;
        delete anchor.event;
        if (value === "transcript_regex") anchor.transcript_regex = "";
        else if (value === "utterance_pair") anchor.utterance_pair = ["", ""];
        else anchor.event = "";
        renderAnchors(item, container);
        scheduleValidate();
      }
    );

    let valueNode;
    if (type === "utterance_pair") {
      const pair = Array.isArray(anchor.utterance_pair) ? anchor.utterance_pair : ["", ""];
      anchor.utterance_pair = pair;
      valueNode = el("div", { class: "pair" }, [
        textInput(pair[0] || "", (value) => { pair[0] = value; scheduleValidate(); }, "caller_fact_id"),
        textInput(pair[1] || "", (value) => { pair[1] = value; scheduleValidate(); }, "trainee_repeat"),
      ]);
    } else {
      const key = type;
      valueNode = textInput(
        anchor[key] || "",
        (value) => {
          anchor[key] = value;
          scheduleValidate();
        },
        type === "event" ? "protocol_card_delivered" : "(?i)address"
      );
    }

    const remove = el("button", { class: "danger", type: "button", text: "Remove" });
    remove.addEventListener("click", () => {
      item.anchors[group].splice(index, 1);
      renderAnchors(item, container);
      scheduleValidate();
    });

    return el("div", { class: "anchor-row" }, [typeSelect, valueNode, remove]);
  }

  function scheduleValidate() {
    clearTimeout(validateTimer);
    validateTimer = setTimeout(runValidate, 350);
  }

  async function runValidate() {
    if (!data) return;
    setMeta("Validating...");
    try {
      const result = await api("/api/scenario/validate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(data),
      });
      const warnings = result.warnings || [];
      setMeta(
        warnings.length ? "Valid, with warnings" : "Valid",
        warnings.length ? "warn" : "ok"
      );
      setOutput(
        warnings.length
          ? `Valid.\n\nWarnings:\n${warnings.map((warning) => `- ${warning}`).join("\n")}`
          : "Valid."
      );
    } catch (error) {
      setMeta("Invalid", "bad");
      setOutput(`INVALID\n\n${error.message}`);
    }
  }

  async function renderYaml() {
    if (!data) return null;
    setMeta("Rendering...");
    try {
      const result = await api("/api/scenario/render", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(data),
      });
      setMeta("Rendered", "ok");
      setOutput(result.yaml);
      return result.yaml;
    } catch (error) {
      setMeta("Render failed", "bad");
      setOutput(error.message);
      return null;
    }
  }

  async function downloadYaml() {
    const yaml = await renderYaml();
    if (!yaml) return;
    const blob = new Blob([yaml], { type: "text/yaml" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${data.id || "scenario"}.yaml`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }

  async function saveToPack() {
    if (!data) return;
    const yaml = await renderYaml();
    if (!yaml) return;
    setMeta("Saving to pack...");
    try {
      const result = await api("/api/packs/scenarios", {
        method: "POST",
        headers: { "Content-Type": "text/yaml" },
        body: yaml,
      });
      setMeta(`Saved to ${result.pack_id}`, "ok");
      setOutput(
        `Saved ${result.scenario.title} (${result.scenario.id}) to pack ${result.pack_id}.
` +
          `${result.scenario.facts} facts · ${result.scenario.rubric_items} rubric items · ` +
          `${result.scenario.critical_items} critical.`
      );
      if (window.refreshLibrary) window.refreshLibrary();
    } catch (error) {
      setMeta("Save failed", "bad");
      setOutput(error.message);
    }
  }

  async function useInRunner() {

    const yaml = await renderYaml();
    if (!yaml) return;
    scenarioInput.value = yaml;
    if (window.updateBrief) window.updateBrief();
    switchTab("scenario-tab");
    statusEl.textContent = "Loaded authored scenario";
    statusEl.className = "status ok";
  }

  async function loadFromYaml(text) {
    if (!text.trim()) {
      setMeta("Nothing to parse", "bad");
      return;
    }
    setMeta("Parsing...");
    try {
      const result = await api("/api/scenario/parse", {
        method: "POST",
        headers: { "Content-Type": "text/plain; charset=utf-8" },
        body: text,
      });
      data = normalize(result.scenario);
      renderForm();
      setMeta(`Loaded ${data.id || "scenario"}`, "ok");
      setOutput(
        result.warnings?.length
          ? result.warnings.map((warning) => `warning: ${warning}`).join("\n")
          : "Parsed."
      );
      scheduleValidate();
    } catch (error) {
      setMeta("Parse failed", "bad");
      setOutput(error.message);
    }
  }

  function switchTab(id) {
    document.querySelectorAll(".tab").forEach((button) => {
      const active = button.dataset.tab === id;
      button.classList.toggle("active", active);
      button.setAttribute("aria-selected", String(active));
    });
    document.querySelectorAll(".tab-panel").forEach((panel) => {
      panel.hidden = panel.id !== id;
    });
  }

  $("author-file").addEventListener("change", async (event) => {
    const file = event.target.files?.[0];
    if (file) await loadFromYaml(await file.text());
  });

  $("author-example").addEventListener("click", async () => {
    try {
      const result = await api("/api/example");
      await loadFromYaml(result.yaml);
    } catch (error) {
      setMeta(error.message, "bad");
    }
  });

  $("author-validate").addEventListener("click", runValidate);
  $("author-render").addEventListener("click", downloadYaml);
  $("author-save-pack").addEventListener("click", saveToPack);
  $("author-use").addEventListener("click", useInRunner);
  document.querySelectorAll(".tab").forEach((button) => {
    button.addEventListener("click", () => switchTab(button.dataset.tab));
  });
})();
