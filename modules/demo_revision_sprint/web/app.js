const MODULE_ID = "demo_revision_sprint";
const STORAGE_KEY = "chattyedu.demo_revision_sprint.v1";
let lastIncomingFingerprint = "";
const fieldIds = [
  "student_name",
  "subject",
  "exam_date",
  "priority_topics",
  "stuck_points",
  "revision_blocks",
  "confidence_notes",
  "next_question_set"
];
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

function buildSummary(topicCount, blockCount) {
  const learner = fields.student_name.value.trim() || "an unnamed learner";
  const subject = fields.subject.value.trim() || "an unnamed subject";
  const nextSet = fields.next_question_set.value.trim() || "the next question set still needs defining";
  return [
    `Revision Sprint is active for ${learner} in ${subject}.`,
    `Priority topics: ${topicCount}.`,
    `Revision blocks planned: ${blockCount}.`,
    `Next question set: ${nextSet}.`
  ].join(" ");
}

function buildSnapshot(topicLines, blockLines) {
  return [
    "# Revision Sprint Snapshot",
    "",
    `- Learner: ${fields.student_name.value.trim() || "not set"}`,
    `- Subject: ${fields.subject.value.trim() || "not set"}`,
    `- Exam date: ${fields.exam_date.value.trim() || "not set"}`,
    "",
    "## Priority topics",
    topicLines.length > 0 ? topicLines.join("\n") : "(none)",
    "",
    "## Stuck points",
    fields.stuck_points.value.trim() || "(empty)",
    "",
    "## Revision blocks",
    blockLines.length > 0 ? blockLines.join("\n") : "(none)",
    "",
    "## Confidence notes",
    fields.confidence_notes.value.trim() || "(empty)",
    "",
    "## Next question set",
    fields.next_question_set.value.trim() || "(empty)"
  ].join("\n");
}

function buildSharedState(state, topicLines, blockLines, summary) {
  return {
    module_id: MODULE_ID,
    summary,
    payload: {
      fields: state,
      metrics: {
        topicCount: topicLines.length,
        blockCount: blockLines.length
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
  const topicLines = meaningfulLines(fields.priority_topics.value);
  const blockLines = meaningfulLines(fields.revision_blocks.value);
  const summary = buildSummary(topicLines.length, blockLines.length);
  const snapshot = buildSnapshot(topicLines, blockLines);
  const sharedState = buildSharedState(state, topicLines, blockLines, summary);

  document.getElementById("topic-count").textContent = String(topicLines.length);
  document.getElementById("block-count").textContent = String(blockLines.length);
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
    tags: ["student", "revision", "planning", "webview", "demo"],
    payload: {
      learner: fields.student_name.value.trim(),
      subject: fields.subject.value.trim(),
      topicCount: topicLines.length,
      blockCount: blockLines.length
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
