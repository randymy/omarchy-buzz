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
  // The agent service's view of the active community (its configuration), or "".
  property string activeRelay: ""
  property var pending: null
  // The service's last model probe: {agentId, state, model, detail}; idle has no agent.
  property var modelProbe: ({agentId: null, state: "idle", model: "", detail: null})
  readonly property bool available: connection === "ready" && capabilitySupported && !sessionFailed

  // The one mutating request this panel has in flight, correlated by UUID.
  property string requestId: ""
  property string requestType: ""
  property string requestAgent: ""
  property string requestState: "idle"
  property string requestCategory: ""
  // A fixed detail code the service gave with a failure, or "".
  property string requestDetail: ""
  property var requestKnownIds: []
  signal agentCreated(string agentId)
  // A request that changes which rooms an agent is a member of has finished:
  // the open room's member list may now include or lack the agent.
  signal membershipChanged()

  readonly property var harnessIds: ["claude-code", "codex"]
  readonly property var errorCategories: ["agent_invalid", "agent_busy", "agent_limit", "harness_missing", "bundle_stale",
    "not_signed_in", "enroll_failed", "unit_failed", "workspace_refused", "relay_unavailable"]
  readonly property var mutatingTypes: ["create_agent", "update_agent", "delete_agent", "enroll_agent", "start_agent",
    "stop_agent", "set_start_at_login", "sign_in", "refresh_bundle", "probe_model", "enroll_agent_in", "leave_agent_community"]
  // One agent can be enrolled in up to this many communities (its instances).
  readonly property int maxInstances: 4
  readonly property var pendingDetails: ["model_not_for_harness"]
  // Model names per harness, as the service checks them (agents_service/models.rs).
  readonly property var modelAliases: ({"claude-code": ["opus", "sonnet", "haiku", "fable"], codex: []})
  readonly property var modelPatterns: ({"claude-code": /^claude-[a-z0-9-]+$/, codex: /^(gpt-[a-z0-9.-]+|o[0-9][a-z0-9-]*|codex-[a-z0-9.-]+)$/})
  readonly property var probeStates: ["idle", "running", "ok", "unavailable", "not_signed_in", "failed"]
  // The only sentences a probe result may carry; the probe's output is never sent.
  readonly property var probeSentences: ["The model answered.", "The provider does not offer this model to this account.",
    "The provider did not accept the harness sign-in. Sign in again.", "The probe did not finish in time.",
    "The sandbox launcher refused the probe.", "The provider is busy or a usage limit was reached. Try again later.",
    "The probe did not get the expected answer.", "The probe failed."]
  readonly property var personaFields: ["name", "description", "instructions", "harness", "model", "rooms", "respondTo", "workspace", "answersDms"]
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
  // brightness; a plain string (the earlier form) is shown as stored. Plain art
  // with a chosen color is stored as {art, color} (color: lowercase #rrggbb).
  // Colors read back are validated; an invalid one is dropped, never shown. The
  // agent editor sets no color, so agent avatars draw in the default color.
  readonly property string avatarStateDir: (Quickshell.env("XDG_STATE_HOME").startsWith("/")
    ? Quickshell.env("XDG_STATE_HOME") : Quickshell.env("HOME") + "/.local/state") + "/omarchy-buzz"
  readonly property string avatarsPath: avatarStateDir + "/avatars.json"
  property var avatarArt: ({})
  // Brightness per id, for grid art stored in the {art, brightness} form only.
  property var avatarBrightness: ({})
  // Color per id for plain art stored in the {art, color} form only.
  property var avatarTint: ({})
  property bool avatarWritePending: false
  function avatarArtFor(id) { return typeof id === "string" && avatarArt.hasOwnProperty(id) ? avatarArt[id] : "" }
  // 0 when the art is shown as stored.
  function avatarBrightnessFor(id) { return typeof id === "string" && avatarBrightness.hasOwnProperty(id) ? avatarBrightness[id] : 0 }
  // "" when the art is drawn in the default color.
  function avatarTintFor(id) { return typeof id === "string" && avatarTint.hasOwnProperty(id) ? avatarTint[id] : "" }
  // The store id for a message author: this user's own key, or this machine's enrolled agent's persona id.
  function avatarIdForKey(key) {
    if (/^[a-f0-9]{64}$/.test(key) && avatarArt.hasOwnProperty(key)) return key
    var entry = agents.find(function(candidate) { return candidate.enrolled && candidate.identity === key })
    return entry ? entry.id : ""
  }
  function avatarArtForKey(key) { return avatarArtFor(avatarIdForKey(key)) }
  function avatarBrightnessForKey(key) { return avatarBrightnessFor(avatarIdForKey(key)) }
  function avatarTintForKey(key) { return avatarTintFor(avatarIdForKey(key)) }
  function validatedAvatars(raw) {
    // Missing or malformed files fail closed: no art is shown from them.
    var parsed
    try { parsed = JSON.parse(raw) } catch (_) { return {art: {}, brightness: {}, tint: {}} }
    if (!exactKeys(parsed, "avatars,version") || parsed.version !== 1 || !parsed.avatars
        || typeof parsed.avatars !== "object" || Array.isArray(parsed.avatars)) return {art: {}, brightness: {}, tint: {}}
    var ids = Object.keys(parsed.avatars)
    // Up to 16 agents plus a few of this user's own identities.
    var none = {art: {}, brightness: {}, tint: {}}
    if (ids.length > 32) return none
    var result = {art: {}, brightness: {}, tint: {}}
    for (var i = 0; i < ids.length; i++) {
      var entry = parsed.avatars[ids[i]], art = entry
      if (entry && typeof entry === "object" && !Array.isArray(entry)) {
        var shape = Object.keys(entry).sort().join(",")
        if (["art,brightness", "art,color"].indexOf(shape) === -1) return none
        art = entry.art
        if (shape === "art,brightness") {
          if (!AnsiArt.isAnsi(art) || !AnsiArt.validBrightness(entry.brightness)) return none
          result.brightness[ids[i]] = entry.brightness
        } else if (AnsiArt.isAnsi(art)) return none
        else if (AnsiArt.validTint(entry.color)) result.tint[ids[i]] = entry.color
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
  // tint (plain art only): a lowercase #rrggbb, "" for the default color, or
  // omitted to keep the current one; anything else is refused.
  // This user's own avatar, keyed by their public key (see above: local only).
  function setOwnAvatarArt(key, text, brightness, tint) {
    if (typeof key !== "string" || !/^[a-f0-9]{64}$/.test(key) || typeof text !== "string") return false
    return storeAvatarArt(key, text, false, brightness, tint)
  }
  function storeAvatarArt(id, text, prune, brightness, tint) {
    if (tint !== undefined && tint !== "" && !AnsiArt.validTint(tint)) return false
    var art = AnsiArt.storedArt(text)
    var level = !AnsiArt.isAnsi(art) ? 0 : typeof brightness === "number" ? AnsiArt.clampBrightness(brightness)
      : avatarBrightnessFor(id) || AnsiArt.DEFAULT_BRIGHTNESS
    var shade = AnsiArt.isAnsi(art) || art === "" ? "" : tint === undefined ? avatarTintFor(id) : tint
    if (avatarArtFor(id) === art && avatarBrightnessFor(id) === level && avatarTintFor(id) === shade) return true
    // Keep own-key art, and only agents the service still lists, so deleted
    // agents' art is dropped. Own art is set without pruning: the agent list
    // may be empty while the agent service is away.
    var next = {}, nextBrightness = {}, nextTint = {}
    Object.keys(avatarArt).forEach(function(key) {
      if (key === id || (prune && uuidV4(key) && !root.agent(key))) return
      next[key] = root.avatarArt[key]
      if (root.avatarBrightness.hasOwnProperty(key)) nextBrightness[key] = root.avatarBrightness[key]
      if (root.avatarTint.hasOwnProperty(key)) nextTint[key] = root.avatarTint[key]
    })
    if (art !== "") next[id] = art
    if (art !== "" && level > 0) nextBrightness[id] = level
    if (art !== "" && shade !== "") nextTint[id] = shade
    avatarTint = nextTint
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
      avatars[key] = root.avatarBrightness.hasOwnProperty(key) ? {art: root.avatarArt[key], brightness: root.avatarBrightness[key]}
        : root.avatarTint.hasOwnProperty(key) ? {art: root.avatarArt[key], color: root.avatarTint[key]} : root.avatarArt[key]
    })
    avatarsFile.setText(JSON.stringify({version: 1, avatars: avatars}) + "\n")
    avatarWritePending = false
  }
  Connections {
    target: root.mainService
    function onNotificationSettingsDirReadyChanged() { root.writeAvatars() }
    // Another community became active: ask for status again, so the service's
    // own `activeRelay` follows the configuration it reads.
    function onRelayChanged() { root.resubscribe() }
  }
  function resubscribe() {
    if (available && bridge.running && instanceId !== "") write({type: "subscribe"})
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
    avatarTint = loaded.tint
    avatarArt = loaded.art
  }
  // blockLoading makes text() wait for the file, so art is there on first render.
  Component.onCompleted: loadAvatars()
  function agent(id) { return agents.find(function(entry) { return entry.id === id }) || null }

  // An agent is enrolled in one or more communities: its `instances`, the
  // first one first (the top-level relay, rooms and state repeat it). The
  // panel's active community is the one the main helper shows; the service's
  // own `activeRelay` stands in while the helper has none.
  function relayKey(value) { return typeof value === "string" ? value.toLowerCase().replace(/\/+$/, "") : "" }
  readonly property string currentRelay: mainService && !mainService.sampleMode && typeof mainService.relay === "string"
    && mainService.relay !== "" ? mainService.relay : activeRelay
  // The agent's instance in a community, or null.
  function instanceIn(entry, relay) {
    if (!entry || !entry.instances || relay === "") return null
    return entry.instances.find(function(instance) { return root.relayKey(instance.relay) === root.relayKey(relay) }) || null
  }
  function currentInstance(entry) { return instanceIn(entry, currentRelay) }
  function isFirstInstance(entry, instance) { return !!entry && !!instance && entry.instances[0] === instance }
  // An agent is listed (and managed) in every community it has an instance in.
  function inCurrentCommunity(entry) { return !!currentInstance(entry) }
  function communityNames(entry) { return entry ? entry.instances.map(function(instance) { return instance.community }).join(", ") : "" }
  // The current community's canonical relay as the agent service names it,
  // when the service and the helper agree on it; else "".
  readonly property string currentCanonicalRelay: activeRelay !== "" && relayKey(activeRelay) === relayKey(currentRelay) ? activeRelay : ""
  // The main helper is signed in to the current community: its rooms are verified there.
  readonly property bool currentAuthenticated: !!mainService && !mainService.sampleMode && !mainService.sessionFailed
    && mainService.connection === "authenticated" && currentRelay !== "" && relayKey(mainService.relay) === relayKey(currentRelay)
  // "Add to <current community>": enrolled, not there yet, helper signed in there, room for another instance.
  function canAddToCurrent(entry) {
    return !!entry && entry.enrolled && !currentInstance(entry) && currentAuthenticated && currentCanonicalRelay !== ""
      && entry.instances.length < maxInstances
  }
  readonly property var currentAgents: agents.filter(function(entry) { return root.inCurrentCommunity(entry) })
  readonly property var otherAgents: agents.filter(function(entry) { return !root.inCurrentCommunity(entry) })
  // The current community's local name: the helper's list, else an agent of it, else its host.
  readonly property string currentCommunityName: {
    if (currentRelay === "") return ""
    var listed = mainService && mainService.communityEntries ? mainService.communityEntries.find(function(entry) {
      return root.relayKey(entry.relay) === root.relayKey(root.currentRelay) }) : null
    if (listed && listed.name) return listed.name
    var member = currentAgents.length ? currentAgents[0] : null
    return member ? member.community : currentRelay.replace(/^wss?:\/\//, "").replace(/\/$/, "")
  }
  // The agent's state here: its instance in the current community (`instance`
  // when given), else its first.
  function statusWord(entry, instance) {
    if (!entry) return ""
    if (!entry.enrolled) return "not enrolled"
    var shown = instance || currentInstance(entry)
    return ({active: "running", inactive: "stopped", failed: "failed"})[shown ? shown.state : entry.unit] || "unknown"
  }
  function categorySentence(category) {
    return ({
      agent_invalid: "The agent service refused these settings.",
      agent_busy: "Another agent change is in progress. Try again shortly.",
      agent_limit: "The limit of 16 agents is reached.",
      harness_missing: "This harness is not installed on this machine.",
      bundle_stale: "The harness bundle needs a refresh.",
      not_signed_in: "Sign in to this harness first.",
      enroll_failed: "Enrollment did not complete.",
      unit_failed: "The agent's service unit failed.",
      workspace_refused: "The workspace was refused. Choose another directory.",
      relay_unavailable: "The relay could not be reached.",
      request_unknown: "No answer from the agent service. Check the agent's state before retrying."
    })[category] || "The request failed."
  }
  function detailSentence(detail) {
    return ({model_not_for_harness: "The model is not one of this harness's models."})[detail] || ""
  }
  function requestLabel(type) {
    return ({create_agent: "Creating agent", update_agent: "Saving agent", delete_agent: "Deleting agent",
      enroll_agent: "Enrolling agent", start_agent: "Starting agent", stop_agent: "Stopping agent",
      set_start_at_login: "Changing start at login", sign_in: "Opening sign-in",
      refresh_bundle: "Refreshing harness bundle", probe_model: "Testing model",
      enroll_agent_in: "Adding agent to community", leave_agent_community: "Leaving community"})[type] || "Agent request"
  }
  // One line for the editor: this panel's request first, then the service's own.
  readonly property string statusLabel: {
    if (requestState === "working") return requestLabel(requestType) + "…"
    if (requestState === "done") return requestLabel(requestType) + " · done"
    if (requestState === "failed" || requestState === "unknown")
      return requestLabel(requestType) + " · failed. " + categorySentence(requestCategory)
        + (requestDetail ? " " + detailSentence(requestDetail) : "")
    if (serviceWorking) return requestLabel(pending.type) + " elsewhere…"
    return ""
  }

  // Room choices: the main helper's verified joined rooms (not direct messages),
  // which are the current community's. An agent of another community has none here.
  readonly property var roomChoices: mainService && !mainService.sampleMode && !mainService.sessionFailed
    && mainService.connection === "authenticated" && ["partial", "ready"].indexOf(mainService.catalogState) !== -1
    ? mainService.streamRooms.map(function(room) { return {id: room.id, name: room.name} }) : []
  function roomChoicesFor(entry) { return !entry || inCurrentCommunity(entry) ? roomChoices : [] }

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
  // Empty is the harness default; otherwise an alias or an id of that harness.
  function harnessModelProblem(value, harnessId) {
    if (modelProblem(value) || value === "" || !modelAliases.hasOwnProperty(harnessId)) return modelProblem(value)
    if (modelAliases[harnessId].indexOf(value) !== -1 || modelPatterns[harnessId].test(value)) return ""
    return harnessId === "claude-code" ? "Claude Code models are opus, sonnet, haiku, fable or a claude-… id."
      : "Codex models are gpt-…, o… (for example o3) or codex-… ids."
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
  // Complete persona fields as sent on create, or the changed subset on update
  // (`current`: the saved agent, for a model or harness change alone).
  function fieldsProblem(fields, creating, current) {
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
      fields.hasOwnProperty("startAtLogin") && typeof fields.startAtLogin !== "boolean" ? "Invalid start at login." : "",
      fields.hasOwnProperty("answersDms") && typeof fields.answersDms !== "boolean" ? "Invalid direct message choice." : ""
    ]
    var problem = checks.find(function(text) { return text !== "" }) || ""
    // A model saved before these patterns existed is kept until model or harness change.
    if (!problem && (fields.hasOwnProperty("model") || fields.hasOwnProperty("harness"))) {
      var base = current || {}
      problem = harnessModelProblem(fields.hasOwnProperty("model") ? fields.model : base.model,
        fields.hasOwnProperty("harness") ? fields.harness : base.harness)
    }
    return problem
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
          || ["ready", "stale", "missing"].indexOf(entry.bundle) === -1
          || (entry.signedIn !== null && typeof entry.signedIn !== "boolean")) return null
      result.push({id: entry.id, bundle: entry.bundle, signedIn: entry.signedIn})
    }
    return result
  }
  // A canonical relay as the service reports it: ws(s)://host[:port]/ only.
  function relayValue(value) {
    return typeof value === "string" && value.length <= 2048 && /^wss?:\/\/[^\/\s?#@]+\/$/.test(value) && !unsafeText(value)
  }
  function communityValue(value) {
    return typeof value === "string" && value.length >= 1 && value.length <= 2048 && !!value.trim() && !unsafeText(value)
  }
  readonly property var unitStates: ["active", "inactive", "failed", "unknown"]
  // One instance: its own unit name (`omarchy-buzz-agent-<id>[-<12 hex>].service`).
  function validatedInstance(value, id) {
    if (!exactKeys(value, "community,lastError,published,relay,rooms,startAtLogin,state,unit") || !relayValue(value.relay)
        || !communityValue(value.community) || !roomsShapeValid(value.rooms) || typeof value.unit !== "string"
        || !/^omarchy-buzz-agent-[a-f0-9-]{36}(-[a-f0-9]{12})?\.service$/.test(value.unit) || value.unit.slice(19, 55) !== id
        || typeof value.startAtLogin !== "boolean" || typeof value.published !== "boolean"
        || (value.lastError !== null && errorCategories.indexOf(value.lastError) === -1)
        || unitStates.indexOf(value.state) === -1) return null
    return {relay: value.relay, community: value.community, rooms: value.rooms.slice(), unit: value.unit,
      startAtLogin: value.startAtLogin, published: value.published, lastError: value.lastError, state: value.state}
  }
  function validatedInstances(value, entry) {
    if (!Array.isArray(value) || value.length < 1 || value.length > maxInstances) return null
    var result = []
    for (var i = 0; i < value.length; i++) {
      var instance = validatedInstance(value[i], entry.id)
      if (!instance || result.some(function(other) { return root.relayKey(other.relay) === root.relayKey(instance.relay) || other.unit === instance.unit }))
        return null
      result.push(instance)
    }
    // The top level repeats the first instance; an agent without an identity has only one.
    var first = result[0]
    if (first.relay !== entry.relay || first.community !== entry.community || JSON.stringify(first.rooms) !== JSON.stringify(entry.rooms)
        || first.startAtLogin !== entry.startAtLogin || first.published !== entry.published || first.lastError !== entry.lastError
        || first.state !== entry.unit || (!entry.enrolled && result.length !== 1)) return null
    return result
  }
  function validatedAgent(entry) {
    if (!exactKeys(entry, "acpCommand,answersDms,community,description,enrolled,harness,id,identity,instances,instructions,lastError,model,name,published,relay,respondTo,rooms,startAtLogin,unit,workspace")
        || !relayValue(entry.relay) || !communityValue(entry.community)
        || !uuidV4(entry.id) || nameProblem(entry.name) || descriptionProblem(entry.description)
        || instructionsProblem(entry.instructions) || harnessIds.indexOf(entry.harness) === -1
        || modelProblem(entry.model) || entry.acpCommand !== "buzz-acp" || !roomsShapeValid(entry.rooms)
        || ["owner-only", "mentions"].indexOf(entry.respondTo) === -1 || workspaceProblem(entry.workspace, false)
        || (entry.identity !== null && (typeof entry.identity !== "string" || !/^[a-f0-9]{64}$/.test(entry.identity)))
        || typeof entry.enrolled !== "boolean" || (entry.enrolled && entry.identity === null)
        || ["active", "inactive", "failed", "unknown"].indexOf(entry.unit) === -1
        || typeof entry.startAtLogin !== "boolean" || typeof entry.published !== "boolean" || typeof entry.answersDms !== "boolean"
        || (entry.lastError !== null && errorCategories.indexOf(entry.lastError) === -1)) return null
    var instances = validatedInstances(entry.instances, entry)
    if (!instances) return null
    return {id: entry.id, name: entry.name, description: entry.description, instructions: entry.instructions,
      harness: entry.harness, model: entry.model, acpCommand: entry.acpCommand, rooms: entry.rooms.slice(),
      respondTo: entry.respondTo, workspace: entry.workspace, identity: entry.identity, enrolled: entry.enrolled,
      unit: entry.unit, startAtLogin: entry.startAtLogin, answersDms: entry.answersDms, published: entry.published, lastError: entry.lastError,
      relay: entry.relay, community: entry.community, instances: instances}
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
    if (!exactKeys(value, "category,detail,requestId,state,type") || !uuidValue(value.requestId)
        || mutatingTypes.indexOf(value.type) === -1 || ["working", "done", "failed"].indexOf(value.state) === -1
        || (value.state === "failed") !== (value.category !== null)
        || (value.category !== null && errorCategories.indexOf(value.category) === -1)
        || (value.detail !== null && (value.state !== "failed" || pendingDetails.indexOf(value.detail) === -1))) return null
    return {value: {requestId: value.requestId, type: value.type, state: value.state, category: value.category, detail: value.detail}}
  }
  function validatedModelProbe(value) {
    if (!exactKeys(value, "agentId,detail,model,state") || probeStates.indexOf(value.state) === -1
        || typeof value.model !== "string" || modelProblem(value.model)) return null
    if (value.state === "idle") {
      if (value.agentId !== null || value.model !== "" || value.detail !== null) return null
    } else if (!uuidV4(value.agentId) || value.model === ""
        || (value.state === "running" ? value.detail !== null : probeSentences.indexOf(value.detail) === -1)) return null
    return {agentId: value.agentId, state: value.state, model: value.model, detail: value.detail}
  }
  function validatedStatus(value) {
    if (!exactKeys(value, "activeRelay,agents,harnesses,modelProbe,pending")
        || (value.activeRelay !== null && !relayValue(value.activeRelay))) return null
    var harnessList = validatedHarnesses(value.harnesses)
    var agentList = validatedAgents(value.agents)
    var pendingView = validatedPending(value.pending)
    var probe = validatedModelProbe(value.modelProbe)
    if (!harnessList || !agentList || !pendingView || !probe) return null
    return {activeRelay: value.activeRelay || "", harnesses: harnessList, agents: agentList, pending: pendingView.value, modelProbe: probe}
  }

  function clearData() {
    capabilitySupported = false
    if (harnesses.length) harnesses = []
    if (agents.length) agents = []
    activeRelay = ""
    pending = null
    modelProbe = {agentId: null, state: "idle", model: "", detail: null}
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
        requestDetail = ""
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
    if (activeRelay !== status.activeRelay) activeRelay = status.activeRelay
    if (!sameProjection(pending, status.pending)) pending = status.pending
    if (!sameProjection(modelProbe, status.modelProbe)) modelProbe = status.modelProbe
    var view = status.pending
    if (view && view.requestId === requestId && requestState === "working" && view.state !== "working") {
      requestTimeout.stop()
      requestState = view.state
      requestCategory = view.category || ""
      requestDetail = view.detail || ""
      if (view.state === "done" && requestType === "create_agent") {
        var known = requestKnownIds
        var added = agents.filter(function(entry) { return known.indexOf(entry.id) === -1 })
        if (added.length === 1) agentCreated(added[0].id)
      }
      if (view.state === "done" && ["enroll_agent", "enroll_agent_in", "leave_agent_community", "delete_agent", "update_agent"].indexOf(requestType) !== -1)
        membershipChanged()
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
    requestDetail = ""
    requestId = write(request)
    // A model probe may take the service's full 90 s bound.
    requestTimeout.interval = request.type === "probe_model" ? 120000 : 60000
    requestTimeout.restart()
    return true
  }
  function copyFields(fields) {
    var copy = {}
    Object.keys(fields).forEach(function(key) { copy[key] = key === "rooms" ? fields.rooms.slice() : fields[key] })
    return copy
  }
  function createAgent(fields) {
    // A new agent joins the current community, which the service must also see as active.
    if (agents.length >= 16 || currentRelay === "" || fieldsProblem(fields, true)) return false
    var copy = copyFields(fields)
    if (!copy.hasOwnProperty("startAtLogin")) copy.startAtLogin = false
    copy.acpCommand = "buzz-acp"
    return mutate({type: "create_agent", fields: copy}, "")
  }
  // Only agents with an instance in the current community are managed here;
  // the others are shown read-only until one of their communities is active
  // again (or until they are added to this one).
  function manageable(id) { return inCurrentCommunity(agent(id)) }
  // The saved fields as the current community's instance has them (its rooms).
  function savedFields(entry) {
    var instance = currentInstance(entry)
    return entry && instance ? Object.assign({}, entry, {rooms: instance.rooms}) : entry
  }
  // Definition fields apply to every community; rooms and the workspace to the
  // current one, named by `relay`. Another community's workspace is not edited here.
  function updateAgent(id, fields) {
    var entry = agent(id), instance = currentInstance(entry)
    if (!manageable(id) || fieldsProblem(fields, false, savedFields(entry))) return false
    if (fields.hasOwnProperty("workspace") && !isFirstInstance(entry, instance)) return false
    return mutate({type: "update_agent", agentId: id, fields: copyFields(fields), relay: instance.relay}, id)
  }
  function deleteAgent(id, forget) {
    if (!manageable(id) || typeof forget !== "boolean") return false
    return mutate({type: "delete_agent", agentId: id, forget: forget}, id)
  }
  // The instance a request acts on: the one in `relay`, else the current community's.
  function targetInstance(entry, relay) { return typeof relay === "string" && relay !== "" ? instanceIn(entry, relay) : currentInstance(entry) }
  // Creates the identity (an agent never enrolled), or retries publication of
  // the current community's instance after a failure.
  function enrollAgent(id) {
    var entry = agent(id), instance = currentInstance(entry)
    if (!manageable(id) || (entry.enrolled && instance.published)) return false
    return mutate({type: "enroll_agent", agentId: id, relay: instance.relay}, id)
  }
  function startAgent(id, relay) {
    var entry = agent(id), instance = targetInstance(entry, relay)
    if (!manageable(id) || !instance || !entry.enrolled || !instance.published || instance.state === "active" || bundleStale(entry.harness)) return false
    return mutate({type: "start_agent", agentId: id, relay: instance.relay}, id)
  }
  function stopAgent(id, relay) {
    var entry = agent(id), instance = targetInstance(entry, relay)
    if (!manageable(id) || !instance || instance.state !== "active") return false
    return mutate({type: "stop_agent", agentId: id, relay: instance.relay}, id)
  }
  function setStartAtLogin(id, enabled, relay) {
    var entry = agent(id), instance = targetInstance(entry, relay)
    if (!manageable(id) || !instance || typeof enabled !== "boolean" || instance.startAtLogin === enabled) return false
    return mutate({type: "set_start_at_login", agentId: id, enabled: enabled, relay: instance.relay}, id)
  }
  // The same agent (identity, instructions) in the current community, with 1–8 of its verified rooms.
  function enrollAgentIn(id, rooms) {
    var entry = agent(id)
    if (!canAddToCurrent(entry) || roomsProblem(rooms)) return false
    return mutate({type: "enroll_agent_in", agentId: id, relay: currentCanonicalRelay, rooms: rooms.slice()}, id)
  }
  // Leaves one community (its rooms, unit and instance); never the last one.
  function leaveCommunity(id, relay) {
    var entry = agent(id), instance = instanceIn(entry, relay)
    if (!manageable(id) || !instance || entry.instances.length < 2) return false
    return mutate({type: "leave_agent_community", agentId: id, relay: instance.relay}, id)
  }
  function signIn(harnessId) {
    var entry = harness(harnessId)
    if (!entry || entry.signedIn === true) return false
    return mutate({type: "sign_in", harness: harnessId}, "")
  }
  // The bundle's launcher differs from the installed scripts: refresh before starting.
  function bundleStale(harnessId) {
    var entry = harness(harnessId)
    return !!entry && entry.bundle === "stale"
  }
  function refreshBundle(harnessId) {
    if (!bundleStale(harnessId)) return false
    return mutate({type: "refresh_bundle", harness: harnessId}, "")
  }
  // One real model turn in the agent's sandbox: the saved model, a signed-in harness, a ready bundle.
  function canProbeModel(id) {
    var entry = agent(id)
    var state = entry ? harness(entry.harness) : null
    return manageable(id) && entry.model !== "" && !harnessModelProblem(entry.model, entry.harness)
      && !!state && state.bundle === "ready" && state.signedIn === true
  }
  function probeModel(id) {
    if (!canProbeModel(id)) return false
    return mutate({type: "probe_model", agentId: id}, id)
  }
  // The service's last probe of this agent, or null.
  function probeFor(id) { return modelProbe.agentId === id ? modelProbe : null }
  function dismissRequest() {
    if (requestState === "working") return false
    requestState = "idle"
    requestCategory = ""
    requestDetail = ""
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
