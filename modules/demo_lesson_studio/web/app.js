const MODULE_ID = "demo_lesson_studio";
const STORAGE_KEY = "chattyedu.demo_lesson_studio.v1";
const INCOMING_ASSET_LANE = "lesson_assets";
let lastIncomingFingerprint = "";
let lastRoomFingerprint = "";
let latestRoomState = null;
let activeSessionKey = "";
let lastAppliedSharedRevision = 0;
let lastAppliedSharedFrom = "";
let lastSyncFlashUntil = 0;
let lastRoomEventsFingerprint = "";
let sharedRoomEvents = [];
let optimisticRoomEvents = [];
let roomToasts = [];
let incomingAssets = [];
let selectedIncomingAssetId = "";
let incomingAssetPreviewText = "(preview appears here)";
const MAX_ROOM_EVENTS = 8;
const ROOM_TOAST_MS = 4200;
const fieldIds = [
  "class_name",
  "lesson_goal",
  "success_criteria",
  "warm_up",
  "main_blocks",
  "checks_for_understanding",
  "homework",
  "resources",
  "teacher_note"
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

function saveLocalState(state) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

function saveState() {
  const state = collectState();
  saveLocalState(state);
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

function buildSummary(blockCount, resourceCount) {
  const className = fields.class_name.value.trim() || "an unnamed class";
  const goal = fields.lesson_goal.value.trim() || "a lesson goal that still needs defining";
  const handoff = fields.homework.value.trim() || "a follow-up task that still needs deciding";
  return [
    `Lesson Studio is planning for ${className}.`,
    `Goal: ${goal}.`,
    `Main lesson blocks: ${blockCount}.`,
    `Resources listed: ${resourceCount}.`,
    `Next bridge: ${handoff}.`,
    latestRoomState?.session_active
      ? `Lesson room session: ${latestRoomState.session_label || latestRoomState.session_id || "active"} (${Math.max(1, Number(latestRoomState.session_revision || 0))}).`
      : "Lesson room session: inactive."
  ].join(" ");
}

function buildSnapshot(blockLines, resourceLines) {
  return [
    "# Lesson Studio Snapshot",
    "",
    `- Class: ${fields.class_name.value.trim() || "not set"}`,
    `- Lesson goal: ${fields.lesson_goal.value.trim() || "not set"}`,
    "",
    "## Success criteria",
    fields.success_criteria.value.trim() || "(empty)",
    "",
    "## Warm-up",
    fields.warm_up.value.trim() || "(empty)",
    "",
    "## Main lesson blocks",
    blockLines.length > 0 ? blockLines.join("\n") : "(none)",
    "",
    "## Checks for understanding",
    fields.checks_for_understanding.value.trim() || "(empty)",
    "",
    "## Homework / bridge",
    fields.homework.value.trim() || "(empty)",
    "",
    "## Resources",
    resourceLines.length > 0 ? resourceLines.join("\n") : "(none)",
    "",
    "## Teacher note",
    fields.teacher_note.value.trim() || "(empty)"
  ].join("\n");
}

function buildSharedState(state, blockLines, resourceLines, summary) {
  return {
    module_id: MODULE_ID,
    summary,
    payload: {
      fields: state,
      metrics: {
        blockCount: blockLines.length,
        resourceCount: resourceLines.length
      }
    },
    updated_at_unix_ms: Date.now(),
    host_authoritative: !!latestRoomState?.host_authoritative
  };
}

function describeParticipants(roomState) {
  if (!roomState || !Array.isArray(roomState.participants) || roomState.participants.length === 0) {
    return "Just this device for now.";
  }
  return roomState.participants
    .map((participant) => {
      const parts = [participant.device_name || participant.device_id || "unknown device"];
      if (participant.is_local) parts.push("you");
      if (participant.connected === false) parts.push("offline");
      return parts.join(" - ");
    })
    .join(" | ");
}

function makeSessionKey(roomState) {
  if (!roomState || !roomState.session_active) {
    return "";
  }
  return [
    roomState.session_id || roomState.session_label || "session",
    roomState.host_device_id || roomState.host_device_name || "teacher"
  ].join("|");
}

function setSyncBadge(elementId, label, tone) {
  const element = document.getElementById(elementId);
  if (!element) {
    return;
  }
  element.textContent = label;
  element.className = `sync-badge ${tone}`;
}

function formatRelativeAge(ageMs) {
  if (ageMs <= 0) {
    return "just now";
  }
  const seconds = Math.floor(ageMs / 1000);
  if (seconds < 5) {
    return "just now";
  }
  if (seconds < 60) {
    return `${seconds}s ago`;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 48) {
    return `${hours}h ago`;
  }
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function makeRoomEventId() {
  return `room-${Date.now()}-${Math.random().toString(16).slice(2, 10)}`;
}

function eventTimestamp(event) {
  return Number(
    event?.received_at_unix_ms ||
      event?.sent_at_unix_ms ||
      event?.created_at_unix_ms ||
      0
  );
}

function trimRoomEventText(value) {
  const text = String(value || "").trim();
  if (!text) {
    return "";
  }
  return text.length > 180 ? `${text.slice(0, 177)}...` : text;
}

function appendUniqueLine(currentValue, nextLine) {
  const trimmedLine = String(nextLine || "").trim();
  if (!trimmedLine) {
    return String(currentValue || "").trim();
  }
  const existing = meaningfulLines(currentValue);
  if (existing.includes(trimmedLine)) {
    return existing.join("\n");
  }
  existing.push(trimmedLine);
  return existing.join("\n");
}

function appendNamedSection(currentValue, heading, body) {
  const trimmedBody = String(body || "").trim();
  if (!trimmedBody) {
    return String(currentValue || "").trim();
  }
  const section = `[${heading}]\n${trimmedBody}`;
  const current = String(currentValue || "").trim();
  if (current.includes(section)) {
    return current;
  }
  return current ? `${current}\n\n${section}` : section;
}

function selectedIncomingAsset() {
  return incomingAssets.find((asset) => asset.asset_id === selectedIncomingAssetId) || null;
}

function incomingAssetDisplayName(asset) {
  return (
    asset?.label?.trim() ||
    asset?.file_name?.trim() ||
    asset?.payload_file_name?.trim() ||
    asset?.kind?.trim() ||
    "Incoming asset"
  );
}

function incomingAssetSource(asset) {
  return asset?.from_device_name?.trim() || asset?.from_device_id?.trim() || "unknown device";
}

function incomingAssetLooksText(asset) {
  const contentType = String(asset?.content_type || "").toLowerCase();
  const fileName = String(asset?.file_name || asset?.payload_file_name || "").toLowerCase();
  return (
    contentType.startsWith("text/") ||
    contentType.includes("json") ||
    contentType.includes("markdown") ||
    contentType.includes("csv") ||
    contentType.includes("xml") ||
    contentType.includes("yaml") ||
    /\.((txt)|(md)|(markdown)|(json)|(csv)|(xml)|(yaml)|(yml))$/.test(fileName)
  );
}

function incomingAssetMeta(asset) {
  const meta = [
    incomingAssetSource(asset),
    asset?.content_type?.trim() || asset?.kind?.trim() || "unknown type",
    `${Number(asset?.byte_len || 0)} bytes`
  ];
  if (Number(asset?.chunk_count || 0) > 1) {
    meta.push(`${asset.chunk_count} chunks`);
  }
  return meta.join(" | ");
}

async function loadIncomingAssetText(asset, { truncate = false } = {}) {
  if (!asset || !incomingAssetLooksText(asset)) {
    return null;
  }

  const payloadUrl = chattyEduIncomingAssetUrl(INCOMING_ASSET_LANE, asset.payload_file_name);
  if (!payloadUrl) {
    return null;
  }

  const response = await fetch(payloadUrl);
  if (!response.ok) {
    throw new Error(`Payload preview failed (${response.status})`);
  }

  let text = await response.text();
  const contentType = String(asset.content_type || "").toLowerCase();
  if (contentType.includes("json")) {
    try {
      text = JSON.stringify(JSON.parse(text), null, 2);
    } catch {
      // Keep raw text when it is not valid JSON.
    }
  }
  if (truncate && text.length > 5000) {
    text = `${text.slice(0, 5000)}\n\n...[preview truncated]`;
  }
  return text || "(empty asset payload)";
}

async function refreshIncomingAssetPreview() {
  const asset = selectedIncomingAsset();
  if (!asset) {
    incomingAssetPreviewText = "(preview appears here)";
    return;
  }

  if (!incomingAssetLooksText(asset)) {
    incomingAssetPreviewText =
      "Binary asset detected. Use Open payload to inspect it externally, or Apply to lesson to record it in the lesson resources.";
    return;
  }

  try {
    incomingAssetPreviewText = await loadIncomingAssetText(asset, { truncate: true });
  } catch (err) {
    incomingAssetPreviewText = `Could not preview this asset: ${err}`;
  }
}

function renderIncomingAssets() {
  const status = document.getElementById("incoming-asset-status");
  const list = document.getElementById("incoming-asset-list");
  const title = document.getElementById("incoming-asset-title");
  const meta = document.getElementById("incoming-asset-meta");
  const preview = document.getElementById("incoming-asset-preview");
  const openButton = document.getElementById("incoming-asset-open");
  const applyButton = document.getElementById("incoming-asset-apply");
  const consumeButton = document.getElementById("incoming-asset-consume");
  if (!status || !list || !title || !meta || !preview || !openButton || !applyButton || !consumeButton) {
    return;
  }

  if (!window.chattyEduBridge?.available) {
    status.textContent = "Local only";
    status.className = "sync-badge subtle";
  } else if (incomingAssets.length > 0) {
    status.textContent = `${incomingAssets.length} waiting`;
    status.className = "sync-badge waiting";
  } else {
    status.textContent = "Lane empty";
    status.className = "sync-badge ok";
  }

  list.innerHTML = "";
  if (incomingAssets.length === 0) {
    const empty = document.createElement("li");
    empty.className = "incoming-asset-empty";
    empty.textContent = window.chattyEduBridge?.available
      ? "No incoming assets waiting in lesson_assets."
      : "Open this module inside Chatty-EDU to test incoming asset delivery.";
    list.appendChild(empty);
  } else {
    for (const asset of incomingAssets) {
      const item = document.createElement("li");
      item.className = `incoming-asset-item${asset.asset_id === selectedIncomingAssetId ? " selected" : ""}`;

      const titleRow = document.createElement("div");
      titleRow.className = "incoming-asset-title-row";

      const strong = document.createElement("strong");
      strong.textContent = incomingAssetDisplayName(asset);
      titleRow.appendChild(strong);

      const kind = document.createElement("span");
      kind.className = "incoming-asset-kind";
      kind.textContent = asset.kind || "asset";
      titleRow.appendChild(kind);

      const summary = document.createElement("div");
      summary.className = "incoming-asset-summary";
      summary.textContent = asset.summary?.trim() || incomingAssetMeta(asset);

      const metaLine = document.createElement("div");
      metaLine.className = "asset-meta";
      metaLine.textContent = incomingAssetMeta(asset);

      item.appendChild(titleRow);
      item.appendChild(summary);
      item.appendChild(metaLine);
      item.addEventListener("click", async () => {
        selectedIncomingAssetId = asset.asset_id;
        await refreshIncomingAssetPreview();
        renderIncomingAssets();
      });
      list.appendChild(item);
    }
  }

  const selected = selectedIncomingAsset();
  title.textContent = selected ? incomingAssetDisplayName(selected) : "No asset selected";
  meta.textContent = selected
    ? `${incomingAssetMeta(selected)}${selected.summary?.trim() ? ` | ${selected.summary.trim()}` : ""}`
    : "Pick an incoming asset to preview or import it into this lesson board.";
  preview.value = incomingAssetPreviewText;
  openButton.disabled = !selected;
  applyButton.disabled = !selected;
  consumeButton.disabled = !selected;
}

async function pollIncomingAssets() {
  if (!window.chattyEduBridge?.available) {
    incomingAssets = [];
    selectedIncomingAssetId = "";
    incomingAssetPreviewText = "(preview appears here)";
    renderIncomingAssets();
    return;
  }

  const nextAssets = await readChattyEduIncomingAssets(INCOMING_ASSET_LANE);
  incomingAssets = Array.isArray(nextAssets) ? nextAssets : [];
  if (!selectedIncomingAssetId || !incomingAssets.some((asset) => asset.asset_id === selectedIncomingAssetId)) {
    selectedIncomingAssetId = incomingAssets[0]?.asset_id || "";
    await refreshIncomingAssetPreview();
  }
  renderIncomingAssets();
}

function openSelectedIncomingAsset() {
  const asset = selectedIncomingAsset();
  if (!asset) {
    return;
  }
  const payloadUrl = chattyEduIncomingAssetUrl(INCOMING_ASSET_LANE, asset.payload_file_name);
  if (!payloadUrl) {
    return;
  }
  window.open(payloadUrl, "_blank", "noopener");
}

async function applySelectedIncomingAsset() {
  const asset = selectedIncomingAsset();
  if (!asset) {
    return;
  }

  const displayName = incomingAssetDisplayName(asset);
  const resourceLine = `- Imported asset: ${displayName}${asset.file_name ? ` [${asset.file_name}]` : ""}`;
  fields.resources.value = appendUniqueLine(fields.resources.value, resourceLine);

  if (incomingAssetLooksText(asset)) {
    try {
      const fullText = await loadIncomingAssetText(asset);
      if (fullText) {
        fields.teacher_note.value = appendNamedSection(
          fields.teacher_note.value,
          `Imported asset - ${displayName}`,
          fullText
        );
      }
    } catch (err) {
      const status = document.getElementById("incoming-asset-status");
      if (status) {
        status.textContent = "Import preview failed";
        status.className = "sync-badge waiting";
      }
      pushRoomToast("info", "Asset import warning", `Could not read ${displayName}. The resource link was still recorded.`);
    }
  }

  saveState();
  pushRoomToast("sync", "Asset imported", `${displayName} was added to this lesson board.`);
  const status = document.getElementById("incoming-asset-status");
  if (status) {
    status.textContent = `Imported ${displayName}`;
    status.className = "sync-badge ok";
  }
}

async function consumeSelectedIncomingAsset() {
  const asset = selectedIncomingAsset();
  if (!asset) {
    return;
  }
  const consumed = await consumeChattyEduIncomingAsset(INCOMING_ASSET_LANE, asset.asset_id);
  if (!consumed) {
    const status = document.getElementById("incoming-asset-status");
    if (status) {
      status.textContent = "Consume failed";
      status.className = "sync-badge waiting";
    }
    return;
  }
  pushRoomToast("info", "Asset consumed", `${incomingAssetDisplayName(asset)} was cleared from the lane.`);
  incomingAssets = incomingAssets.filter((item) => item.asset_id !== asset.asset_id);
  selectedIncomingAssetId = incomingAssets[0]?.asset_id || "";
  await refreshIncomingAssetPreview();
  renderIncomingAssets();
}

function normalizeRoomEvent(event) {
  if (!event || typeof event !== "object") {
    return null;
  }
  return {
    event_id: String(event.event_id || ""),
    event_type: String(event.event_type || "note"),
    label: String(event.label || event.event_type || "Room event"),
    payload_text: trimRoomEventText(event.payload_text),
    from_device_name: String(event.from_device_name || ""),
    local_echo: !!event.local_echo,
    received_at_unix_ms: eventTimestamp(event)
  };
}

function currentRoomEvents() {
  const shared = sharedRoomEvents
    .map(normalizeRoomEvent)
    .filter(Boolean);
  const sharedIds = new Set(shared.map((event) => event.event_id).filter(Boolean));
  optimisticRoomEvents = optimisticRoomEvents.filter((event) => {
    if (!event) {
      return false;
    }
    if (sharedIds.has(event.event_id)) {
      return false;
    }
    return Date.now() - event.received_at_unix_ms < 30_000;
  });

  return [...shared, ...optimisticRoomEvents]
    .sort((left, right) => eventTimestamp(right) - eventTimestamp(left))
    .slice(0, MAX_ROOM_EVENTS);
}

function updateRoomEventStatus() {
  const status = document.getElementById("room-event-status");
  if (!status) {
    return;
  }
  if (!window.chattyEduBridge?.available) {
    status.textContent =
      "Local activity feed only. Open this inside Chatty-EDU to test classroom room events.";
    return;
  }
  if (!latestRoomState?.active_for_module) {
    status.textContent =
      "Hosted locally. Start a lesson room session from Networking when you want connected peers to see these quick classroom signals.";
    return;
  }
  if (latestRoomState?.session_active) {
    status.textContent =
      "Classroom room events are live. These quick signals help peers coordinate without waiting for a full lesson revision.";
    return;
  }
  status.textContent =
    "Lesson room connected. Start a shared module session to turn these local signals into classroom events for peers.";
}

function renderRoomEvents() {
  const list = document.getElementById("room-event-list");
  if (!list) {
    return;
  }
  const events = currentRoomEvents();
  list.innerHTML = "";
  if (events.length === 0) {
    const empty = document.createElement("li");
    empty.className = "room-event-empty";
    empty.textContent = window.chattyEduBridge?.available
      ? "No classroom room activity yet."
      : "No local room activity yet.";
    list.appendChild(empty);
    return;
  }

  for (const event of events) {
    const item = document.createElement("li");
    item.className = `room-event-item${event.local_echo ? " local-echo" : ""}`;

    const title = document.createElement("div");
    title.className = "room-event-title";

    const strong = document.createElement("strong");
    strong.textContent = event.label || "Room event";
    title.appendChild(strong);

    const type = document.createElement("span");
    type.className = "room-event-type";
    type.textContent = event.event_type || "note";
    title.appendChild(type);

    const meta = document.createElement("div");
    meta.className = "room-event-meta";
    const actor = event.from_device_name || (event.local_echo ? "You" : "Unknown participant");
    meta.textContent = `${actor} - ${formatRelativeAge(Math.max(0, Date.now() - event.received_at_unix_ms))}`;

    item.appendChild(title);
    item.appendChild(meta);

    if (event.payload_text) {
      const payload = document.createElement("div");
      payload.className = "room-event-payload";
      payload.textContent = event.payload_text;
      item.appendChild(payload);
    }

    list.appendChild(item);
  }
}

function renderRoomToasts() {
  const stack = document.getElementById("room-toast-stack");
  if (!stack) {
    return;
  }
  stack.innerHTML = "";
  for (const toast of roomToasts) {
    const item = document.createElement("div");
    item.className = `room-toast ${toast.kind || "info"}`;

    const title = document.createElement("div");
    title.className = "room-toast-title";

    const strong = document.createElement("strong");
    strong.textContent = toast.title;
    title.appendChild(strong);

    const age = document.createElement("span");
    age.className = "room-toast-age";
    age.textContent = "now";
    title.appendChild(age);

    const detail = document.createElement("div");
    detail.className = "room-toast-detail";
    detail.textContent = toast.detail;

    item.appendChild(title);
    item.appendChild(detail);
    stack.appendChild(item);
  }
}

function pruneRoomToasts() {
  const cutoff = Date.now() - ROOM_TOAST_MS;
  roomToasts = roomToasts.filter((toast) => toast.created_at_unix_ms >= cutoff);
}

function pushRoomToast(kind, title, detail) {
  roomToasts = [
    {
      id: makeRoomEventId(),
      kind,
      title,
      detail,
      created_at_unix_ms: Date.now()
    },
    ...roomToasts
  ].slice(0, 4);
  renderRoomToasts();
}

function connectedParticipantMap(roomState) {
  const participants = Array.isArray(roomState?.participants) ? roomState.participants : [];
  const map = new Map();
  for (const participant of participants) {
    if (!participant || participant.connected === false) {
      continue;
    }
    const deviceId = String(participant.device_id || "").trim();
    if (!deviceId) {
      continue;
    }
    map.set(deviceId, participant);
  }
  return map;
}

function syncParticipantToasts(previousRoomState, nextRoomState, previousSessionKey, nextSessionKey) {
  if (!nextRoomState?.active_for_module || !nextRoomState?.session_active) {
    return;
  }
  if (!previousRoomState?.active_for_module || !previousRoomState?.session_active || previousSessionKey !== nextSessionKey) {
    return;
  }

  const previousParticipants = connectedParticipantMap(previousRoomState);
  const nextParticipants = connectedParticipantMap(nextRoomState);

  for (const [deviceId, participant] of nextParticipants) {
    if (previousParticipants.has(deviceId) || participant.is_local) {
      continue;
    }
    pushRoomToast(
      "join",
      "Participant joined",
      `${participant.device_name || deviceId} joined the lesson room.`
    );
  }

  for (const [deviceId, participant] of previousParticipants) {
    if (nextParticipants.has(deviceId) || participant.is_local) {
      continue;
    }
    pushRoomToast(
      "leave",
      "Participant left",
      `${participant.device_name || deviceId} left the lesson room.`
    );
  }
  pruneRoomToasts();
  renderRoomToasts();
}

function syncSessionLifecycleToasts(previousRoomState, nextRoomState, previousSessionKey, nextSessionKey) {
  const previousActive = !!(previousRoomState?.active_for_module && previousRoomState?.session_active);
  const nextActive = !!(nextRoomState?.active_for_module && nextRoomState?.session_active);

  if (!previousActive && !nextActive) {
    return;
  }

  if (!previousActive && nextActive) {
    roomToasts = [];
    pushRoomToast(
      "info",
      "Lesson session started",
      `${nextRoomState?.session_label || nextRoomState?.session_id || "Lesson room session"} is now active.`
    );
    return;
  }

  if (previousActive && !nextActive) {
    roomToasts = [];
    pushRoomToast(
      "info",
      "Lesson session ended",
      `${previousRoomState?.session_label || previousRoomState?.session_id || "Lesson room session"} has ended.`
    );
    return;
  }

  if (previousSessionKey !== nextSessionKey) {
    roomToasts = [];
    pushRoomToast(
      "info",
      "New lesson session started",
      `${nextRoomState?.session_label || nextRoomState?.session_id || "Lesson room session"} replaced the previous classroom session.`
    );
  }
}

function syncTurnToasts(previousRoomState, nextRoomState, previousSessionKey, nextSessionKey) {
  if (!nextRoomState?.active_for_module || !nextRoomState?.session_active) {
    return;
  }
  if (!previousRoomState?.active_for_module || !previousRoomState?.session_active || previousSessionKey !== nextSessionKey) {
    return;
  }

  const previousTalkingStick = String(previousRoomState?.turn_mode || "").toLowerCase().includes("talking");
  const nextTalkingStick = String(nextRoomState?.turn_mode || "").toLowerCase().includes("talking");
  if (!nextTalkingStick) {
    return;
  }

  const previousHasTurn = previousRoomState?.local_has_turn !== false;
  const nextHasTurn = nextRoomState?.local_has_turn !== false;

  if (!previousTalkingStick && nextTalkingStick) {
    pushRoomToast(
      "turn",
      nextHasTurn ? "Talking stick is yours" : "Talking stick mode active",
      nextHasTurn
        ? "You can edit now and prepare the next lesson revision."
        : "Another participant currently has the stick. You can still send quick classroom signals."
    );
    return;
  }

  if (!previousHasTurn && nextHasTurn) {
    pushRoomToast(
      "turn",
      "Your turn to contribute",
      "The talking stick has been passed to you. Lesson editing is unlocked."
    );
    return;
  }

  if (previousHasTurn && !nextHasTurn) {
    pushRoomToast(
      "turn",
      "Turn moved away",
      "Another participant now has the talking stick. Your copy is back in follow mode."
    );
  }
}

function queueLocalRoomEvent(event) {
  const normalized = normalizeRoomEvent(event);
  if (!normalized) {
    return;
  }
  optimisticRoomEvents = [normalized, ...optimisticRoomEvents.filter((item) => item.event_id !== normalized.event_id)].slice(
    0,
    MAX_ROOM_EVENTS
  );
  renderRoomEvents();
}

function emitRoomEvent(eventType, label, payloadText) {
  const event = {
    event_id: makeRoomEventId(),
    event_type: eventType,
    label,
    payload_text: trimRoomEventText(payloadText),
    content_type: "text/plain; charset=utf-8",
    from_device_name: "You",
    local_echo: true,
    created_at_unix_ms: Date.now(),
    received_at_unix_ms: Date.now()
  };
  queueLocalRoomEvent(event);
  const sent = emitChattyEduRoomEvent(event);
  updateRoomEventStatus();
  return sent;
}

function updateSyncIndicators() {
  const syncStatus = document.getElementById("sync-status");
  const roomRevision = Math.max(0, Number(latestRoomState?.session_revision || 0));
  const hostActivityAge = Math.max(0, Date.now() - Number(latestRoomState?.host_activity_updated_at_unix_ms || 0));
  const teacherEditing = String(latestRoomState?.host_activity_state || "").toLowerCase() === "editing" && hostActivityAge <= 8_000;
  const teacherActivityLabel = latestRoomState?.host_activity_label?.trim() || "Teacher is connected";

  if (!window.chattyEduBridge?.available) {
    if (syncStatus) {
      syncStatus.textContent = "local only";
    }
    setSyncBadge("sync-revision-badge", "Revision local", "local");
    setSyncBadge("sync-source-badge", "Standalone lesson board", "subtle");
    setSyncBadge("presence-badge", "Local only", "subtle");
    setSyncBadge("last-activity-badge", "Last activity local only", "subtle");
    return;
  }

  if (!latestRoomState?.active_for_module) {
    if (syncStatus) {
      syncStatus.textContent = "hosted local";
    }
    setSyncBadge("sync-revision-badge", "Revision local", "local");
    setSyncBadge("sync-source-badge", "Lesson room not active", "subtle");
    setSyncBadge("presence-badge", "No active lesson room", "subtle");
    setSyncBadge("last-activity-badge", "Last activity room inactive", "subtle");
    return;
  }

  if (latestRoomState.session_active && latestRoomState.local_is_host) {
    if (syncStatus) {
      syncStatus.textContent = "teacher host";
    }
    setSyncBadge("sync-revision-badge", `Hosting rev ${Math.max(1, roomRevision)}`, "host");
    setSyncBadge("sync-source-badge", "Learners follow this board", "subtle");
    setSyncBadge("presence-badge", teacherEditing ? "You are preparing the next lesson revision" : "You are the teacher host", teacherEditing ? "editing pulse" : "host");
    setSyncBadge(
      "last-activity-badge",
      latestRoomState?.host_activity_updated_at_unix_ms
        ? `Your last activity ${formatRelativeAge(hostActivityAge)}`
        : "Your activity not tracked yet",
      teacherEditing ? "editing" : "subtle"
    );
    return;
  }

  if (latestRoomState.session_active && latestRoomState.host_authoritative) {
    if (lastAppliedSharedRevision >= Math.max(1, roomRevision) && roomRevision > 0) {
      const justApplied = Date.now() < lastSyncFlashUntil;
      if (syncStatus) {
        syncStatus.textContent = justApplied ? "just applied" : "synced";
      }
      setSyncBadge(
        "sync-revision-badge",
        justApplied ? `Applied rev ${lastAppliedSharedRevision}` : `Synced rev ${lastAppliedSharedRevision}`,
        justApplied ? "ok celebrate" : "ok"
      );
      setSyncBadge(
        "sync-source-badge",
        justApplied
          ? `Just applied from ${lastAppliedSharedFrom || latestRoomState.host_device_name || "teacher"}`
          : `Last from ${lastAppliedSharedFrom || latestRoomState.host_device_name || "teacher"}`,
        "subtle"
      );
      return;
    }

    if (syncStatus) {
      syncStatus.textContent = lastAppliedSharedRevision > 0 ? "out of date" : "waiting";
    }
    setSyncBadge(
      "sync-revision-badge",
      roomRevision > 0
        ? lastAppliedSharedRevision > 0
          ? `Out of date - rev ${Math.max(1, roomRevision)}`
          : `Awaiting rev ${Math.max(1, roomRevision)}`
        : "Awaiting first teacher revision",
      "waiting pulse"
    );
    setSyncBadge(
      "sync-source-badge",
      lastAppliedSharedRevision > 0
        ? `Last applied rev ${lastAppliedSharedRevision} - teacher is ahead`
        : "No teacher revision applied yet",
      "subtle"
    );
    setSyncBadge(
      "presence-badge",
      teacherEditing ? teacherActivityLabel : "Teacher idle / between revisions",
      teacherEditing ? "editing pulse" : "subtle"
    );
    setSyncBadge(
      "last-activity-badge",
      latestRoomState?.host_activity_updated_at_unix_ms
        ? `Last teacher activity ${formatRelativeAge(hostActivityAge)}`
        : "Teacher activity not seen yet",
      teacherEditing ? "editing" : "subtle"
    );
    return;
  }

  if (syncStatus) {
    syncStatus.textContent = latestRoomState.session_active ? "shared session" : "lesson room";
  }
  setSyncBadge(
    "sync-revision-badge",
    latestRoomState.session_active ? `Shared rev ${Math.max(1, roomRevision)}` : "Revision local",
    latestRoomState.session_active ? "ok" : "local"
  );
  setSyncBadge("sync-source-badge", "Local lesson edits are allowed here", "subtle");
  setSyncBadge(
    "presence-badge",
    teacherEditing ? teacherActivityLabel : "Lesson room active",
    teacherEditing ? "editing pulse" : "subtle"
  );
  setSyncBadge(
    "last-activity-badge",
    latestRoomState?.host_activity_updated_at_unix_ms
      ? `Last teacher activity ${formatRelativeAge(hostActivityAge)}`
      : "Teacher activity not seen yet",
    teacherEditing ? "editing" : "subtle"
  );
}

function setEditorsLocked(locked, detail) {
  document.body.classList.toggle("room-locked", locked);
  for (const element of Object.values(fields)) {
    element.readOnly = locked;
  }
  document.getElementById("save-state").disabled = locked;
  document.getElementById("reset-state").disabled = locked;
  document.getElementById("room-policy-heading").textContent = locked ? "Mirroring teacher" : "Local editing";
  document.getElementById("room-policy-detail").textContent = detail;
}

function updateRoomActionHint({ active, sessionActive, hostAuthoritative, localIsHost, localHasTurn, talkingStick }) {
  const heading = document.getElementById("room-action-heading");
  const title = document.getElementById("room-action-title");
  const hint = document.getElementById("room-action-hint");
  if (!heading || !title || !hint) {
    return;
  }

  if (!active) {
    heading.textContent = "Testing hint";
    title.textContent = "Teacher-led sync is available";
    hint.textContent = "Start a module session from Chatty-EDU Networking, then use the quick classroom-signal buttons below or share this lesson state when the board is ready.";
    return;
  }

  if (sessionActive && hostAuthoritative && localIsHost) {
    heading.textContent = "Teacher action";
    title.textContent = "Push lesson state now";
    hint.textContent = "You are leading this lesson session. Use the quick classroom-signal buttons for light coordination, then share the current lesson revision to learners when you are ready.";
    return;
  }

  if (sessionActive && hostAuthoritative && !localIsHost) {
    heading.textContent = "Learner action";
    title.textContent = "Following latest teacher revision";
    hint.textContent = "This copy is following the teacher-led session. It will apply the next shared lesson revision automatically when the teacher pushes state, while room-event buttons stay available for quick classroom signals.";
    return;
  }

  if (talkingStick && !localHasTurn) {
    heading.textContent = "Turn-based note";
    title.textContent = "Waiting for your turn";
    hint.textContent = "Another participant currently has the talking stick. Once it is passed to you, editing will unlock for your turn. You can still send a quick classroom signal in the meantime.";
    return;
  }

  if (sessionActive) {
    heading.textContent = "Room action";
    title.textContent = "Shared lesson session active";
    hint.textContent = "This lesson board is in a shared room session. Use quick classroom signals for light coordination, then push the current state when you want other participants to catch up.";
    return;
  }

  heading.textContent = "Connected room";
  title.textContent = "Ready for lesson session";
  hint.textContent = "The room is connected but no module session is active yet. Start one from Networking when you want everyone following the same lesson board.";
}

function applyRoomState(roomState) {
  const previousRoomState = latestRoomState;
  const previousSessionKey = activeSessionKey;
  latestRoomState = roomState && typeof roomState === "object" ? roomState : null;
  const active = !!latestRoomState?.active_for_module;
  const sessionActive = !!latestRoomState?.session_active;
  const hostAuthoritative = !!latestRoomState?.host_authoritative;
  const localIsHost = !!latestRoomState?.local_is_host;
  const localHasTurn = latestRoomState?.local_has_turn !== false;
  const talkingStick = String(latestRoomState?.turn_mode || "").toLowerCase().includes("talking");
  const participantCount = latestRoomState?.participant_count || latestRoomState?.participants?.length || 1;
  activeSessionKey = makeSessionKey(latestRoomState);

  if (activeSessionKey !== previousSessionKey) {
    lastAppliedSharedRevision = 0;
    lastAppliedSharedFrom = "";
    lastSyncFlashUntil = 0;
    sharedRoomEvents = [];
    optimisticRoomEvents = [];
    lastRoomEventsFingerprint = "";
  }

  const roomMode = !active
    ? "standalone"
    : sessionActive
      ? hostAuthoritative
        ? "teacher-led session"
        : "shared session"
      : "lesson room";
  document.getElementById("room-mode").textContent = roomMode;

  let role = "teacher only";
  if (active) {
    if (localIsHost) {
      role = hostAuthoritative ? "teacher / host" : "host";
    } else if (talkingStick && localHasTurn) {
      role = "participant with stick";
    } else if (sessionActive && hostAuthoritative) {
      role = "participant / follower";
    } else {
      role = "participant";
    }
  }
  document.getElementById("room-role").textContent = role;

  document.getElementById("room-session-heading").textContent = !active
    ? "No active room session"
    : sessionActive
      ? (latestRoomState.session_label || latestRoomState.session_id || "Active lesson session")
      : "Lesson room connected";
  document.getElementById("room-session-detail").textContent = !active
    ? "This copy is currently a local lesson board."
    : [
        `Participants: ${participantCount}.`,
        `AI mode: ${latestRoomState.ai_mode || "not set"}.`,
        `Turn mode: ${latestRoomState.turn_mode || "open"}.`,
        sessionActive ? `Revision: ${Math.max(1, Number(latestRoomState.session_revision || 0))}.` : "Waiting for the teacher to start a module session."
      ].join(" ");
  document.getElementById("room-participant-count").textContent = String(participantCount);
  document.getElementById("room-participants").textContent = describeParticipants(latestRoomState);

  let locked = false;
  let lockDetail = "Local editing is available.";
  if (active && sessionActive && hostAuthoritative && !localIsHost) {
    locked = true;
    lockDetail = "Teacher-led lesson session is active. This board mirrors the teacher's current revision.";
  } else if (active && talkingStick && !localHasTurn) {
    locked = true;
    lockDetail = "Talking stick is with another participant right now. Wait for your turn to edit.";
  } else if (active && localIsHost) {
    lockDetail = "You are leading the current lesson room session.";
  } else if (active) {
    lockDetail = "Lesson room is active. Local editing is allowed on this copy.";
  }
  setEditorsLocked(locked, lockDetail);
  updateRoomActionHint({
    active,
    sessionActive,
    hostAuthoritative,
    localIsHost,
    localHasTurn,
    talkingStick
  });
  syncSessionLifecycleToasts(previousRoomState, latestRoomState, previousSessionKey, activeSessionKey);
  syncParticipantToasts(previousRoomState, latestRoomState, previousSessionKey, activeSessionKey);
  syncTurnToasts(previousRoomState, latestRoomState, previousSessionKey, activeSessionKey);
  updateSyncIndicators();
  updateRoomEventStatus();
  renderRoomEvents();
}

function applyIncomingSharedState(incoming) {
  if (!incoming || typeof incoming !== "object" || !incoming.payload || typeof incoming.payload !== "object") {
    return false;
  }
  if (
    latestRoomState?.session_active &&
    latestRoomState?.host_authoritative &&
    latestRoomState?.local_is_host
  ) {
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
  const nextRevision = Math.max(1, Number(incoming.session_revision || latestRoomState?.session_revision || 0));
  const previousAppliedRevision = lastAppliedSharedRevision;
  lastAppliedSharedRevision = nextRevision;
  lastAppliedSharedFrom = incoming.from_device_name || incoming.authoritative_device_name || "teacher";
  if (changed) {
    lastSyncFlashUntil = Date.now() + 2200;
    saveLocalState(collectState());
    refreshDerivedUi();
    if (nextRevision > previousAppliedRevision) {
      pushRoomToast(
        "sync",
        "Lesson revision applied",
        `Applied teacher revision ${nextRevision} from ${lastAppliedSharedFrom || "teacher"}.`
      );
    }
  }
  updateSyncIndicators();
  return changed;
}

async function pollIncomingSharedState() {
  const incoming = await readChattyEduIncomingSharedState();
  if (!incoming || typeof incoming !== "object") {
    return;
  }
  applyIncomingSharedState(incoming);
}

async function pollSharedRoomState() {
  const roomState = await readChattyEduSharedRoomState();
  const fingerprint = JSON.stringify(roomState || null);
  if (fingerprint === lastRoomFingerprint) {
    return;
  }
  lastRoomFingerprint = fingerprint;
  applyRoomState(roomState);
  refreshDerivedUi();
}

async function pollSharedRoomEvents() {
  const roomEvents = await readChattyEduSharedRoomEvents();
  const fingerprint = JSON.stringify(roomEvents || null);
  if (fingerprint === lastRoomEventsFingerprint) {
    return;
  }
  lastRoomEventsFingerprint = fingerprint;
  sharedRoomEvents = Array.isArray(roomEvents?.events) ? roomEvents.events : [];
  updateRoomEventStatus();
  renderRoomEvents();
}

function sendPresetRoomEvent(eventType, label, payloadText) {
  const sent = emitRoomEvent(eventType, label, payloadText);
  const status = document.getElementById("room-event-status");
  if (!status) {
    return;
  }
  if (!window.chattyEduBridge?.available) {
    status.textContent = "Saved to the local classroom activity feed. Host this module inside Chatty-EDU to share it with peers.";
    return;
  }
  status.textContent = sent
    ? "Classroom signal sent. Connected peers should see it in their room-event feed shortly."
    : "Classroom signal saved locally, but the bridge could not send it right now.";
}

function refreshDerivedUi() {
  const state = collectState();
  const blockLines = meaningfulLines(fields.main_blocks.value);
  const resourceLines = meaningfulLines(fields.resources.value);
  const summary = buildSummary(blockLines.length, resourceLines.length);
  const snapshot = buildSnapshot(blockLines, resourceLines);
  const sharedState = buildSharedState(state, blockLines, resourceLines, summary);

  document.getElementById("block-count").textContent = String(blockLines.length);
  document.getElementById("resource-count").textContent = String(resourceLines.length);
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
    tags: ["teacher", "lesson", "planning", "webview", "demo", "room_aware"],
    payload: {
      className: fields.class_name.value.trim(),
      blockCount: blockLines.length,
      resourceCount: resourceLines.length,
      room: latestRoomState
        ? {
            activeForModule: !!latestRoomState.active_for_module,
            sessionActive: !!latestRoomState.session_active,
            sessionRevision: Number(latestRoomState.session_revision || 0),
            participantCount: latestRoomState.participant_count || latestRoomState.participants?.length || 0
          }
        : null
    }
  }));
  updateChattyEduBridgeSharedState(sharedState);
  updateSyncIndicators();
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
document.getElementById("event-ready").addEventListener("click", () => {
  const className = fields.class_name.value.trim() || "Class";
  sendPresetRoomEvent("class_ready", "Class ready", `${className} is ready for the next lesson step.`);
});
document.getElementById("event-help").addEventListener("click", () => {
  const goal = fields.lesson_goal.value.trim() || "the current lesson goal";
  sendPresetRoomEvent("need_help", "Need help", `Need a quick clarification or support before continuing with ${goal}.`);
});
document.getElementById("event-next").addEventListener("click", () => {
  const nextBlock = meaningfulLines(fields.main_blocks.value)[0] || "the next activity";
  sendPresetRoomEvent("move_next", "Move to next activity", `Ready to move into ${nextBlock}.`);
});
document.getElementById("event-note").addEventListener("click", () => {
  const input = document.getElementById("room-note-input");
  const note = input.value.trim();
  if (!note) {
    const status = document.getElementById("room-event-status");
    if (status) {
      status.textContent = "Write a short room note first, then send it.";
    }
    return;
  }
  sendPresetRoomEvent("room_note", "Room note", note);
  input.value = "";
});
document.getElementById("room-note-input").addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    document.getElementById("event-note").click();
  }
});
document.getElementById("incoming-asset-open").addEventListener("click", openSelectedIncomingAsset);
document.getElementById("incoming-asset-apply").addEventListener("click", () => {
  applySelectedIncomingAsset();
});
document.getElementById("incoming-asset-consume").addEventListener("click", () => {
  consumeSelectedIncomingAsset();
});

restoreState();
applyRoomState(null);
renderIncomingAssets();
pollIncomingAssets();
pollIncomingSharedState();
pollSharedRoomState();
pollSharedRoomEvents();
window.setInterval(pollIncomingAssets, 3000);
window.setInterval(pollIncomingSharedState, 2500);
window.setInterval(pollSharedRoomState, 2500);
window.setInterval(pollSharedRoomEvents, 1500);
window.setInterval(() => {
  pruneRoomToasts();
  renderRoomToasts();
}, 1000);
