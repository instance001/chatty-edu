function updateChattyEduBridgeStatus(buildPayload) {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.updateStatus !== "function") {
    return false;
  }

  try {
    const payload = typeof buildPayload === "function" ? buildPayload() : buildPayload;
    if (!payload || typeof payload !== "object") {
      return false;
    }

    window.chattyEduBridge.updateStatus({
      event_type: "suspend_rundown",
      ...payload,
    });
    return true;
  } catch (err) {
    console.warn("Chatty-EDU bridge update failed", err);
    return false;
  }
}

function clearChattyEduBridgeStatus() {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.clearStatus !== "function") {
    return false;
  }

  try {
    window.chattyEduBridge.clearStatus();
    return true;
  } catch (err) {
    console.warn("Chatty-EDU bridge clear failed", err);
    return false;
  }
}

function updateChattyEduBridgeSharedState(buildPayload) {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.updateSharedState !== "function") {
    return false;
  }

  try {
    const payload = typeof buildPayload === "function" ? buildPayload() : buildPayload;
    if (!payload || typeof payload !== "object") {
      return false;
    }

    window.chattyEduBridge.updateSharedState(payload);
    return true;
  } catch (err) {
    console.warn("Chatty-EDU shared-state update failed", err);
    return false;
  }
}

function clearChattyEduBridgeSharedState() {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.clearSharedState !== "function") {
    return false;
  }

  try {
    window.chattyEduBridge.clearSharedState();
    return true;
  } catch (err) {
    console.warn("Chatty-EDU shared-state clear failed", err);
    return false;
  }
}

async function readChattyEduIncomingSharedState() {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.readIncomingSharedState !== "function") {
    return null;
  }

  try {
    return await window.chattyEduBridge.readIncomingSharedState();
  } catch (err) {
    console.warn("Chatty-EDU incoming shared-state read failed", err);
    return null;
  }
}

async function readChattyEduSharedRoomState() {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.readSharedRoomState !== "function") {
    return null;
  }

  try {
    return await window.chattyEduBridge.readSharedRoomState();
  } catch (err) {
    console.warn("Chatty-EDU shared-room read failed", err);
    return null;
  }
}

async function readChattyEduSharedRoomEvents() {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.readSharedRoomEvents !== "function") {
    return null;
  }

  try {
    return await window.chattyEduBridge.readSharedRoomEvents();
  } catch (err) {
    console.warn("Chatty-EDU shared-room events read failed", err);
    return null;
  }
}

async function readChattyEduIncomingAssets(laneId) {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.readIncomingAssets !== "function") {
    return [];
  }

  try {
    return await window.chattyEduBridge.readIncomingAssets(laneId);
  } catch (err) {
    console.warn("Chatty-EDU incoming-assets read failed", err);
    return [];
  }
}

function chattyEduIncomingAssetUrl(laneId, payloadFileName) {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.incomingAssetUrl !== "function") {
    return null;
  }

  try {
    return window.chattyEduBridge.incomingAssetUrl(laneId, payloadFileName);
  } catch (err) {
    console.warn("Chatty-EDU incoming-asset URL failed", err);
    return null;
  }
}

async function consumeChattyEduIncomingAsset(laneId, assetId) {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.consumeIncomingAsset !== "function") {
    return false;
  }

  try {
    return await window.chattyEduBridge.consumeIncomingAsset(laneId, assetId);
  } catch (err) {
    console.warn("Chatty-EDU incoming-asset consume failed", err);
    return false;
  }
}

function emitChattyEduRoomEvent(buildPayload) {
  if (!window.chattyEduBridge?.available || typeof window.chattyEduBridge.emitRoomEvent !== "function") {
    return false;
  }

  try {
    const payload = typeof buildPayload === "function" ? buildPayload() : buildPayload;
    if (!payload || typeof payload !== "object") {
      return false;
    }
    window.chattyEduBridge.emitRoomEvent(payload);
    return true;
  } catch (err) {
    console.warn("Chatty-EDU room event emit failed", err);
    return false;
  }
}
