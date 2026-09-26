import QtQuick
import Quickshell
import Quickshell.Io
import "SampleData.js" as SampleData

Item {
  id: root
  property var shell: null
  property var manifest: null
  // Only offscreen fixtures opt into sampleMode; production never loads sample rooms.
  property bool sampleMode: false
  property bool autoConnect: true
  property string helperExecutable: Quickshell.env("HOME") + "/.local/bin/omarchy-buzz"
  property string connection: "unavailable"
  property string category: "helper_unavailable"
  property string instanceId: ""
  property string relay: ""
  property bool sessionFailed: true
  property int generation: 0
  property int requestSequence: 0
  readonly property var sample: sampleMode ? SampleData.snapshot().payload : null
  readonly property var viewModel: sample || ({ community: "Buzz" })
  readonly property var rooms: sample ? sample.rooms : []
  property string selectedRoomId: "sample-general"
  readonly property var selectedRoom: rooms.find(function(room) { return room.id === root.selectedRoomId }) || null
  readonly property var messages: sample ? sample.messages.filter(function(message) { return message.roomId === root.selectedRoomId }) : []
  readonly property string barLabel: sampleMode ? "TEST" : ({unconfigured: "Setup", connecting: "Connecting", authenticated: "Connected", identity_locked: "Locked", disconnected: "Offline", unavailable: "Error"})[connection] || "Error"
  readonly property string barSymbol: sampleMode ? "T" : ({unconfigured: "?", connecting: "…", authenticated: "✓", identity_locked: "!", disconnected: "○", unavailable: "!"})[connection] || "!"
  readonly property string statusLabel: sampleMode ? "Sample data" : category === "identity_access_pending" ? "Waiting for secret store unlock" : ({
    unconfigured: "Setup required", connecting: "Connecting", authenticated: "Authenticated · history unavailable",
    identity_locked: "Identity locked", disconnected: "Disconnected", unavailable: "Helper unavailable"
  })[connection] || "Unavailable"
  readonly property string setupInstructions: (category === "config_unavailable" || category === "invalid_config") ? "The helper configuration could not be read or validated. Check your local omarchy-buzz configuration, then select Retry. Credentials do not belong in the configuration file." : (connection === "identity_locked" || category === "identity_access_pending") ? "Unlock your OS secret store, then select Retry. The existing identity is retained; no replacement identity is created." : connection === "authenticated" ? "Relay authentication succeeded. Room discovery, history synchronization, and message sending are not implemented yet. No unread or agent health claims are available." : "Install the helper and enable its user socket first.\nIn a terminal, run: omarchy-buzz setup relay wss://your-relay.example\nThen: omarchy-buzz setup identity enroll\nEnrollment uses hidden terminal input and your OS secret store. Never paste a private key into this panel.\nSelect Retry after setup. Authentication does not mean room history is synchronized; messaging arrives in a later milestone."

  function selectRoom(roomId) {
    if (rooms.some(function(room) { return room.id === roomId })) selectedRoomId = roomId
  }
  function beginSession() {
    instanceId = ""
    relay = ""
    sessionFailed = false
    generation = 0
    connection = "connecting"
    category = ""
  }
  function fail(reason) {
    handshake.stop()
    sessionFailed = true
    relay = ""
    connection = "unavailable"
    category = reason
    bridge.running = false
  }
  function boundedString(value, limit) { return typeof value === "string" && value.length <= limit }
  function acceptFrame(line) {
    if (sessionFailed) return false
    if (!boundedString(line, 65536)) { fail("invalid_response"); return false }
    var frame
    try { frame = JSON.parse(line) } catch (_) { fail("invalid_response"); return false }
    if (!frame || frame.version !== 1 || ["hello", "status"].indexOf(frame.type) === -1
        || !boundedString(frame.instanceId, 128) || !/^[A-Za-z0-9_-]+$/.test(frame.instanceId)
        || !Number.isInteger(frame.generation) || frame.generation < 1 || frame.generation > 2147483647
        || !frame.status || frame.status.generation !== frame.generation
        || !Array.isArray(frame.capabilities) || frame.capabilities.length !== 1 || frame.capabilities[0] !== "connection_status") {
      fail("incompatible_response"); return false
    }
    if (instanceId !== "" && (frame.instanceId !== instanceId || frame.generation < generation)) return false
    if ((instanceId === "" && frame.type !== "hello") || (instanceId !== "" && frame.type === "hello")) {
      fail("invalid_response"); return false
    }
    var state = frame.status
    var states = ["unconfigured", "connecting", "authenticated", "identity_locked", "disconnected", "unavailable"]
    var categories = ["identity_access_pending", "identity_missing", "identity_locked", "identity_invalid", "identity_unavailable", "auth_rejected", "relay_timeout", "relay_unavailable", "relay_protocol_error", "config_unavailable", "invalid_config"]
    if (states.indexOf(state.connection) === -1
        || (state.category !== null && categories.indexOf(state.category) === -1)
        || (state.identity !== null && (!boundedString(state.identity, 64) || !/^[a-f0-9]{64}$/.test(state.identity)))
        || (state.relay !== null && (!boundedString(state.relay, 2048) || !/^wss?:\/\//.test(state.relay) || state.relay.indexOf("@") !== -1))) {
      fail("invalid_response"); return false
    }
    instanceId = frame.instanceId
    generation = frame.generation
    relay = state.relay || ""
    connection = state.connection
    category = state.category || ""
    handshake.stop()
    if (frame.type === "hello" && bridge.running) send("subscribe")
    return true
  }
  function send(kind) {
    if (!sessionFailed && bridge.running && instanceId !== "") {
      requestSequence++
      bridge.write(JSON.stringify({ version: 1, id: "ui-" + requestSequence, type: kind }) + "\n")
    }
  }
  function retry() {
    if (sampleMode) return
    if (!sessionFailed && bridge.running && instanceId !== "") send("retry_connection")
    else {
      if (!helperExecutable.startsWith("/")) { fail("helper_unavailable"); return }
      beginSession()
      bridge.running = true
      handshake.restart()
    }
  }
  Component.onCompleted: if (autoConnect && !sampleMode) retry()

  Timer {
    id: handshake
    interval: 5000
    onTriggered: root.fail("handshake_timeout")
  }
  Process {
    id: bridge
    command: [root.helperExecutable, "ui-bridge"]
    stdinEnabled: true
    stdout: SplitParser { onRead: function(line) { root.acceptFrame(line) } }
    // Drain diagnostics without exposing raw helper output or secrets to logs/UI.
    stderr: SplitParser { onRead: function(line) {} }
    onExited: {
      handshake.stop()
      root.sessionFailed = true
      root.relay = ""
      root.connection = "unavailable"
      if (!root.category) root.category = "helper_unavailable"
    }
  }
}
