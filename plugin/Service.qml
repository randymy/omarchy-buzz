import QtQuick
import Quickshell
import Quickshell.Io
import "SampleData.js" as SampleData
import "ActivityObserver.js" as ActivityObserver
import "RoomActivity.js" as RoomActivity

Item {
  id: root
  property var shell: null
  property var manifest: null
  // Only offscreen fixtures opt into sampleMode; production never loads sample rooms.
  property bool sampleMode: false
  property bool autoConnect: true
  property string setupProvider: "hosted"
  // Opt-in across shell restarts. Only this boolean is stored; alert payloads stay generic.
  property bool notificationsEnabled: false
  readonly property string stateHome: Quickshell.env("XDG_STATE_HOME").startsWith("/")
    ? Quickshell.env("XDG_STATE_HOME") : Quickshell.env("HOME") + "/.local/state"
  readonly property string notificationSettingsDir: stateHome + "/omarchy-buzz"
  readonly property string notificationSettingsPath: notificationSettingsDir + "/notifications.json"
  property bool notificationSettingsLoaded: false
  property bool notificationSettingsDirReady: false
  property bool notificationPreferenceDirty: false
  property bool hydratingNotificationPreference: false
  property bool panelOpen: false
  property var roomActivity: RoomActivity.fresh()
  property bool roomActivitySupported: false
  readonly property bool activityVisible: !sessionFailed && connection === "authenticated" && ["partial", "ready"].indexOf(catalogState) !== -1
  readonly property int observedActivityCount: activityVisible ? RoomActivity.total(roomActivity) : 0
  readonly property string observedActivityLabel: activityVisible ? RoomActivity.totalLabel(roomActivity) : "0"
  function roomActivityCount(room) { return activityVisible ? Math.min(999, RoomActivity.count(roomActivity, room)) : 0 }
  onPanelOpenChanged: if (panelOpen && historyState === "snapshot") roomActivity = RoomActivity.markSeen(roomActivity, selectedRoomId)
  function notifyActivity() {
    if (notificationsEnabled && !sampleMode && !notificationProcess.running && !notificationCooldown.running) {
      notificationProcess.running = true
      notificationCooldown.start()
    }
  }
  property var activityObservation: ActivityObserver.fresh()
  onNotificationsEnabledChanged: {
    activityObservation = ActivityObserver.fresh()
    if (hydratingNotificationPreference) return
    notificationPreferenceDirty = true
    if (notificationSettingsLoaded && notificationSettingsDirReady) notificationSettingsSave.restart()
  }

  function loadNotificationSettings(raw) {
    if (notificationSettingsLoaded) return
    var enabled = false
    try {
      var parsed = JSON.parse(raw)
      if (parsed && parsed.version === 1 && parsed.enabled === true) enabled = true
    } catch (error) { /* Missing or malformed settings fail closed. */ }
    if (!notificationPreferenceDirty) {
      hydratingNotificationPreference = true
      notificationsEnabled = enabled
      hydratingNotificationPreference = false
    }
    notificationSettingsLoaded = true
    if (notificationPreferenceDirty && notificationSettingsDirReady) notificationSettingsSave.restart()
  }

  FileView {
    id: notificationSettingsFile
    path: root.notificationSettingsPath
    watchChanges: false
    atomicWrites: true
    printErrors: false
    onLoaded: root.loadNotificationSettings(text())
    onLoadFailed: root.loadNotificationSettings("")
  }
  Process {
    id: notificationSettingsDirProcess
    command: ["mkdir", "-m", "700", "-p", root.notificationSettingsDir]
    onExited: function(exitCode) {
      if (exitCode !== 0) return
      root.notificationSettingsDirReady = true
      notificationSettingsFile.reload()
      if (root.notificationSettingsLoaded && root.notificationPreferenceDirty) notificationSettingsSave.restart()
    }
  }
  Timer {
    id: notificationSettingsSave
    interval: 200
    onTriggered: {
      if (!root.notificationSettingsLoaded || !root.notificationSettingsDirReady) return
      notificationSettingsFile.setText(JSON.stringify({version: 1, enabled: root.notificationsEnabled}) + "\n")
      root.notificationPreferenceDirty = false
    }
  }
  property string helperExecutable: Quickshell.env("HOME") + "/.local/bin/omarchy-buzz"
  property string connection: "unavailable"
  property string category: "helper_unavailable"
  property string instanceId: ""
  property string relay: ""
  property bool sessionFailed: true
  property int generation: 0
  property int requestSequence: 0
  property string pendingHistoryRequestId: ""
  property bool sendSupported: false
  property var drafts: ({})
  property string draftScopeKey: ""
  property bool threadSendSupported: false
  property var replyTargets: ({})
  readonly property string replyRootId: replyTargets[selectedRoomId] || ""
  readonly property string composerKey: selectedRoomId + (replyRootId ? ":" + replyRootId : "")
  readonly property string draftText: drafts[composerKey] || ""
  readonly property bool replyReady: !replyRootId || canReplyTo(replyRootId)
  readonly property string composerLabel: replyRootId ? (replyReady ? "Replying in this thread" : "Reply target unavailable · reopen its replies or return to the room") : "Message this room"
  property string deliveryState: "idle"
  property string deliveryCategory: ""
  property string submissionId: ""
  property string submissionRoom: ""
  property string submissionRoot: ""
  property string submissionDraftKey: ""
  property string submissionText: ""
  property int submissionGeneration: 0
  property string submissionInstance: ""
  property string acknowledgedRefreshId: ""
  property bool recipientsSupported: false
  property string recipientsState: "unavailable"
  property string recipientsRoomId: ""
  property string recipientsCategory: ""
  property var recipientEntries: []
  property var agentProfiles: []
  property var recipientDrafts: ({})
  readonly property var selectedRecipients: recipientDrafts[composerKey] || []
  readonly property var unavailableRecipients: selectedRecipients.filter(function(key) {
    return recipientsState !== "snapshot" || !recipientEntries.some(function(entry) { return entry.key === key })
  })
  readonly property bool recipientIntentValid: selectedRecipients.length === 0 || (recipientsSupported && recipientsState === "snapshot" && unavailableRecipients.length === 0)
  property bool recipientsPartial: false
  property string pendingRecipientsRequestId: ""
  property var submissionMentions: []
  readonly property bool recipientPickerLocked: deliveryState === "sending" || deliveryState === "unknown" || deliveryState === "rejected" || deliveryCategory === "send_request_reused"
  readonly property string recipientsLabel: recipientsState === "loading" ? "Loading recipients" : recipientsState === "snapshot" ? (recipientsPartial ? "Partial recipient list · " : "Room recipients · ") + recipientEntries.length : "Recipients unavailable"

  readonly property bool canSend: sendSupported && !sampleMode && !sessionFailed && connection === "authenticated"
    && selectedRoom !== null && deliveryState !== "sending" && deliveryState !== "unknown" && deliveryState !== "rejected" && deliveryCategory !== "send_request_reused"
    && replyReady && recipientIntentValid && draftText.trim().length > 0 && draftText.indexOf("\u0000") === -1 && utf8Size(draftText) <= 4096
  readonly property bool deliveryScopeMismatch: submissionId !== "" && submissionDraftKey !== composerKey
  readonly property string submissionScopeLabel: {
    var room = rooms.find(function(entry) { return entry.id === submissionRoom })
    var label = "#" + (room ? room.name : submissionRoom)
    return submissionRoot ? label + " · thread " + submissionRoot.slice(0, 8) + "…" : label
  }
  readonly property string deliveryBaseLabel: deliveryCategory === "send_request_reused" ? "Submission ID cannot be reused. Start a new submission explicitly." : deliveryCategory === "send_ledger_unavailable" ? "Local send ledger unavailable. Check state directory permissions and free space, then retry." : ({idle:"",sending:"Sending…",acknowledged:"Acknowledged by relay",rejected:"Message rejected. Start a new submission explicitly to retry; this receipt will not send again.",failed:"Send failed · draft retained",unknown:"Outcome unknown. Sending again may create a duplicate. Discard the uncertain draft explicitly to continue."})[deliveryState] || ""
  readonly property string deliveryLabel: deliveryScopeMismatch && deliveryBaseLabel ? submissionScopeLabel + ": " + deliveryBaseLabel : deliveryBaseLabel

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
  property bool automaticHistorySupported: false
  property var historyHasMore: null
  property bool threadSupported: false
  property string threadRootId: ""
  property string threadState: "unavailable"
  property string threadCategory: ""
  property var threadRows: []
  property var threadHasMore: null
  property string pendingThreadRequestId: ""
  readonly property string threadLabel: threadState === "loading" ? "Loading replies" : threadState === "snapshot"
    ? (threadRows.length ? "Replies" : "No replies in this snapshot") + (threadHasMore ? " · older replies available" : "")
    : threadCategory === "thread_access_denied" ? "Replies unavailable for this room" : "Replies unavailable · try Refresh replies"
  readonly property var messages: sample ? sample.messages.filter(function(message) { return message.roomId === root.selectedRoomId }) : historyRows
  readonly property string historyLabel: historyState === "loading" ? "Loading recent snapshot" : historyState === "snapshot"
    ? (automaticHistorySupported ? "Auto-refreshing snapshot" : "Snapshot") + " · completeness unknown" + (historyHasMore ? " · older history available" : "") : ({request_busy: "Helper busy · refresh again", history_timeout: "History request timed out", history_invalid: "History response could not be validated", history_access_denied: "History unavailable for this room"})[historyCategory] || "History not available yet"
  readonly property string barLabel: sampleMode ? "TEST" : ({unconfigured: "Setup", connecting: "Connecting", authenticated: "Connected", identity_locked: "Locked", disconnected: "Offline", unavailable: "Error"})[connection] || "Error"
  readonly property string barSymbol: sampleMode ? "T" : ({unconfigured: "?", connecting: "…", authenticated: "✓", identity_locked: "!", disconnected: "○", unavailable: "!"})[connection] || "!"
  readonly property string statusLabel: sampleMode ? "Sample data" : category === "incompatible_response" ? "Incompatible helper" : category === "identity_access_pending" ? "Waiting for secret store unlock" : ({
    unconfigured: "Setup required", connecting: "Connecting", authenticated: historyState === "snapshot" ? "Authenticated · recent snapshot" : historyState === "loading" ? "Authenticated · history loading" : "Authenticated · history unavailable",
    identity_locked: "Identity locked", disconnected: "Disconnected", unavailable: "Helper unavailable"
  })[connection] || "Unavailable"
  readonly property string providerInstructions: setupProvider === "hosted"
    ? "Set up your account and identity binding at buzz.xyz. Create or join a community, then use its assigned URL. There is no single public global relay; invitations and membership still apply."
    : "Use the URL of a relay you already belong to, including one you were invited to. Choosing custom does not require running your own relay."
  readonly property string setupInstructions: category === "incompatible_response"
    ? "Install a matching Buzz plugin and helper release, restart the helper service, then Retry. Updating the Omarchy plugin alone does not replace its helper. Your existing identity stays in the secret store."
    : (category === "config_unavailable" || category === "invalid_config")
    ? "Check your local helper configuration, then Retry. Credentials do not belong in that file."
    : (connection === "identity_locked" || category === "identity_access_pending")
      ? "Unlock your OS secret store, then Retry. Your existing identity is retained."
      : connection === "authenticated"
        ? "Relay authentication succeeded. Choose a joined room to fetch a recent snapshot. Use the composer to send plain text. Select exact room recipients when available; agent execution is configured separately."
        : providerInstructions + "\nLink manually in a terminal after installing the helper:\nomarchy-buzz setup relay <community-url>\nomarchy-buzz setup identity enroll\nEnroll the same existing Buzz identity using hidden input and your OS secret store, then Retry. Hosted account sign-in stays in your browser. Never enter keys or account tokens in this panel."

  function chooseSetupProvider(provider) {
    // Presentation only: choosing a provider never writes config or sends IPC.
    if (provider === "hosted" || provider === "custom") setupProvider = provider
  }

  function selectRoom(roomId) {
    if (rooms.some(function(room) { return room.id === roomId })) {
      if (selectedRoomId !== roomId) { clearHistory(); clearRecipients() }
      selectedRoomId = roomId
      if (!sampleMode) { refreshHistory(); refreshRecipients() }
    }
  }
  function uuidValue(value) { return typeof value === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(value) }
  function utf8Size(value) {
    try { return encodeURIComponent(value).replace(/%[A-F0-9]{2}/gi, "x").length }
    catch (_) { return Infinity }
  }
  function updateDraft(text) {
    if (!selectedRoom || typeof text !== "string" || text.length > 4096 || text === draftText) return
    var copy = Object.assign({}, drafts)
    copy[composerKey] = text
    drafts = copy
    if (deliveryState !== "sending" && deliveryState !== "unknown") {
      deliveryState = "idle"
      deliveryCategory = ""
      submissionId = ""
    }
  }
  function newDraft(preserveText) {
    if (deliveryState === "sending") return
    var copy = Object.assign({}, drafts)
    var key = submissionDraftKey || composerKey
    copy[key] = preserveText === true ? (drafts[key] || "") : ""
    drafts = copy
    deliveryState = "idle"
    deliveryCategory = ""
    submissionId = ""
    submissionText = ""
  }
  function toggleRecipient(key) {
    if (recipientPickerLocked) return
    var copy = selectedRecipients.slice()
    var index = copy.indexOf(key)
    if (index >= 0) copy.splice(index, 1)
    else if (recipientsState === "snapshot" && recipientEntries.some(function(entry) { return entry.key === key }) && copy.length < 20) copy.push(key)
    else return
    var intents = Object.assign({}, recipientDrafts)
    intents[composerKey] = copy
    recipientDrafts = intents
    submissionId = ""
    deliveryState = "idle"
    deliveryCategory = ""
  }
  function correlationUuid() {
    // Randomness supplies correlation only; helper scope/identity are the authority.
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function(c) {
      var n = Math.floor(Math.random() * 16)
      return (c === "x" ? n : (n & 3) | 8).toString(16)
    })
  }
  function prepareSubmission() {
    if (!canSend) return null
    if (!submissionId || submissionRoom !== selectedRoomId || submissionRoot !== replyRootId || submissionText !== draftText || JSON.stringify(submissionMentions) !== JSON.stringify(selectedRecipients)) submissionId = correlationUuid()
    submissionRoom = selectedRoomId
    submissionRoot = replyRootId
    submissionDraftKey = composerKey
    submissionText = draftText
    submissionMentions = selectedRecipients.slice()
    submissionGeneration = generation
    submissionInstance = instanceId
    deliveryState = "sending"
    deliveryCategory = ""
    var request = {version:1,id:submissionId,type:"send_message",roomId:submissionRoom,text:submissionText,
      mentions:submissionMentions.slice(),generation:generation,instanceId:instanceId}
    if (submissionRoot) request.rootId = submissionRoot
    return request
  }
  function submitDraft() {
    if (!bridge.running) return false
    var request = prepareSubmission()
    if (!request) return false
    bridge.write(JSON.stringify(request) + "\n")
    deliveryTimeout.restart()
    return true
  }
  function losePendingDelivery() {
    deliveryTimeout.stop()
    if (deliveryState === "sending") { deliveryState = "unknown"; deliveryCategory = "delivery_unknown" }
  }
  function validatedDelivery(delivery) {
    var categories = ["delivery_unknown","send_rejected","send_unavailable","send_invalid","send_busy","send_access_denied","send_ledger_unavailable","send_request_reused","send_scope_changed"]
    if (!delivery || ["idle","sending","acknowledged","rejected","unknown","failed"].indexOf(delivery.state) === -1
        || (delivery.category !== null && categories.indexOf(delivery.category) === -1)
        || (delivery.eventId !== null && (typeof delivery.eventId !== "string" || !/^[a-f0-9]{64}$/.test(delivery.eventId)))) return null
    if (delivery.state === "idle") {
      if (delivery.requestId !== null || delivery.roomId !== null || delivery.eventId !== null) return null
    } else if (!uuidValue(delivery.requestId) || !uuidValue(delivery.roomId)
        || (delivery.state === "acknowledged" && delivery.eventId === null)) return null
    return delivery
  }
  function applyDelivery(delivery) {
    if (!delivery || !submissionId || delivery.requestId !== submissionId || delivery.roomId !== submissionRoom
        || generation !== submissionGeneration || instanceId !== submissionInstance) return
    // A late acceptance cannot silently resolve an already ambiguous local outcome.
    if (deliveryState !== "sending") return
    deliveryState = delivery.state
    deliveryCategory = delivery.category || ""
    if (deliveryState !== "sending") deliveryTimeout.stop()
    if (deliveryState === "acknowledged") {
      if (drafts[submissionDraftKey] === submissionText) {
        var copy = Object.assign({}, drafts)
        copy[submissionDraftKey] = ""
        drafts = copy
      }
      if (acknowledgedRefreshId !== submissionId) {
        acknowledgedRefreshId = submissionId
        if (selectedRoomId === submissionRoom) {
          if (!submissionRoot) refreshHistory()
          else if (threadRootId === submissionRoot) refreshThread()
        }
      }
    }
  }
  function canReplyTo(id) {
    return threadSendSupported && sendSupported && canOpenThread(id)
      && threadRootId === id && threadState === "snapshot"
  }
  function composeReply(id) {
    if (recipientPickerLocked || !canReplyTo(id)) return false
    var targets = Object.assign({}, replyTargets)
    targets[selectedRoomId] = id
    replyTargets = targets
    submissionId = ""
    deliveryState = "idle"
    deliveryCategory = ""
    return true
  }
  function composeRoom() {
    if (recipientPickerLocked) return false
    var targets = Object.assign({}, replyTargets)
    delete targets[selectedRoomId]
    replyTargets = targets
    submissionId = ""
    deliveryState = "idle"
    deliveryCategory = ""
    return true
  }
  function clearHistory() {
    clearThread()
    activityObservation = ActivityObserver.fresh()
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
  function clearThread() {
    threadRootId = ""
    threadState = "unavailable"
    threadCategory = ""
    threadRows = []
    threadHasMore = null
    pendingThreadRequestId = ""
  }
  function canOpenThread(id) {
    return !sampleMode && threadSupported && connection === "authenticated" && historyState === "snapshot"
      && historyRows.some(function(row) { return row.id === id && !row.unavailable })
  }
  function openThread(id) {
    if (!canOpenThread(id)) return
    clearThread()
    threadRootId = id
    threadState = "loading"
    send("fetch_thread", selectedRoomId, id)
  }
  function refreshThread() { if (threadRootId) openThread(threadRootId) }
  function closeThread() { clearThread(); if (threadSupported) send("close_thread") }
  function validatedThread(value) {
    var categories = ["thread_unavailable", "thread_timeout", "thread_invalid", "thread_access_denied", "thread_completeness_unknown"]
    if (!value || !Array.isArray(value.rows) || value.rows.length > 8 || (value.category !== null && categories.indexOf(value.category) === -1)
        || (value.rootId !== null && (typeof value.rootId !== "string" || !/^[a-f0-9]{64}$/.test(value.rootId)))
        || ((value.roomId === null) !== (value.rootId === null))
        || (value.state !== "unavailable" && value.rootId === null)) return null
    var checked = validatedHistory({state:value.state, roomId:value.roomId, rows:value.rows, hasMore:value.hasMore,
      category:value.category === null ? null : value.category.replace(/^thread_/, "history_")})
    if (!checked || checked.rows.some(function(row) { return row.id === value.rootId })) return null
    checked.rootId = value.rootId
    checked.category = value.category || ""
    return checked
  }
  function formatTimestamp(seconds) { return Qt.formatDateTime(new Date(seconds * 1000), "yyyy-MM-dd HH:mm:ss t") }
  function clearRecipients() {
    pendingRecipientsRequestId = ""
    recipientsRoomId = ""
    recipientEntries = []
    agentProfiles = []
    recipientsState = "unavailable"
    recipientsCategory = ""
    recipientsPartial = false
  }
  function refreshRecipients() {
    clearRecipients()
    if (sampleMode || !recipientsSupported || connection !== "authenticated" || !selectedRoom) return
    recipientsState = "loading"
    send("fetch_recipients", selectedRoomId)
  }
  function validatedRecipients(value) {
    if (!value || ["unavailable", "loading", "snapshot"].indexOf(value.state) === -1
        || (value.roomId !== null && !uuidValue(value.roomId)) || typeof value.partial !== "boolean"
        || !Array.isArray(value.entries) || value.entries.length > 20
        || (value.category !== null && ["recipients_unavailable", "recipients_timeout", "recipients_invalid", "recipients_access_denied"].indexOf(value.category) === -1)) return null
    if (value.state === "snapshot" && value.roomId === null || value.state !== "snapshot" && value.entries.length !== 0) return null
    var seen = ({})
    var entries = []
    for (var i = 0; i < value.entries.length; i++) {
      var entry = value.entries[i]
      if (!entry || typeof entry.key !== "string" || !/^[a-f0-9]{64}$/.test(entry.key) || seen[entry.key]
          || !boundedString(entry.name, 64) || utf8Size(entry.name) > 64 || /[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069\u200e\u200f\u061c]/.test(entry.name)) return null
      seen[entry.key] = true
      entries.push({key:entry.key,name:entry.name})
    }
    return {state:value.state,roomId:value.roomId,entries:entries,partial:value.partial,category:value.category || ""}
  }
  function validatedAgents(value, recipients) {
    if (!Array.isArray(value) || value.length > 10 || !recipients) return null
    var seen = ({})
    var result = []
    for (var i = 0; i < value.length; i++) {
      var entry = value[i]
      if (!entry || typeof entry.key !== "string" || seen[entry.key]
          || !recipients.entries.some(function(r) { return r.key === entry.key })
          || !boundedString(entry.name, 64) || utf8Size(entry.name) > 64
          || /[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069\u200e\u200f\u061c]/.test(entry.name)
          || typeof entry.profileEventId !== "string" || !/^[a-f0-9]{64}$/.test(entry.profileEventId)
          || entry.executionState !== "unknown") return null
      seen[entry.key] = true
      result.push({key:entry.key,name:entry.name,profileEventId:entry.profileEventId,executionState:"unknown"})
    }
    return result
  }
  function participantLabel(key) {
    var agent = agentProfiles.find(function(a) { return a.key === key })
    return agent ? "Self-described agent" : "Participant"
  }
  function messageAuthorName(key) {
    // Signed profile labels are presentation only; mentions still use exact keys.
    if (recipientsState === "snapshot" && recipientsRoomId === selectedRoomId && selectedRoomId !== "") {
      var recipient = recipientEntries.find(function(entry) { return entry.key === key })
      if (recipient && recipient.name.trim()) return recipient.name
    }
    return key.slice(0, 12) + "…"
  }
  function messageAuthorLabel(key) {
    var label = messageAuthorName(key)
    if (recipientsState === "snapshot" && recipientsRoomId === selectedRoomId
        && agentProfiles.some(function(agent) { return agent.key === key })) label += " · Self-described agent"
    return label
  }
  function clearCatalog() {
    roomActivity = RoomActivity.fresh()
    activityObservation = ActivityObserver.fresh()
    clearHistory()
    clearRecipients()
    catalogRooms = []
    catalogState = "unavailable"
    catalogCategory = ""
    if (!sampleMode) selectedRoomId = ""
  }
  function validCapabilities(capabilities) {
    return Array.isArray(capabilities) && capabilities.length >= 1 && capabilities.length <= 10
      && capabilities.indexOf("connection_status") !== -1
      && capabilities.every(function(cap, index) {
        return ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients", "history_auto_refresh", "room_activity", "agent_profiles", "thread_replies", "thread_send"].indexOf(cap) !== -1 && capabilities.indexOf(cap) === index
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
    losePendingDelivery()
    sendSupported = false
    clearCatalog()
    instanceId = ""
    relay = ""
    sessionFailed = false
    historySupported = false
    threadSupported = false
    threadSendSupported = false
    recipientsSupported = false
    generation = 0
    connection = "connecting"
    category = ""
  }
  function fail(reason) {
    losePendingDelivery()
    sendSupported = false
    handshake.stop()
    clearCatalog()
    sessionFailed = true
    historySupported = false
    threadSupported = false
    threadSendSupported = false
    recipientsSupported = false
    relay = ""
    connection = "unavailable"
    category = reason
    bridge.running = false
  }
  function boundedString(value, limit) { return typeof value === "string" && value.length <= limit }
  function acceptFrame(line) {
    if (sessionFailed) return false
    if (!boundedString(line, 98304)) { fail("invalid_response"); return false }
    var frame
    try { frame = JSON.parse(line) } catch (_) { fail("invalid_response"); return false }
    if (frame && frame.version === 1 && frame.type === "error" && ["request_busy", "send_busy", "send_scope_changed", "send_request_reused", "send_invalid", "send_unavailable", "send_access_denied", "send_ledger_unavailable", "delivery_unknown"].indexOf(frame.category) !== -1) {
      if (instanceId === "" || frame.instanceId !== instanceId) return false
      if (!boundedString(frame.id, 128) || !/^ui-[0-9]+$/.test(frame.id) && !uuidValue(frame.id)) { fail("invalid_response"); return false }
      if (frame.id === submissionId && deliveryState === "sending") {
        deliveryTimeout.stop()
        deliveryState = ["send_scope_changed", "delivery_unknown"].indexOf(frame.category) !== -1 ? "unknown" : "failed"
        deliveryCategory = frame.category === "request_busy" ? "send_busy" : frame.category
      }
      if (frame.id === pendingHistoryRequestId) {
        clearHistory()
        historyCategory = "request_busy"
      }
      if (frame.id === pendingThreadRequestId) {
        threadRows = []
        threadState = "unavailable"
        threadCategory = "thread_unavailable"
        pendingThreadRequestId = ""
      }
      if (frame.id === pendingRecipientsRequestId) {
        clearRecipients()
        recipientsCategory = "recipients_unavailable"
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
    var supportsThread = frame.capabilities.indexOf("thread_replies") !== -1
    var thread = supportsThread ? validatedThread(state.thread) : null
    if (supportsThread && (!supportsHistory || !thread)) { fail("invalid_response"); return false }
    var supportsRecipients = frame.capabilities.indexOf("room_recipients") !== -1
    var recipients = supportsRecipients ? validatedRecipients(state.recipients) : null
    if (supportsRecipients && !recipients) { fail("invalid_response"); return false }
    var agents = frame.capabilities.indexOf("agent_profiles") !== -1 ? validatedAgents(state.recipients && state.recipients.agents, recipients) : []
    if (agents === null) { fail("invalid_response"); return false }

    var supportsSend = frame.capabilities.indexOf("message_send") !== -1
    var delivery = supportsSend ? validatedDelivery(state.delivery) : null
    if (supportsSend && !delivery) { fail("invalid_response"); return false }
    var supportsActivity = frame.capabilities.indexOf("room_activity") !== -1
    if (supportsActivity && (!Array.isArray(state.activity) || state.activity.length > 20 || state.activity.some(function(a, i) {
      return !RoomActivity.valid(a) || !uuidValue(a.roomId) || !catalog.rooms.some(function(r) { return r.id === a.roomId })
        || state.activity.slice(0,i).some(function(b) { return b.roomId === a.roomId })
    }))) { fail("invalid_response"); return false }
    var incomingScope = (state.relay || "") + "|" + (state.identity || "")
    if (draftScopeKey && (incomingScope !== draftScopeKey || (instanceId !== "" && frame.generation !== generation))) {
      losePendingDelivery()
      clearThread()
      drafts = ({})
      recipientDrafts = ({})
      replyTargets = ({})
      submissionText = ""
    }
    draftScopeKey = incomingScope
    if (state.connection !== "authenticated") losePendingDelivery()
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
      clearRecipients()
      selectedRoomId = catalogRooms.length ? catalogRooms[0].id : ""
    }
    historySupported = supportsHistory
    if (state.connection !== "authenticated" || !supportsHistory || ["loading", "unavailable"].indexOf(catalogState) !== -1) clearHistory()
    else if (history && history.roomId === selectedRoomId && selectedRoomId !== "") {
      historyRows = history.rows
      historyState = history.state
      historyCategory = history.category
      historyHasMore = history.hasMore
      if (history.state === "snapshot" && typeof state.identity === "string") {
        var observed = ActivityObserver.observe(activityObservation,
          incomingScope + "|" + frame.instanceId + "|" + frame.generation + "|" + selectedRoomId,
          history.rows, state.identity, Math.floor(Date.now() / 1000))
        activityObservation = observed.state
        if (observed.notify && !supportsActivity && !panelOpen) notifyActivity()
      } else if (history.state === "unavailable") activityObservation = ActivityObserver.fresh()
    }
    threadSendSupported = supportsThread && frame.capabilities.indexOf("message_send") !== -1 && frame.capabilities.indexOf("thread_send") !== -1
    threadSupported = supportsThread
    if (!supportsThread || state.connection !== "authenticated" || historyState !== "snapshot"
        || !historyRows.some(function(row) { return row.id === root.threadRootId && !row.unavailable })) clearThread()
    else if (thread && thread.roomId === selectedRoomId && thread.rootId === threadRootId) {
      threadRows = thread.rows
      threadState = thread.state
      threadCategory = thread.category
      threadHasMore = thread.hasMore
    } else if (thread && thread.rootId === null && threadState !== "loading") clearThread()
    if (state.connection !== "authenticated" || !supportsRecipients || ["loading", "unavailable"].indexOf(catalogState) !== -1) clearRecipients()
    else if (recipients && recipients.roomId === selectedRoomId && selectedRoomId !== "") {
      recipientsRoomId = recipients.roomId
      recipientEntries = recipients.entries
      agentProfiles = agents
      recipientsState = recipients.state
      recipientsCategory = recipients.category
      recipientsPartial = recipients.partial
    }
    roomActivitySupported = supportsActivity
    if (!supportsActivity || state.connection !== "authenticated" || typeof state.identity !== "string" || catalogState === "unavailable") {
      roomActivity = RoomActivity.fresh()
    } else if (catalogState !== "loading") {
      var activityUpdate = RoomActivity.update(roomActivity,
        incomingScope + "|" + frame.instanceId + "|" + frame.generation, state.activity,
        panelOpen && historyState === "snapshot" ? selectedRoomId : "")
      roomActivity = activityUpdate.state
      if (activityUpdate.notify) notifyActivity()
    }
    recipientsSupported = supportsRecipients
    automaticHistorySupported = frame.capabilities.indexOf("history_auto_refresh") !== -1
    instanceId = frame.instanceId
    generation = frame.generation
    relay = state.relay || ""
    connection = state.connection
    category = state.category || ""
    sendSupported = supportsSend
    applyDelivery(delivery)
    handshake.stop()
    if (frame.type === "hello" && bridge.running) send("subscribe")
    // One fetch per first population/catalog refresh completion, never per status echo.
    if (supportsHistory && connection === "authenticated" && selectedRoom
        && (selectedBeforeCatalog === "" || previousCatalogState === "loading")
        && historyState !== "snapshot") refreshHistory()
    if (supportsRecipients && connection === "authenticated" && selectedRoom
        && (selectedBeforeCatalog === "" || previousCatalogState === "loading")
        && recipientsState !== "snapshot") refreshRecipients()
    return true
  }
  function send(kind, roomId, rootId) {
    if (!sessionFailed && bridge.running && instanceId !== "") {
      requestSequence++
      var request = { version: 1, id: "ui-" + requestSequence, type: kind }
      if (kind === "fetch_recent") { request.roomId = roomId; pendingHistoryRequestId = request.id }
      if (kind === "fetch_thread") { request.roomId = roomId; request.rootId = rootId; pendingThreadRequestId = request.id }
      if (kind === "fetch_recipients") { request.roomId = roomId; pendingRecipientsRequestId = request.id }
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
  Component.onCompleted: {
    notificationSettingsDirProcess.running = true
    if (autoConnect && !sampleMode) retry()
  }

  Timer {
    interval: 15000
    running: root.threadState === "loading"
    onTriggered: {
      root.threadRows = []
      root.threadState = "unavailable"
      root.threadCategory = "thread_timeout"
      root.pendingThreadRequestId = ""
    }
  }
  Timer {
    interval: 8000
    repeat: true
    running: root.panelOpen && root.threadState === "snapshot" && root.canOpenThread(root.threadRootId)
    onTriggered: root.refreshThread()
  }
  Timer {
    id: notificationCooldown
    interval: 10000
  }
  Process {
    id: notificationProcess
    // Fixed argv; no shell, relay content, credentials, or executable supplied by events.
    command: ["timeout", "5s", "omarchy", "notification", "send", "--app-name", "Buzz",
      "-u", "normal", "-t", "5000", "Buzz", "New activity in your Buzz rooms."]
    stdout: SplitParser { onRead: function(line) {} }
    stderr: SplitParser { onRead: function(line) {} }
  }
  Timer {
    id: handshake
    interval: 5000
    onTriggered: root.fail("handshake_timeout")
  }
  Timer {
    id: deliveryTimeout
    interval: 30000
    onTriggered: root.losePendingDelivery()
  }
  Process {
    id: bridge
    command: [root.helperExecutable, "ui-bridge"]
    stdinEnabled: true
    stdout: SplitParser { onRead: function(line) { root.acceptFrame(line) } }
    // Drain diagnostics without exposing raw helper output or secrets to logs/UI.
    stderr: SplitParser { onRead: function(line) {} }
    onExited: {
      root.losePendingDelivery()
      root.sendSupported = false
      handshake.stop()
      root.clearCatalog()
      root.historySupported = false
      root.threadSupported = false
      root.threadSendSupported = false
      root.recipientsSupported = false
      root.sessionFailed = true
      root.relay = ""
      root.connection = "unavailable"
      if (!root.category) root.category = "helper_unavailable"
    }
  }
}
