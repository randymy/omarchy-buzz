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
  property string setupProvider: "hosted"
  property string helperExecutable: Quickshell.env("HOME") + "/.local/bin/omarchy-buzz"
  property string connection: "unavailable"
  property string category: "helper_unavailable"
  property string instanceId: ""
  property string relay: ""
  property bool sessionFailed: true
  property int generation: 0
  property int requestSequence: 0
  property string pendingHistoryRequestId: ""
  readonly property var sample: sampleMode ? SampleData.snapshot().payload : null
  readonly property var viewModel: sample || ({ community: "Buzz" })
  property var catalogRooms: []
  property string catalogState: "unavailable"
  property string catalogCategory: ""
  readonly property string catalogLabel: sampleMode ? "Sample rooms" : ({unavailable: "Rooms unavailable", loading: "Loading rooms", partial: "Partial list · " + catalogRooms.length + " shown (limit 20)", ready: catalogRooms.length ? "Joined rooms · " + catalogRooms.length : "No joined rooms"})[catalogState]
  readonly property var rooms: sample ? sample.rooms : catalogRooms
  property string selectedRoomId: sampleMode ? "sample-general" : ""
  readonly property var selectedRoom: rooms.find(function(room) { return room.id === root.selectedRoomId }) || null
  property var historyRows: []
  property string historyState: "unavailable"
  property string historyCategory: ""
  property bool historySupported: false
  property var historyHasMore: null
  readonly property var messages: sample ? sample.messages.filter(function(message) { return message.roomId === root.selectedRoomId }) : historyRows
  readonly property string historyLabel: historyState === "loading" ? "Loading recent snapshot" : historyState === "snapshot"
    ? "Snapshot · completeness unknown" + (historyHasMore ? " · older history available" : "") : ({request_busy: "Helper busy · refresh again", history_timeout: "History request timed out", history_invalid: "History response could not be validated", history_access_denied: "History unavailable for this room"})[historyCategory] || "History not available yet"
  readonly property string barLabel: sampleMode ? "TEST" : ({unconfigured: "Setup", connecting: "Connecting", authenticated: "Connected", identity_locked: "Locked", disconnected: "Offline", unavailable: "Error"})[connection] || "Error"
  readonly property string barSymbol: sampleMode ? "T" : ({unconfigured: "?", connecting: "…", authenticated: "✓", identity_locked: "!", disconnected: "○", unavailable: "!"})[connection] || "!"
  readonly property string statusLabel: sampleMode ? "Sample data" : category === "identity_access_pending" ? "Waiting for secret store unlock" : ({
    unconfigured: "Setup required", connecting: "Connecting", authenticated: historyState === "snapshot" ? "Authenticated · recent snapshot" : historyState === "loading" ? "Authenticated · history loading" : "Authenticated · history unavailable",
    identity_locked: "Identity locked", disconnected: "Disconnected", unavailable: "Helper unavailable"
  })[connection] || "Unavailable"
  readonly property string providerInstructions: setupProvider === "hosted"
    ? "Set up your account and identity binding at buzz.xyz. Create or join a community, then use its assigned URL. There is no single public global relay; invitations and membership still apply."
    : "Use the URL of a relay you already belong to, including one you were invited to. Choosing custom does not require running your own relay."
  readonly property string setupInstructions: (category === "config_unavailable" || category === "invalid_config")
    ? "Check your local helper configuration, then Retry. Credentials do not belong in that file."
    : (connection === "identity_locked" || category === "identity_access_pending")
      ? "Unlock your OS secret store, then Retry. Your existing identity is retained."
      : connection === "authenticated"
        ? "Relay authentication succeeded. Choose a joined room to fetch a recent snapshot. Messaging is not available yet."
        : providerInstructions + "\nLink manually in a terminal after installing the helper:\nomarchy-buzz setup relay <community-url>\nomarchy-buzz setup identity enroll\nEnroll the same existing Buzz identity using hidden input and your OS secret store, then Retry. Hosted account sign-in stays in your browser. Never enter keys or account tokens in this panel."

  function chooseSetupProvider(provider) {
    // Presentation only: choosing a provider never writes config or sends IPC.
    if (provider === "hosted" || provider === "custom") setupProvider = provider
  }

  function selectRoom(roomId) {
    if (rooms.some(function(room) { return room.id === roomId })) {
      if (selectedRoomId !== roomId) clearHistory()
      selectedRoomId = roomId
      if (!sampleMode) refreshHistory()
    }
  }
  function clearHistory() {
    pendingHistoryRequestId = ""
    historyRows = []
    historyState = "unavailable"
    historyCategory = ""
    historyHasMore = null
  }
  function refreshHistory() {
    clearHistory()
    if (sampleMode || !historySupported || connection !== "authenticated" || !selectedRoom) return
    historyState = "loading"
    send("fetch_recent", selectedRoomId)
  }
  function formatTimestamp(seconds) { return Qt.formatDateTime(new Date(seconds * 1000), "yyyy-MM-dd HH:mm:ss t") }
  function clearCatalog() {
    clearHistory()
    catalogRooms = []
    catalogState = "unavailable"
    catalogCategory = ""
    if (!sampleMode) selectedRoomId = ""
  }
  function validCapabilities(capabilities) {
    return Array.isArray(capabilities) && capabilities.length >= 1 && capabilities.length <= 3
      && capabilities.indexOf("connection_status") !== -1
      && capabilities.every(function(cap, index) {
        return ["connection_status", "room_catalog", "room_history"].indexOf(cap) !== -1 && capabilities.indexOf(cap) === index
      })
  }
  function validatedCatalog(catalog) {
    if (!catalog || ["unavailable", "loading", "partial", "ready"].indexOf(catalog.state) === -1
        || !Array.isArray(catalog.rooms) || catalog.rooms.length > 20
        || (catalog.category !== null && ["room_catalog_unavailable", "room_catalog_partial", "room_catalog_timeout", "room_catalog_invalid", "room_catalog_unsupported", "relay_identity_unavailable", "relay_identity_changed"].indexOf(catalog.category) === -1)) return null
    var clean = []
    var ids = ({})
    for (var i = 0; i < catalog.rooms.length; i++) {
      var room = catalog.rooms[i]
      if (!room || typeof room.id !== "string" || !/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(room.id)
          || ids[room.id] || !boundedString(room.name, 128) || !room.name.trim()
          || !boundedString(room.description, 512)) return null
      ids[room.id] = true
      clean.push({id: room.id, name: room.name, description: room.description})
    }
    if (["unavailable", "loading"].indexOf(catalog.state) !== -1 && clean.length !== 0) return null
    return {state: catalog.state, rooms: clean, category: catalog.category || ""}
  }
  function validatedHistory(history) {
    var uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/
    if (!history || ["unavailable", "loading", "snapshot"].indexOf(history.state) === -1
        || (history.roomId !== null && (typeof history.roomId !== "string" || !uuid.test(history.roomId)))
        || !Array.isArray(history.rows) || history.rows.length > 20
        || (history.hasMore !== null && typeof history.hasMore !== "boolean")
        || (history.category !== null && ["history_unavailable", "history_timeout", "history_invalid", "history_access_denied", "history_completeness_unknown"].indexOf(history.category) === -1)) return null
    if (history.state === "snapshot" && (history.roomId === null || typeof history.hasMore !== "boolean")) return null
    if (history.state !== "snapshot" && history.rows.length !== 0) return null
    var clean = []
    var ids = ({})
    for (var i = 0; i < history.rows.length; i++) {
      var row = history.rows[i]
      if (!row || typeof row.id !== "string" || !/^[a-f0-9]{64}$/.test(row.id) || ids[row.id]
          || typeof row.author !== "string" || !/^[a-f0-9]{64}$/.test(row.author)
          || !Number.isInteger(row.time) || row.time < 0 || row.time > 253402300799
          || !boundedString(row.text, 2048) || typeof row.edited !== "boolean"
          || typeof row.truncated !== "boolean" || typeof row.unavailable !== "boolean") return null
      try { if (encodeURIComponent(row.text).replace(/%[A-F0-9]{2}/gi, "x").length > 2048) return null }
      catch (_) { return null }
      ids[row.id] = true
      clean.push({id: row.id, author: row.author, time: row.time, text: row.unavailable ? "" : row.text,
        edited: row.edited, truncated: row.truncated, unavailable: row.unavailable})
    }
    return {state: history.state, roomId: history.roomId, rows: clean, hasMore: history.hasMore, category: history.category || ""}
  }
  function beginSession() {
    clearCatalog()
    instanceId = ""
    relay = ""
    sessionFailed = false
    historySupported = false
    generation = 0
    connection = "connecting"
    category = ""
  }
  function fail(reason) {
    handshake.stop()
    clearCatalog()
    sessionFailed = true
    historySupported = false
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
    if (frame && frame.version === 1 && frame.type === "error" && frame.category === "request_busy") {
      if (instanceId === "" || frame.instanceId !== instanceId) return false
      if (!boundedString(frame.id, 128) || !/^ui-[0-9]+$/.test(frame.id)) { fail("invalid_response"); return false }
      if (frame.id === pendingHistoryRequestId) {
        clearHistory()
        historyCategory = "request_busy"
      }
      // A bounded queue rejection is recoverable and never retries automatically.
      return true
    }
    if (!frame || frame.version !== 1 || ["hello", "status"].indexOf(frame.type) === -1
        || !boundedString(frame.instanceId, 128) || !/^[A-Za-z0-9_-]+$/.test(frame.instanceId)
        || !Number.isInteger(frame.generation) || frame.generation < 1 || frame.generation > 2147483647
        || !frame.status || frame.status.generation !== frame.generation
        || !validCapabilities(frame.capabilities)) {
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
    var catalog = {state: "unavailable", rooms: [], category: ""}
    if (frame.capabilities.indexOf("room_catalog") !== -1) {
      catalog = validatedCatalog(state.catalog)
      if (!catalog) { fail("invalid_response"); return false }
    }
    var history = null
    var supportsHistory = frame.capabilities.indexOf("room_history") !== -1
    if (supportsHistory) {
      history = validatedHistory(state.history)
      if (!history) { fail("invalid_response"); return false }
    }
    var previousCatalogState = catalogState
    if (state.connection !== "authenticated") catalog = {state: "unavailable", rooms: [], category: ""}
    if (frame.generation !== generation || state.connection !== "authenticated") clearCatalog()
    var selectedBeforeCatalog = selectedRoomId
    catalogRooms = catalog.rooms
    catalogState = catalog.state
    catalogCategory = catalog.category
    if (["partial", "ready"].indexOf(catalogState) !== -1
        && !catalogRooms.some(function(room) { return room.id === root.selectedRoomId })) {
      clearHistory()
      selectedRoomId = catalogRooms.length ? catalogRooms[0].id : ""
    }
    historySupported = supportsHistory
    if (state.connection !== "authenticated" || !supportsHistory || ["loading", "unavailable"].indexOf(catalogState) !== -1) clearHistory()
    else if (history && history.roomId === selectedRoomId && selectedRoomId !== "") {
      historyRows = history.rows
      historyState = history.state
      historyCategory = history.category
      historyHasMore = history.hasMore
    }
    instanceId = frame.instanceId
    generation = frame.generation
    relay = state.relay || ""
    connection = state.connection
    category = state.category || ""
    handshake.stop()
    if (frame.type === "hello" && bridge.running) send("subscribe")
    // One fetch per first population/catalog refresh completion, never per status echo.
    if (supportsHistory && connection === "authenticated" && selectedRoom
        && (selectedBeforeCatalog === "" || previousCatalogState === "loading")
        && historyState !== "snapshot") refreshHistory()
    return true
  }
  function send(kind, roomId) {
    if (!sessionFailed && bridge.running && instanceId !== "") {
      requestSequence++
      var request = { version: 1, id: "ui-" + requestSequence, type: kind }
      if (kind === "fetch_recent") { request.roomId = roomId; pendingHistoryRequestId = request.id }
      bridge.write(JSON.stringify(request) + "\n")
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
      root.clearCatalog()
      root.historySupported = false
      root.sessionFailed = true
      root.relay = ""
      root.connection = "unavailable"
      if (!root.category) root.category = "helper_unavailable"
    }
  }
}
