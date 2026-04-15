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
