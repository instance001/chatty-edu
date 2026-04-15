const MODULE_ID = "template_module";
const STORAGE_KEY = "chattyedu.template_module.webview.v1";
let lastIncomingFingerprint = "";
const fieldIds = ["module_title", "current_focus", "recent_changes", "next_action", "artifacts"];
const fields = Object.fromEntries(fieldIds.map((id) => [id, document.getElementById(id)]));

function collectState() {
  const state = {};
  for (const [id, element] of Object.entries(fields)) {
    state[id] = element.value ?? "";
  }
  return state;
}

function loadState() {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || "{}");
  } catch {
    return {};
  }
}

function meaningfulLines(value) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function saveState() {
  const state = collectState();
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  refreshDerivedUi();
}

function resetState() {
  localStorage.removeItem(STORAGE_KEY);
  for (const element of Object.values(fields)) {
    element.value = "";
  }
  clearChattyEduBridgeStatus();
  clearChattyEduBridgeSharedState();
  document.getElementById("sync-status").textContent = "reset";
  refreshDerivedUi();
}

function buildSummary(artifactCount, completion) {
  const title = fields.module_title.value.trim() || "Template Module";
  const focus = fields.current_focus.value.trim() || "no current focus set";
  const next = fields.next_action.value.trim() || "define the next action";
  return [
    `${title} is ${completion}% filled out.`,
    `Current focus: ${focus}.`,
    `Artifacts listed: ${artifactCount}.`,
    `Next handoff focus: ${next}.`
  ].join(" ");
}

function buildSnapshot(artifactLines) {
  return [
    "# Template Module Snapshot",
    "",
    `- Module title: ${fields.module_title.value.trim() || "not set"}`,
    `- Current focus: ${fields.current_focus.value.trim() || "not set"}`,
    "",
    "## Recent changes",
    fields.recent_changes.value.trim() || "(empty)",
    "",
    "## Next action",
    fields.next_action.value.trim() || "(empty)",
    "",
    "## Artifacts",
    artifactLines.length > 0 ? artifactLines.join("\n") : "(none)"
  ].join("\n");
}

function buildSharedState(state, artifactLines, summary, completion) {
  return {
    module_id: MODULE_ID,
    summary,
    payload: {
      fields: state,
      metrics: {
        completion,
        artifactCount: artifactLines.length
      }
    },
    updated_at_unix_ms: Date.now()
  };
}

function applyIncomingSharedState(incoming) {
  if (!incoming || typeof incoming !== "object" || !incoming.payload || typeof incoming.payload !== "object") {
    return false;
  }
  const fingerprint = JSON.stringify(incoming);
  if (!fingerprint || fingerprint === lastIncomingFingerprint) {
    return false;
  }
  const nextFields = incoming.payload.fields;
  if (!nextFields || typeof nextFields !== "object") {
    return false;
  }

  let changed = false;
  for (const [id, element] of Object.entries(fields)) {
    const nextValue = typeof nextFields[id] === "string" ? nextFields[id] : "";
    if ((element.value ?? "") !== nextValue) {
      element.value = nextValue;
      changed = true;
    }
  }

  lastIncomingFingerprint = fingerprint;
  if (changed) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(collectState()));
    document.getElementById("sync-status").textContent = `applied from ${incoming.from_device_name || "peer"}`;
    refreshDerivedUi();
  }
  return changed;
}

async function pollIncomingSharedState() {
  const incoming = await readChattyEduIncomingSharedState();
  if (!incoming || typeof incoming !== "object") {
    return;
  }
  applyIncomingSharedState(incoming);
}

function refreshDerivedUi() {
  const state = collectState();
  const artifactLines = meaningfulLines(fields.artifacts.value);
  const filled = fieldIds.filter((id) => fields[id].value.trim().length > 0).length;
  const completion = Math.round((filled / fieldIds.length) * 100);
  const summary = buildSummary(artifactLines.length, completion);
  const snapshot = buildSnapshot(artifactLines);
  const sharedState = buildSharedState(state, artifactLines, summary, completion);

  document.getElementById("completion-score").textContent = `${completion}%`;
  document.getElementById("artifact-count").textContent = String(artifactLines.length);
  document.getElementById("bridge-status").textContent = window.chattyEduBridge?.available ? "hosted" : "standalone";
  if (!window.chattyEduBridge?.available) {
    document.getElementById("sync-status").textContent = "local only";
  } else if (!document.getElementById("sync-status").textContent.trim()) {
    document.getElementById("sync-status").textContent = "hosted";
  }
  document.getElementById("summary_preview").value = summary;
  document.getElementById("snapshot_preview").value = snapshot;

  updateChattyEduBridgeStatus(() => ({
    module_id: MODULE_ID,
    summary,
    snapshot,
    tags: ["template", "webview", "starter"],
    payload: {
      moduleTitle: fields.module_title.value.trim(),
      completion,
      artifactCount: artifactLines.length
    }
  }));
  updateChattyEduBridgeSharedState(sharedState);
}

function restoreState() {
  const state = loadState();
  for (const [id, element] of Object.entries(fields)) {
    if (typeof state[id] === "string") {
      element.value = state[id];
    }
    element.addEventListener("input", saveState);
    element.addEventListener("change", saveState);
  }
  refreshDerivedUi();
}

document.getElementById("save-state").addEventListener("click", saveState);
document.getElementById("reset-state").addEventListener("click", resetState);
document.getElementById("refresh-preview").addEventListener("click", refreshDerivedUi);

restoreState();
pollIncomingSharedState();
window.setInterval(pollIncomingSharedState, 2500);
