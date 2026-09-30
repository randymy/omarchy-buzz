import QtQuick
import Quickshell
import Quickshell.Io
import "Identicon.js" as Identicon
import "AnsiArt.js" as AnsiArt

// Client of the separate agent service (docs/AGENTS_SERVICE.md). It sends only
// the structured requests of that interface and renders validated status; it
// never holds keys, tokens, provider credentials or command lines. Every frame
// is checked exactly and an unexpected shape ends the session.
Item {
  id: root
  // The main helper service: room choices come only from its verified catalog.
  property var mainService: null
  property string helperExecutable: Quickshell.env("HOME") + "/.local/bin/omarchy-buzz"
  property bool autoConnect: false
  property string connection: "unavailable"
  property string category: "agent_service_unavailable"
  property bool sessionFailed: true
  property string instanceId: ""
  property bool capabilitySupported: false
  property var harnesses: []
  property var agents: []
  property var pending: null
  readonly property bool available: connection === "ready" && capabilitySupported && !sessionFailed

  // The one mutating request this panel has in flight, correlated by UUID.
  property string requestId: ""
  property string requestType: ""
  property string requestAgent: ""
  property string requestState: "idle"
  property string requestCategory: ""
  property var requestKnownIds: []
  signal agentCreated(string agentId)

  readonly property var harnessIds: ["claude-code", "codex"]
  readonly property var errorCategories: ["agent_invalid", "agent_busy", "agent_limit", "harness_missing", "not_signed_in",
    "enroll_failed", "unit_failed", "workspace_refused", "relay_unavailable"]
  readonly property var mutatingTypes: ["create_agent", "update_agent", "delete_agent", "enroll_agent", "start_agent",
    "stop_agent", "set_start_at_login", "sign_in"]
  readonly property var personaFields: ["name", "description", "instructions", "harness", "model", "rooms", "respondTo", "workspace"]
  readonly property bool serviceWorking: !!pending && pending.state === "working"
  readonly property bool busy: requestState === "working" || serviceWorking
  readonly property bool canMutate: available && !busy && bridge.running && instanceId !== ""

  function harnessLabel(id) { return ({"claude-code": "Claude Code", codex: "Codex"})[id] || id }
  function harness(id) { return harnesses.find(function(entry) { return entry.id === id }) || null }
  // Avatar key: the agent's public key once enrolled, else its persona id.
  function avatarKey(entry) { return entry ? (entry.enrolled && entry.identity ? entry.identity : entry.id) : "" }

  // Pasted avatar art, stored only on this machine in the plugin's state
  // directory beside the last-room memory, keyed by persona id. The service's
  // persona record has no avatar field yet; this moves into the persona record
  // (and the service contract) when the service gains one. Nothing here is sent.
  // The same file keeps this user's own avatar keyed by their 64-hex public key.
  // That is local only until profile avatars are published to the relay: other
  // people keep seeing this user's identicon. Art is plain pasted text (6 × 12)
  // or sanitized ANSI grid art from a file (AnsiArt.js), never a file path.
  // Grid art is stored as {art, brightness} and shown auto-leveled at that
  // brightness; a plain string (the earlier form) is shown as stored.
  readonly property string avatarStateDir: (Quickshell.env("XDG_STATE_HOME").startsWith("/")
    ? Quickshell.env("XDG_STATE_HOME") : Quickshell.env("HOME") + "/.local/state") + "/omarchy-buzz"
  readonly property string avatarsPath: avatarStateDir + "/avatars.json"
  property var avatarArt: ({})
  // Brightness per id, for grid art stored in the {art, brightness} form only.
  property var avatarBrightness: ({})
  property bool avatarWritePending: false
  function avatarArtFor(id) { return typeof id === "string" && avatarArt.hasOwnProperty(id) ? avatarArt[id] : "" }
  // 0 when the art is shown as stored.
  function avatarBrightnessFor(id) { return typeof id === "string" && avatarBrightness.hasOwnProperty(id) ? avatarBrightness[id] : 0 }
  // The store id for a message author: this user's own key, or this machine's enrolled agent's persona id.
  function avatarIdForKey(key) {
    if (/^[a-f0-9]{64}$/.test(key) && avatarArt.hasOwnProperty(key)) return key
    var entry = agents.find(function(candidate) { return candidate.enrolled && candidate.identity === key })
    return entry ? entry.id : ""
  }
  function avatarArtForKey(key) { return avatarArtFor(avatarIdForKey(key)) }
  function avatarBrightnessForKey(key) { return avatarBrightnessFor(avatarIdForKey(key)) }
  function validatedAvatars(raw) {
    // Missing or malformed files fail closed: no art is shown from them.
    var parsed
    try { parsed = JSON.parse(raw) } catch (_) { return {art: {}, brightness: {}} }
    if (!exactKeys(parsed, "avatars,version") || parsed.version !== 1 || !parsed.avatars
        || typeof parsed.avatars !== "object" || Array.isArray(parsed.avatars)) return {art: {}, brightness: {}}
    var ids = Object.keys(parsed.avatars)
    // Up to 16 agents plus a few of this user's own identities.
    var none = {art: {}, brightness: {}}
    if (ids.length > 32) return none
    var result = {art: {}, brightness: {}}
    for (var i = 0; i < ids.length; i++) {
      var entry = parsed.avatars[ids[i]], art = entry
      if (entry && typeof entry === "object" && !Array.isArray(entry)) {
        if (!exactKeys(entry, "art,brightness") || !AnsiArt.isAnsi(entry.art) || !AnsiArt.validBrightness(entry.brightness)) return none
        art = entry.art
        result.brightness[ids[i]] = entry.brightness
      }
      if (!(uuidV4(ids[i]) || /^[a-f0-9]{64}$/.test(ids[i])) || typeof art !== "string" || art === ""
          || art.length > 1048576 || AnsiArt.storedArt(art) !== art) return none
      result.art[ids[i]] = art
    }
    return result
  }
  // brightness applies to grid art only: 0.5–3 in steps of 0.25; when omitted
  // the current brightness is kept, or new grid art starts at 1.5.
  function setAvatarArt(id, text, brightness) {
    if (!agent(id) || typeof text !== "string") return false
    return storeAvatarArt(id, text, true, brightness)
  }
  // This user's own avatar, keyed by their public key (see above: local only).
  function setOwnAvatarArt(key, text, brightness) {
    if (typeof key !== "string" || !/^[a-f0-9]{64}$/.test(key) || typeof text !== "string") return false
    return storeAvatarArt(key, text, false, brightness)
  }
  function storeAvatarArt(id, text, prune, brightness) {
    var art = AnsiArt.storedArt(text)
    var level = !AnsiArt.isAnsi(art) ? 0 : typeof brightness === "number" ? AnsiArt.clampBrightness(brightness)
      : avatarBrightnessFor(id) || AnsiArt.DEFAULT_BRIGHTNESS
    if (avatarArtFor(id) === art && avatarBrightnessFor(id) === level) return true
    // Keep own-key art, and only agents the service still lists, so deleted
    // agents' art is dropped. Own art is set without pruning: the agent list
    // may be empty while the agent service is away.
    var next = {}, nextBrightness = {}
    Object.keys(avatarArt).forEach(function(key) {
      if (key === id || (prune && uuidV4(key) && !root.agent(key))) return
      next[key] = root.avatarArt[key]
      if (root.avatarBrightness.hasOwnProperty(key)) nextBrightness[key] = root.avatarBrightness[key]
    })
    if (art !== "") next[id] = art
    if (art !== "" && level > 0) nextBrightness[id] = level
    avatarBrightness = nextBrightness
    avatarArt = next
    avatarWritePending = true
    writeAvatars()
    return true
  }
  function writeAvatars() {
    if (!avatarWritePending || !mainService || !mainService.notificationSettingsDirReady) return
    var avatars = {}
    Object.keys(avatarArt).forEach(function(key) {
      avatars[key] = root.avatarBrightness.hasOwnProperty(key) ? {art: root.avatarArt[key], brightness: root.avatarBrightness[key]} : root.avatarArt[key]
    })
    avatarsFile.setText(JSON.stringify({version: 1, avatars: avatars}) + "\n")
    avatarWritePending = false
  }
  Connections {
    target: root.mainService
    function onNotificationSettingsDirReadyChanged() { root.writeAvatars() }
  }
  FileView {
    id: avatarsFile
    path: root.avatarsPath
    watchChanges: false
    atomicWrites: true
    // Written before setAvatarArt returns, so a fresh reader sees the new art.
    blockWrites: true
    printErrors: false
    blockLoading: true
    onLoaded: root.loadAvatars()
  }
  function loadAvatars() {
    if (avatarWritePending) return
    var loaded = validatedAvatars(avatarsFile.text())
    avatarBrightness = loaded.brightness
    avatarArt = loaded.art
  }
  // blockLoading makes text() wait for the file, so art is there on first render.
  Component.onCompleted: loadAvatars()
  function agent(id) { return agents.find(function(entry) { return entry.id === id }) || null }
  function statusWord(entry) {
    if (!entry) return ""
    if (!entry.enrolled) return "not enrolled"
    return ({active: "running", inactive: "stopped", failed: "failed"})[entry.unit] || "unknown"
  }
  function categorySentence(category) {
    return ({
      agent_invalid: "The agent service refused these settings.",
      agent_busy: "Another agent change is in progress. Try again shortly.",
      agent_limit: "The limit of 16 agents is reached.",
      harness_missing: "This harness is not installed on this machine.",
      not_signed_in: "Sign in to this harness first.",
      enroll_failed: "Enrollment did not complete.",
      unit_failed: "The agent's service unit failed.",
      workspace_refused: "The workspace was refused. Choose another directory.",
      relay_unavailable: "The relay could not be reached.",
      request_unknown: "No answer from the agent service. Check the agent's state before retrying."
    })[category] || "The request failed."
  }
  function requestLabel(type) {
    return ({create_agent: "Creating agent", update_agent: "Saving agent", delete_agent: "Deleting agent",
      enroll_agent: "Enrolling agent", start_agent: "Starting agent", stop_agent: "Stopping agent",
      set_start_at_login: "Changing start at login", sign_in: "Opening sign-in"})[type] || "Agent request"
  }
  // One line for the editor: this panel's request first, then the service's own.
  readonly property string statusLabel: {
    if (requestState === "working") return requestLabel(requestType) + "…"
    if (requestState === "done") return requestLabel(requestType) + " · done"
    if (requestState === "failed" || requestState === "unknown")
      return requestLabel(requestType) + " · failed. " + categorySentence(requestCategory)
    if (serviceWorking) return requestLabel(pending.type) + " elsewhere…"
    return ""
  }

  // Room choices: the main helper's verified joined rooms (not direct messages).
  readonly property var roomChoices: mainService && !mainService.sampleMode && !mainService.sessionFailed
    && mainService.connection === "authenticated" && ["partial", "ready"].indexOf(mainService.catalogState) !== -1
    ? mainService.streamRooms.map(function(room) { return {id: room.id, name: room.name} }) : []

  // Shared field rules, applied to what the service reports and to what is sent.
  function uuidValue(value) { return typeof value === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(value) }
  function uuidV4(value) { return typeof value === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(value) }
  function utf8Size(value) {
    try { return encodeURIComponent(value).replace(/%[A-F0-9]{2}/gi, "x").length }
    catch (_) { return Infinity }
  }
  function unsafeText(value) { return /[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069\u200e\u200f\u061c]/.test(value) }
  function nameProblem(value) {
    if (typeof value !== "string" || value.length < 1 || value.length > 64 || !value.trim()) return "Name must be 1 to 64 characters."
    if (unsafeText(value)) return "Name cannot contain control or direction characters."
    return ""
  }
  function descriptionProblem(value) {
    if (typeof value !== "string" || value.length > 256) return "Description must be at most 256 characters."
    if (unsafeText(value)) return "Description cannot contain control or direction characters."
    return ""
  }
  function instructionsProblem(value) {
    if (typeof value !== "string" || utf8Size(value) > 16384) return "Instructions must be at most 16 KiB."
    if (value.indexOf("\u0000") !== -1) return "Instructions cannot contain NUL characters."
    return ""
  }
  function modelProblem(value) {
    return typeof value === "string" && /^[A-Za-z0-9._:-]{0,64}$/.test(value) ? ""
      : "Model must be empty or up to 64 letters, digits and . _ : -"
  }
  function workspaceProblem(value, allowEmpty) {
    if (typeof value !== "string" || (value === "" && allowEmpty)) return typeof value === "string" ? "" : "Workspace must be a path."
    if (value.length > 4096 || value[0] !== "/" || unsafeText(value)) return "Workspace must be an absolute path."
    return ""
  }
  function roomsShapeValid(rooms) {
    return Array.isArray(rooms) && rooms.length >= 1 && rooms.length <= 8
      && rooms.every(function(id, index) { return root.uuidValue(id) && rooms.indexOf(id) === index })
  }
  function roomsProblem(rooms) {
    if (!roomsShapeValid(rooms)) return "Choose 1 to 8 rooms."
    if (!rooms.every(function(id) { return root.roomChoices.some(function(room) { return room.id === id }) }))
      return "Choose only rooms verified by the Buzz helper."
    return ""
  }
  // Complete persona fields as sent on create, or the changed subset on update.
  function fieldsProblem(fields, creating) {
    if (!fields || typeof fields !== "object" || Array.isArray(fields)) return "Invalid fields."
    var allowed = personaFields.concat(creating ? ["startAtLogin"] : [])
    var keys = Object.keys(fields)
    if (keys.some(function(key) { return allowed.indexOf(key) === -1 })) return "Invalid fields."
    if (creating && personaFields.some(function(key) { return !fields.hasOwnProperty(key) })) return "Invalid fields."
    if (!creating && keys.length === 0) return "Nothing changed."
    var checks = [
      fields.hasOwnProperty("name") ? nameProblem(fields.name) : "",
      fields.hasOwnProperty("description") ? descriptionProblem(fields.description) : "",
      fields.hasOwnProperty("instructions") ? instructionsProblem(fields.instructions) : "",
      fields.hasOwnProperty("harness") && harnessIds.indexOf(fields.harness) === -1 ? "Choose a harness." : "",
      fields.hasOwnProperty("model") ? modelProblem(fields.model) : "",
      fields.hasOwnProperty("rooms") ? roomsProblem(fields.rooms) : "",
      fields.hasOwnProperty("respondTo") && ["owner-only", "mentions"].indexOf(fields.respondTo) === -1 ? "Choose who the agent answers." : "",
      // Empty means the service's default workspace; only a new agent has none yet.
      fields.hasOwnProperty("workspace") ? workspaceProblem(fields.workspace, creating) : "",
      fields.hasOwnProperty("startAtLogin") && typeof fields.startAtLogin !== "boolean" ? "Invalid start at login." : ""
    ]
    return checks.find(function(text) { return text !== "" }) || ""
  }

  function validCapabilities(value) {
    return Array.isArray(value) && value.length <= 8 && value.every(function(cap, index) {
      return ["agent_manager"].indexOf(cap) !== -1 && value.indexOf(cap) === index
    })
  }
  function exactKeys(value, keys) {
    return !!value && typeof value === "object" && !Array.isArray(value) && Object.keys(value).sort().join(",") === keys
  }
  function validatedHarnesses(value) {
    if (!Array.isArray(value) || value.length > harnessIds.length) return null
    var result = []
    for (var i = 0; i < value.length; i++) {
      var entry = value[i]
      if (!exactKeys(entry, "bundle,id,signedIn") || harnessIds.indexOf(entry.id) === -1
          || result.some(function(other) { return other.id === entry.id })
          || ["ready", "missing"].indexOf(entry.bundle) === -1
          || (entry.signedIn !== null && typeof entry.signedIn !== "boolean")) return null
      result.push({id: entry.id, bundle: entry.bundle, signedIn: entry.signedIn})
    }
    return result
  }
  function validatedAgent(entry) {
    if (!exactKeys(entry, "acpCommand,description,enrolled,harness,id,identity,instructions,lastError,model,name,published,respondTo,rooms,startAtLogin,unit,workspace")
        || !uuidV4(entry.id) || nameProblem(entry.name) || descriptionProblem(entry.description)
        || instructionsProblem(entry.instructions) || harnessIds.indexOf(entry.harness) === -1
        || modelProblem(entry.model) || entry.acpCommand !== "buzz-acp" || !roomsShapeValid(entry.rooms)
        || ["owner-only", "mentions"].indexOf(entry.respondTo) === -1 || workspaceProblem(entry.workspace, false)
        || (entry.identity !== null && (typeof entry.identity !== "string" || !/^[a-f0-9]{64}$/.test(entry.identity)))
        || typeof entry.enrolled !== "boolean" || (entry.enrolled && entry.identity === null)
        || ["active", "inactive", "failed", "unknown"].indexOf(entry.unit) === -1
        || typeof entry.startAtLogin !== "boolean" || typeof entry.published !== "boolean"
        || (entry.lastError !== null && errorCategories.indexOf(entry.lastError) === -1)) return null
    return {id: entry.id, name: entry.name, description: entry.description, instructions: entry.instructions,
      harness: entry.harness, model: entry.model, acpCommand: entry.acpCommand, rooms: entry.rooms.slice(),
      respondTo: entry.respondTo, workspace: entry.workspace, identity: entry.identity, enrolled: entry.enrolled,
      unit: entry.unit, startAtLogin: entry.startAtLogin, published: entry.published, lastError: entry.lastError}
  }
  function validatedAgents(value) {
    if (!Array.isArray(value) || value.length > 16) return null
    var result = []
    for (var i = 0; i < value.length; i++) {
      var entry = validatedAgent(value[i])
      if (!entry || result.some(function(other) { return other.id === entry.id })) return null
      result.push(entry)
    }
    return result
  }
  function validatedPending(value) {
    if (value === null) return {value: null}
    if (!exactKeys(value, "category,requestId,state,type") || !uuidValue(value.requestId)
        || mutatingTypes.indexOf(value.type) === -1 || ["working", "done", "failed"].indexOf(value.state) === -1
        || (value.state === "failed") !== (value.category !== null)
        || (value.category !== null && errorCategories.indexOf(value.category) === -1)) return null
    return {value: {requestId: value.requestId, type: value.type, state: value.state, category: value.category}}
  }
  function validatedStatus(value) {
    if (!exactKeys(value, "agents,harnesses,pending")) return null
    var harnessList = validatedHarnesses(value.harnesses)
    var agentList = validatedAgents(value.agents)
    var pendingView = validatedPending(value.pending)
    if (!harnessList || !agentList || !pendingView) return null
    return {harnesses: harnessList, agents: agentList, pending: pendingView.value}
  }

  function clearData() {
    capabilitySupported = false
    if (harnesses.length) harnesses = []
    if (agents.length) agents = []
    pending = null
  }
  function loseRequest() {
    requestTimeout.stop()
    // A request whose answer never arrived has an unknown outcome, never a failure.
    if (requestState === "working") { requestState = "unknown"; requestCategory = "request_unknown" }
  }
  function fail(reason) {
    loseRequest()
    handshake.stop()
    clearData()
    sessionFailed = true
    instanceId = ""
    connection = "unavailable"
    category = reason
    bridge.running = false
  }
  function sameProjection(before, after) { return JSON.stringify(before) === JSON.stringify(after) }
  function acceptFrame(line) {
    if (sessionFailed) return false
    if (typeof line !== "string" || line.length > 1048576) { fail("invalid_response"); return false }
    var frame
    try { frame = JSON.parse(line) } catch (_) { fail("invalid_response"); return false }
    if (frame && frame.type === "error") {
      if (!exactKeys(frame, "category,id,instanceId,type,version") || frame.version !== 1 || instanceId === ""
          || frame.instanceId !== instanceId || !uuidValue(frame.id) || errorCategories.indexOf(frame.category) === -1) {
        fail("invalid_response"); return false
      }
      if (frame.id === requestId && requestState === "working") {
        requestTimeout.stop()
        requestState = "failed"
        requestCategory = frame.category
      }
      return true
    }
    if (!exactKeys(frame, "capabilities,id,instanceId,status,type,version") || frame.version !== 1
        || ["hello", "status"].indexOf(frame.type) === -1
        || typeof frame.instanceId !== "string" || frame.instanceId.length > 128 || !/^[A-Za-z0-9_-]+$/.test(frame.instanceId)
        || (frame.id !== null && !uuidValue(frame.id)) || !validCapabilities(frame.capabilities)) {
      fail("incompatible_response"); return false
    }
    if ((instanceId === "") !== (frame.type === "hello") || (instanceId !== "" && frame.instanceId !== instanceId)) {
      fail("invalid_response"); return false
    }
    var supported = frame.capabilities.indexOf("agent_manager") !== -1
    var status = null
    if (supported) {
      status = validatedStatus(frame.status)
      if (!status) { fail("invalid_response"); return false }
    }
    instanceId = frame.instanceId
    handshake.stop()
    connection = "ready"
    category = ""
    if (!supported) {
      // Without the capability nothing is offered and nothing is requested.
      loseRequest()
      clearData()
      return true
    }
    capabilitySupported = true
    if (!sameProjection(harnesses, status.harnesses)) harnesses = status.harnesses
    if (!sameProjection(agents, status.agents)) agents = status.agents
    if (!sameProjection(pending, status.pending)) pending = status.pending
    var view = status.pending
    if (view && view.requestId === requestId && requestState === "working" && view.state !== "working") {
      requestTimeout.stop()
      requestState = view.state
      requestCategory = view.category || ""
      if (view.state === "done" && requestType === "create_agent") {
        var known = requestKnownIds
        var added = agents.filter(function(entry) { return known.indexOf(entry.id) === -1 })
        if (added.length === 1) agentCreated(added[0].id)
      }
    }
    if (frame.type === "hello" && bridge.running) write({type: "subscribe"})
    return true
  }

  function correlationUuid() {
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function(c) {
      var n = Math.floor(Math.random() * 16)
      return (c === "x" ? n : (n & 3) | 8).toString(16)
    })
  }
  function write(request) {
    request.version = 1
    request.id = correlationUuid()
    request.instanceId = instanceId
    bridge.write(JSON.stringify(request) + "\n")
    return request.id
  }
  function mutate(request, agentId) {
    if (!canMutate) return false
    requestType = request.type
    requestAgent = agentId || ""
    requestKnownIds = agents.map(function(entry) { return entry.id })
    requestState = "working"
    requestCategory = ""
    requestId = write(request)
    requestTimeout.restart()
    return true
  }
  function copyFields(fields) {
    var copy = {}
    Object.keys(fields).forEach(function(key) { copy[key] = key === "rooms" ? fields.rooms.slice() : fields[key] })
    return copy
  }
  function createAgent(fields) {
    if (agents.length >= 16 || fieldsProblem(fields, true)) return false
    var copy = copyFields(fields)
    if (!copy.hasOwnProperty("startAtLogin")) copy.startAtLogin = false
    copy.acpCommand = "buzz-acp"
    return mutate({type: "create_agent", fields: copy}, "")
  }
  function updateAgent(id, fields) {
    if (!agent(id) || fieldsProblem(fields, false)) return false
    return mutate({type: "update_agent", agentId: id, fields: copyFields(fields)}, id)
  }
  function deleteAgent(id, forget) {
    if (!agent(id) || typeof forget !== "boolean") return false
    return mutate({type: "delete_agent", agentId: id, forget: forget}, id)
  }
  function enrollAgent(id) {
    var entry = agent(id)
    if (!entry || entry.enrolled) return false
    return mutate({type: "enroll_agent", agentId: id}, id)
  }
  function startAgent(id) {
    var entry = agent(id)
    if (!entry || !entry.enrolled || entry.unit === "active") return false
    return mutate({type: "start_agent", agentId: id}, id)
  }
  function stopAgent(id) {
    var entry = agent(id)
    if (!entry || entry.unit !== "active") return false
    return mutate({type: "stop_agent", agentId: id}, id)
  }
  function setStartAtLogin(id, enabled) {
    var entry = agent(id)
    if (!entry || typeof enabled !== "boolean" || entry.startAtLogin === enabled) return false
    return mutate({type: "set_start_at_login", agentId: id, enabled: enabled}, id)
  }
  function signIn(harnessId) {
    var entry = harness(harnessId)
    if (!entry || entry.signedIn === true) return false
    return mutate({type: "sign_in", harness: harnessId}, "")
  }
  function dismissRequest() {
    if (requestState === "working") return false
    requestState = "idle"
    requestCategory = ""
    return true
  }

  function beginSession() {
    loseRequest()
    clearData()
    instanceId = ""
    sessionFailed = false
    connection = "connecting"
    category = ""
  }
  function retry() {
    if (bridge.running && !sessionFailed) return
    if (!helperExecutable.startsWith("/")) { fail("agent_service_unavailable"); return }
    beginSession()
    bridge.running = true
    handshake.restart()
  }

  Timer {
    id: handshake
    interval: 5000
    onTriggered: root.fail("handshake_timeout")
  }
  Timer {
    id: requestTimeout
    // Enrollment publishes to the relay; wait well past it before calling the outcome unknown.
    interval: 60000
    onTriggered: root.loseRequest()
  }
  Process {
    id: bridge
    command: [root.helperExecutable, "agents-bridge"]
    stdinEnabled: true
    stdout: SplitParser { onRead: function(line) { root.acceptFrame(line) } }
    // Diagnostics are drained, never shown or logged.
    stderr: SplitParser { onRead: function(line) {} }
    onExited: {
      root.loseRequest()
      handshake.stop()
      root.clearData()
      root.sessionFailed = true
      root.instanceId = ""
      root.connection = "unavailable"
      if (!root.category) root.category = "agent_service_unavailable"
    }
  }
}
