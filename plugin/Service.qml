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
  onPanelOpenChanged: {
    if (panelOpen && historyState === "snapshot") roomActivity = RoomActivity.markSeen(roomActivity, selectedRoomId)
    // Opening the panel or Retry reconnects a lost agent session; nothing polls.
    if (panelOpen && agentService.autoConnect) agentService.retry()
  }
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
  // The room last chosen by the user, restored after a shell restart. Only a
  // public room ID and its relay and identity scope are stored; no message data.
  readonly property string viewSettingsPath: notificationSettingsDir + "/view.json"
  property string rememberedScope: ""
  property string rememberedRoom: ""
  function loadViewSettings(raw) {
    try {
      var parsed = JSON.parse(raw)
      if (parsed && parsed.version === 1 && uuidValue(parsed.roomId) && boundedString(parsed.scope, 2200) && parsed.scope !== "") {
        rememberedScope = parsed.scope
        rememberedRoom = parsed.roomId
      }
    } catch (error) { /* Missing or malformed settings select the first room. */ }
  }
  function rememberRoom(roomId) {
    // Both a relay and an identity are required: an incomplete scope is never remembered.
    if (sampleMode || !/^[^|]+\|[^|]+$/.test(draftScopeKey) || !uuidValue(roomId)
        || (rememberedScope === draftScopeKey && rememberedRoom === roomId)) return
    rememberedScope = draftScopeKey
    rememberedRoom = roomId
    if (notificationSettingsDirReady) viewSettingsFile.setText(JSON.stringify({version: 1, scope: rememberedScope, roomId: rememberedRoom}) + "\n")
  }
  FileView {
    id: viewSettingsFile
    path: root.viewSettingsPath
    watchChanges: false
    atomicWrites: true
    printErrors: false
    blockLoading: true
    onLoaded: root.loadViewSettings(text())
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
  property int recipientsRetryBudget: 0
  property string recipientsRetryRoom: ""
  property string recipientsRetryInstance: ""
  property int recipientsRetryGeneration: 0
  property bool recipientsSupported: false
  property string recipientsState: "unavailable"
  property string recipientsRoomId: ""
  property string recipientsCategory: ""
  property var recipientEntries: []
  property var agentProfiles: []
  property var recipientDrafts: ({})
  property var inlineMentionDrafts: ({})
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
  // The room and the open thread each have a composer. Editing or submitting one
  // makes it the single active destination; drafts never move between them.
  readonly property string roomDraftText: drafts[selectedRoomId] || ""
  readonly property string threadDraftText: threadRootId ? drafts[selectedRoomId + ":" + threadRootId] || "" : ""
  function composeScope(rootId) {
    if (replyRootId === rootId) return true
    return rootId ? composeReply(rootId) : composeRoom()
  }
  function updateDraftFor(rootId, text) {
    if ((drafts[selectedRoomId + (rootId ? ":" + rootId : "")] || "") === text) return true
    if (!composeScope(rootId)) return false
    updateDraft(text)
    return draftText === text
  }
  function canSendFor(rootId) {
    if (replyRootId === rootId) return canSend
    var key = selectedRoomId + (rootId ? ":" + rootId : "")
    var text = drafts[key] || ""
    var chosen = recipientDrafts[key] || []
    return sendSupported && !sampleMode && !sessionFailed && connection === "authenticated" && selectedRoom !== null
      && !recipientPickerLocked && (!rootId || canReplyTo(rootId))
      && (chosen.length === 0 || (recipientsSupported && recipientsState === "snapshot"
        && chosen.every(function(key) { return recipientEntries.some(function(entry) { return entry.key === key }) })))
      && text.trim().length > 0 && text.indexOf("\u0000") === -1 && utf8Size(text) <= 4096
  }
  function submitFor(rootId) { return composeScope(rootId) && submitDraft() }
  readonly property bool deliveryScopeMismatch: submissionId !== "" && submissionDraftKey !== composerKey
  readonly property string submissionScopeLabel: {
    var room = rooms.find(function(entry) { return entry.id === submissionRoom })
    var label = room && room.kind === "dm" ? room.name : "#" + (room ? room.name : submissionRoom)
    return submissionRoot ? label + " · thread " + submissionRoot.slice(0, 8) + "…" : label
  }
  readonly property string deliveryBaseLabel: deliveryCategory === "send_request_reused" ? "Submission ID cannot be reused. Start a new submission explicitly." : deliveryCategory === "send_ledger_unavailable" ? "Local send ledger unavailable. Check state directory permissions and free space, then retry." : ({idle:"",sending:"Sending…",acknowledged:"Acknowledged by relay",rejected:"Message rejected. Start a new submission explicitly to retry; this receipt will not send again.",failed:"Send failed · draft retained",unknown:"Outcome unknown. Sending again may create a duplicate. Discard the uncertain draft explicitly to continue."})[deliveryState] || ""
  readonly property string deliveryLabel: deliveryScopeMismatch && deliveryBaseLabel ? submissionScopeLabel + ": " + deliveryBaseLabel : deliveryBaseLabel

  // Starting a DM (helper `dm_open`). People come only from the verified roster
  // on screen or from an existing DM's participants; the relay decides. The
  // viewer's public key is used only to leave the viewer out of the choice.
  property string identity: ""
  property bool dmOpenSupported: false
  property string dmOpenState: "idle"
  property string dmOpenCategory: ""
  property string dmOpenRequestId: ""
  property int dmOpenGeneration: 0
  property string dmOpenInstance: ""
  // The channel the relay named for the last acknowledged open, selected once listed.
  property string dmOpenTarget: ""
  property var dmSelection: []
  readonly property var dmCandidates: recipientsState === "snapshot" && recipientsRoomId === selectedRoomId && selectedRoomId !== ""
    ? recipientEntries.filter(function(entry) { return entry.key !== root.identity })
      .map(function(entry) { return {key: entry.key, name: entry.name, label: root.participantLabel(entry.key)} }) : []
  readonly property bool dmOpenAvailable: dmOpenSupported && !sampleMode && !sessionFailed && connection === "authenticated"
    && ["partial", "ready"].indexOf(catalogState) !== -1 && instanceId !== ""
  readonly property bool canStartDm: dmOpenAvailable && dmOpenState !== "sending" && validDmKeys(dmSelection)
  readonly property string dmOpenLabel: ({
    sending: "Starting conversation…",
    acknowledged: dmOpenCategory === "dm_open_response_unknown" ? "Relay accepted · conversation not identified, check Direct messages"
      : dmOpenTarget !== "" ? "Conversation started · waiting for it to be listed" : "",
    rejected: "Relay refused this conversation",
    unknown: "Outcome unknown · the conversation may exist, check Direct messages",
    failed: ({dm_open_busy: "Helper busy · try again", dm_open_access_denied: "Only people verified here can be chosen",
      dm_open_invalid: "Choose 1 to 8 other people", dm_open_scope_changed: "Connection changed · choose again"})[dmOpenCategory]
      || "Conversation not started · try again"
  })[dmOpenState] || ""
  function dmKeyAllowed(key) {
    if (typeof key !== "string" || !/^[a-f0-9]{64}$/.test(key) || key === identity) return false
    if (recipientsState === "snapshot" && recipientsRoomId === selectedRoomId && selectedRoomId !== ""
        && recipientEntries.some(function(entry) { return entry.key === key })) return true
    return rooms.some(function(room) { return room.kind === "dm" && room.participants.indexOf(key) !== -1 })
  }
  function validDmKeys(keys) {
    return Array.isArray(keys) && keys.length >= 1 && keys.length <= 8
      && keys.every(function(key, index) { return keys.indexOf(key) === index && root.dmKeyAllowed(key) })
  }
  function toggleDmParticipant(key) {
    if (!dmOpenAvailable || dmOpenState === "sending") return false
    var copy = dmSelection.slice()
    var index = copy.indexOf(key)
    if (index !== -1) copy.splice(index, 1)
    else if (copy.length < 8 && dmKeyAllowed(key)) copy.push(key)
    else return false
    dmSelection = copy
    if (dmOpenState !== "idle") { dmOpenState = "idle"; dmOpenCategory = "" }
    return true
  }
  function openDm(keys) {
    if (!dmOpenAvailable || !bridge.running || dmOpenState === "sending" || !validDmKeys(keys)) return false
    dmOpenRequestId = correlationUuid()
    dmOpenGeneration = generation
    dmOpenInstance = instanceId
    dmOpenState = "sending"
    dmOpenCategory = ""
    dmOpenTarget = ""
    bridge.write(JSON.stringify({version: 1, id: dmOpenRequestId, type: "open_dm", participants: keys.slice(),
      generation: generation, instanceId: instanceId}) + "\n")
    dmOpenTimeout.restart()
    return true
  }
  function startDm() { return openDm(dmSelection) }
  function loseDmOpen() {
    dmOpenTimeout.stop()
    dmOpenTarget = ""
    if (dmOpenState === "sending") { dmOpenState = "unknown"; dmOpenCategory = "dm_open_unknown" }
  }
  function validatedDmOpen(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)
        || Object.keys(value).sort().join(",") !== "category,channelId,created,requestId,state"
        || ["idle", "sending", "acknowledged", "rejected", "unknown"].indexOf(value.state) === -1
        || (value.category !== null && ["dm_open_rejected", "dm_open_unknown", "dm_open_response_unknown"].indexOf(value.category) === -1)
        || (value.channelId !== null && !uuidValue(value.channelId))
        || (value.created !== null && typeof value.created !== "boolean")) return null
    if (value.state === "idle") {
      if (value.requestId !== null || value.channelId !== null || value.created !== null || value.category !== null) return null
    } else if (!uuidValue(value.requestId)) return null
    else if (value.state === "acknowledged") {
      // A channel comes only with the relay's `created` flag; without one the answer named no channel.
      if ((value.channelId === null) !== (value.created === null)
          || value.category !== (value.channelId === null ? "dm_open_response_unknown" : null)) return null
    } else if (value.channelId !== null || value.created !== null
        || value.category !== ({sending: null, rejected: "dm_open_rejected", unknown: "dm_open_unknown"})[value.state]) return null
    return {state: value.state, requestId: value.requestId, channelId: value.channelId, created: value.created, category: value.category}
  }
  function applyDmOpen(view) {
    if (!view || !dmOpenRequestId || view.requestId !== dmOpenRequestId || dmOpenState !== "sending"
        || generation !== dmOpenGeneration || instanceId !== dmOpenInstance || view.state === "sending") return
    dmOpenTimeout.stop()
    dmOpenState = view.state
    dmOpenCategory = view.category || ""
    if (view.state === "acknowledged") {
      dmSelection = []
      if (view.channelId) dmOpenTarget = view.channelId
    }
  }
  function selectOpenedDm() {
    if (dmOpenTarget === "" || !dmRooms.some(function(room) { return room.id === root.dmOpenTarget })) return
    var target = dmOpenTarget
    dmOpenTarget = ""
    selectRoom(target)
  }

  readonly property var sample: sampleMode ? SampleData.snapshot().payload : null
  readonly property var viewModel: sample || ({ community: "Buzz" })
  property var catalogRooms: []
  property string catalogState: "unavailable"
  property string catalogCategory: ""
  readonly property string catalogLabel: sampleMode ? "Sample rooms" : ({unavailable: "Rooms unavailable", loading: "Loading rooms", partial: "Partial list · " + catalogRooms.length + " shown (limit 20)", ready: catalogRooms.length ? "Joined rooms · " + catalogRooms.length : "No joined rooms"})[catalogState]
  readonly property var rooms: sample ? sample.rooms : catalogRooms
  // Sample rooms predate room kinds and count as streams; validated frames always carry kind.
  readonly property var streamRooms: rooms.filter(function(room) { return room.kind !== "dm" })
  // Hidden DMs stay selectable in the catalog but are not listed.
  readonly property var dmRooms: rooms.filter(function(room) { return room.kind === "dm" && room.hidden !== true })
  readonly property var visibleRooms: streamRooms.concat(dmRooms)
  function roomTitle(room) { return room ? (room.kind === "dm" ? room.name : "# " + room.name) : "" }
  property string selectedRoomId: sampleMode ? "sample-general" : ""
  readonly property var selectedRoom: rooms.find(function(room) { return room.id === root.selectedRoomId }) || null
  property var historyRows: []
  property string historyState: "unavailable"
  property string historyCategory: ""
  property bool historySupported: false
  property bool automaticHistorySupported: false
  // The helper projects relay thread summaries: a row without one has no known replies.
  property bool threadSummariesSupported: false
  property var historyHasMore: null
  // Older pages: the helper holds the signed cursor; the panel only asks for the next page.
  property bool olderHistorySupported: false
  property var historyNextCursor: null
  property string historyOlderState: "idle"
  // The helper's live subscription for this room is primed. Live events only make the
  // helper refetch verified pages; the rows shown are still those pages.
  property bool liveUpdatesSupported: false
  property bool historyLive: false
  readonly property int threadRefreshInterval: historyLive ? 30000 : 8000
  property bool olderRequested: false
  property string olderRequestCursor: ""
  property string pendingOlderRequestId: ""
  property int olderRetryBudget: 0
  property string olderRetryRoom: ""
  property string olderRetryInstance: ""
  property int olderRetryGeneration: 0
  readonly property bool olderLoading: olderRequested || historyOlderState === "loading"
  readonly property bool canLoadOlder: !sampleMode && olderHistorySupported && connection === "authenticated"
    && historyState === "snapshot" && historyNextCursor !== null
  property bool threadSupported: false
  property string threadRootId: ""
  property string threadState: "unavailable"
  property string threadCategory: ""
  property var threadRows: []
  property var threadHasMore: null
  property string pendingThreadRequestId: ""
  // The helper blanks its views while it re-checks joined rooms (about every 30 seconds).
  // Within one authenticated scope the displayed snapshots stay until the helper's
  // views are re-established in order: catalog, history, recipients, open thread.
  property string resyncStage: ""
  // A submission made during that check is written once the helper can validate it.
  property string heldSubmission: ""
  property int threadRetryBudget: 0
  property string threadRetryRoom: ""
  property string threadRetryRoot: ""
  property string threadRetryInstance: ""
  property int threadRetryGeneration: 0
  readonly property string threadLabel: threadState === "loading" ? "Loading replies" : threadState === "snapshot"
    ? (threadRows.length ? "Replies" : "No replies in this snapshot") + (threadHasMore ? " · more replies exist" : "")
    : threadCategory === "thread_access_denied" ? "Replies unavailable for this room" : "Replies unavailable · try Refresh replies"
  readonly property var messages: sample ? sample.messages.filter(function(message) { return message.roomId === root.selectedRoomId }) : historyRows
  readonly property string historyLabel: historyState === "loading" ? "Loading recent snapshot" : historyState === "snapshot"
    ? (historyLive ? "Live" : automaticHistorySupported ? "Auto-refreshing snapshot" : "Snapshot") + " · " + historyRows.length + (historyRows.length === 1 ? " message" : " messages") + " shown · completeness unknown"
      + (!historyHasMore ? "" : historyCategory === "history_older_unheld" ? " · older messages exist but are not held" : " · older history available")
      + (historyOlderState === "unavailable" && !olderLoading ? " · older messages could not be loaded" : "") : ({request_busy: "Helper busy · refresh again", history_timeout: "History request timed out", history_invalid: "History response could not be validated", history_access_denied: "History unavailable for this room"})[historyCategory] || "History not available yet"
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
        : setupAssistAvailable
          ? providerInstructions + "\nEnter that relay address below and choose Use this relay. Hosted account sign-in stays in your browser. Never enter keys or account tokens in this panel."
        : providerInstructions + "\nLink manually in a terminal after installing the helper:\nomarchy-buzz setup relay <community-url>\nomarchy-buzz setup identity enroll\nEnroll the same existing Buzz identity using hidden input and your OS secret store, then Retry. Hosted account sign-in stays in your browser. Never enter keys or account tokens in this panel."

  // Setup assist (`setup_assist`): relay choice and identity creation go to the
  // helper only while it is not authenticated. The secret never reaches QML;
  // the helper answers with a status frame whose `identity` is the public key.
  property bool setupAssistSupported: false
  property string setupState: "idle"
  property string setupCategory: ""
  property string setupRequestId: ""
  property string setupRequestKind: ""
  property string setupInstance: ""
  property string createdIdentity: ""
  property bool publicKeyCopied: false
  readonly property bool setupAssistAvailable: setupAssistSupported && !sampleMode && !sessionFailed && instanceId !== ""
    && ["unconfigured", "disconnected", "unavailable"].indexOf(connection) !== -1 && category !== "identity_access_pending"
  readonly property bool identitySetupAvailable: setupAssistAvailable && connection === "unconfigured" && relay !== ""
    && (identity === "" || category === "identity_missing")
  // Only a relay without an identity can take a new one; the helper refuses otherwise.
  readonly property bool canCreateIdentity: identitySetupAvailable && identity === "" && setupState !== "sending"
  readonly property string setupCategoryLabel: setupState === "failed" ? (({
    setup_invalid_relay: "That relay address was not accepted. Use wss://… (ws:// only for this computer), without a path or login.",
    identity_exists: "This device already has an identity for this relay.",
    identity_unavailable: "The secret store could not keep a new identity. Unlock it and try again.",
    relay_unavailable: "Could not reach the relay to verify it. Check the address and try again.",
    setup_busy: "The helper is busy or did not confirm in time. Check the status above, then try again.",
    setup_not_allowed: "Setup is only available while this helper is not connected.",
    config_unavailable: "The helper could not save its configuration. Check its configuration folder."
  })[setupCategory] || "Setup did not complete. Try again.") : setupState === "sending" ? (setupRequestKind === "create_identity" ? "Creating identity…" : "Saving relay…") : ""
  readonly property string shortPublicKey: /^[a-f0-9]{64}$/.test(identity) ? identity.slice(0, 12) + "…" : ""
  function beginSetup(kind) {
    setupRequestId = correlationUuid()
    setupRequestKind = kind
    setupInstance = instanceId
    setupState = "sending"
    setupCategory = ""
    setupTimeout.restart()
  }
  function refuseSetup(category) {
    setupTimeout.stop()
    setupState = "failed"
    setupCategory = category
    setupRequestId = ""
  }
  function loseSetup() {
    setupTimeout.stop()
    if (setupState === "sending") { setupState = "idle"; setupCategory = "" }
    setupRequestId = ""
  }
  function setupRelay(url) {
    if (!setupAssistAvailable || !bridge.running || setupState === "sending") return false
    var value = typeof url === "string" ? url.trim() : ""
    // Shape only; the helper applies the same checks as `omarchy-buzz setup relay`.
    if (!value || value.length > 2048 || !/^wss?:\/\/[^\s@]+$/.test(value)) { refuseSetup("setup_invalid_relay"); return false }
    beginSetup("set_relay")
    bridge.write(JSON.stringify({version: 1, id: setupRequestId, type: "set_relay", url: value}) + "\n")
    return true
  }
  function createIdentity() {
    if (!canCreateIdentity || !bridge.running) return false
    beginSetup("create_identity")
    bridge.write(JSON.stringify({version: 1, id: setupRequestId, type: "create_identity"}) + "\n")
    return true
  }
  function copyPublicKey() {
    // Only the public key ever reaches the clipboard.
    if (!/^[a-f0-9]{64}$/.test(identity)) return false
    Quickshell.clipboardText = identity
    publicKeyCopied = true
    return true
  }
  onIdentityChanged: publicKeyCopied = false

  function chooseSetupProvider(provider) {
    // Presentation only: choosing a provider never writes config or sends IPC.
    if (provider === "hosted" || provider === "custom") setupProvider = provider
  }

  function selectRoom(roomId) {
    if (rooms.some(function(room) { return room.id === roomId })) {
      if (selectedRoomId !== roomId) { clearHistory(); clearRecipients() }
      selectedRoomId = roomId
      rememberRoom(roomId)
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
    reconcileInlineMentions(composerKey, text)
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
    reconcileInlineMentions(key, copy[key])
    deliveryState = "idle"
    deliveryCategory = ""
    submissionId = ""
    submissionText = ""
  }
  function mentionTokenPresent(text, token) {
    var offset = text.indexOf(token)
    while (offset !== -1) {
      var before = offset === 0 ? "" : text[offset - 1]
      var after = text[offset + token.length] || ""
      if ((!before || /[\s([{,;:]/.test(before)) && (!after || /[\s.,!?;:)\]}]/.test(after))) return true
      offset = text.indexOf(token, offset + 1)
    }
    return false
  }
  function reconcileInlineMentions(scope, text) {
    var tracked = inlineMentionDrafts[scope] || []
    var kept = tracked.filter(function(item) { return item.tokens.some(function(token) { return mentionTokenPresent(text, token) }) })
    var removed = tracked.filter(function(item) { return !kept.some(function(other) { return item.key === other.key }) })
    if (!removed.length) return
    var bindings = Object.assign({}, inlineMentionDrafts)
    bindings[scope] = kept
    inlineMentionDrafts = bindings
    var recipients = Object.assign({}, recipientDrafts)
    recipients[scope] = (recipients[scope] || []).filter(function(key) { return !removed.some(function(item) { return item.key === key }) })
    recipientDrafts = recipients
  }
  function insertMention(key, start, end) {
    if (recipientPickerLocked || recipientsState !== "snapshot" || recipientsRoomId !== selectedRoomId
        || !Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end > draftText.length || start >= end
        || (start > 0 && !/[\s([{,;:]/.test(draftText[start - 1]))
        || draftText[start] !== "@" || !/^@[^\s@]*$/.test(draftText.slice(start, end))) return -1
    var entry = recipientEntries.find(function(item) { return item.key === key && item.name.trim() })
    if (!entry || (selectedRecipients.indexOf(key) === -1 && selectedRecipients.length >= 20)) return -1
    var token = "@" + entry.name.trim()
    var sameName = recipientEntries.filter(function(item) { return item.name.trim().toLowerCase() === entry.name.trim().toLowerCase() })
    if (sameName.length > 1) {
      var length = 12
      while (length < 64 && sameName.some(function(item) { return item.key !== key && item.key.slice(0, length) === key.slice(0, length) })) length += 4
      token += "[" + key.slice(0, length) + "]"
    }
    var next = draftText.slice(0, start) + token + " " + draftText.slice(end)
    if (next.length > 4096 || utf8Size(next) > 4096) return -1
    updateDraft(next)
    var tracked = (inlineMentionDrafts[composerKey] || []).slice()
    var previous = tracked.find(function(item) { return item.key === key })
    if (previous || selectedRecipients.indexOf(key) === -1) {
      tracked = tracked.filter(function(item) { return item.key !== key })
      tracked.push({key:key, tokens:previous ? previous.tokens.concat([token]) : [token]})
      var bindings = Object.assign({}, inlineMentionDrafts)
      bindings[composerKey] = tracked
      inlineMentionDrafts = bindings
    }
    if (selectedRecipients.indexOf(key) === -1) toggleRecipient(key, true)
    return start + token.length + 1
  }
  function toggleRecipient(key, fromInline) {
    if (recipientPickerLocked) return
    if (fromInline !== true) {
      var bindings = Object.assign({}, inlineMentionDrafts)
      bindings[composerKey] = (bindings[composerKey] || []).filter(function(item) { return item.key !== key })
      inlineMentionDrafts = bindings
    }
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
    if (resyncStage !== "") { heldSubmission = JSON.stringify(request); return true }
    bridge.write(JSON.stringify(request) + "\n")
    deliveryTimeout.restart()
    return true
  }
  function dropHeldSubmission() {
    if (heldSubmission === "") return false
    // Nothing was written, so this outcome is a known failure with the draft retained.
    heldSubmission = ""
    if (deliveryState === "sending") { deliveryState = "failed"; deliveryCategory = "send_unavailable" }
    return true
  }
  function losePendingDelivery() {
    deliveryTimeout.stop()
    if (dropHeldSubmission()) return
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
        reconcileInlineMentions(submissionDraftKey, "")
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
  function endResync(settled) {
    resyncTimeout.stop()
    resyncStage = ""
    if (heldSubmission === "") return
    if (settled === true && bridge.running && deliveryState === "sending" && generation === submissionGeneration
        && instanceId === submissionInstance) {
      bridge.write(heldSubmission + "\n")
      heldSubmission = ""
      deliveryTimeout.restart()
    } else dropHeldSubmission()
  }
  function advanceResync(stage) {
    if (stage === "thread" && (threadRootId === "" || threadState !== "snapshot")) stage = ""
    if (stage === "") { endResync(true); return }
    resyncStage = stage
    if (stage === "history") refreshHistory()
    else if (stage === "recipients") refreshRecipients()
    else if (stage === "thread") { pendingThreadRequestId = ""; refreshThread() }
  }
  function clearHistory() {
    endResync()
    clearThread()
    activityObservation = ActivityObserver.fresh()
    pendingHistoryRequestId = ""
    historyRows = []
    historyState = "unavailable"
    historyCategory = ""
    historyHasMore = null
    historyNextCursor = null
    historyOlderState = "idle"
    historyLive = false
    clearOlderRequest()
  }
  function clearOlderRequest() {
    olderRetry.stop()
    olderTimeout.stop()
    olderRequested = false
    olderRequestCursor = ""
    pendingOlderRequestId = ""
  }
  // One bounded, quiet request for the next older page; what is shown is never cleared.
  function loadOlder() {
    if (!canLoadOlder || olderLoading || !bridge.running || instanceId === "") return false
    olderRequested = true
    olderRequestCursor = JSON.stringify(historyNextCursor)
    olderRetryBudget = 2
    olderRetryRoom = selectedRoomId
    olderRetryInstance = instanceId
    olderRetryGeneration = generation
    olderTimeout.restart()
    send("fetch_older", selectedRoomId)
    return true
  }
  function refreshHistory() {
    if (sampleMode || !historySupported || connection !== "authenticated" || !selectedRoom) return
    if (historyState !== "snapshot") historyState = "loading"
    send("fetch_recent", selectedRoomId)
  }
  function clearThread() {
    threadRetry.stop()
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
    threadRetryBudget = 2
    threadRetryRoom = selectedRoomId
    threadRetryRoot = id
    threadRetryInstance = instanceId
    threadRetryGeneration = generation
    send("fetch_thread", selectedRoomId, id)
  }
  function refreshThread() {
    if (!threadRootId || pendingThreadRequestId) return
    if (threadState !== "snapshot") { openThread(threadRootId); return }
    // Refresh an already displayed thread without clearing it or entering loading.
    threadRetryBudget = 2
    threadRetryRoom = selectedRoomId
    threadRetryRoot = threadRootId
    threadRetryInstance = instanceId
    threadRetryGeneration = generation
    send("fetch_thread", selectedRoomId, threadRootId)
  }
  function sameProjection(before, after) { return JSON.stringify(before) === JSON.stringify(after) }
  function closeThread() { clearThread(); if (threadSupported) send("close_thread") }
  // Up to 200 replies, oldest first. Each names its parent: the root at depth 1,
  // otherwise an earlier reply one level up. Anything else rejects the frame.
  function validatedThread(value) {
    var failures = ["thread_unavailable", "thread_timeout", "thread_invalid", "thread_access_denied"]
    var snapshots = ["thread_completeness_unknown", "thread_more_unshown", "thread_replies_hidden"]
    if (!value || !Array.isArray(value.rows) || value.rows.length > 200
        || (value.category !== null && failures.concat(snapshots).indexOf(value.category) === -1)
        || (value.rootId !== null && (typeof value.rootId !== "string" || !/^[a-f0-9]{64}$/.test(value.rootId)))
        || ((value.roomId === null) !== (value.rootId === null))
        || (value.state !== "unavailable" && value.rootId === null)) return null
    // "More exist" is a helper heuristic; its category and hasMore must agree.
    if (value.state === "snapshot" && (snapshots.indexOf(value.category) === -1
        || value.category === "thread_more_unshown" && value.hasMore !== true
        || value.category === "thread_completeness_unknown" && value.hasMore !== false)) return null
    var checked = validatedHistory({state:value.state, roomId:value.roomId, rows:value.rows, hasMore:value.hasMore, category:null}, 200)
    if (!checked || checked.rows.some(function(row) { return row.id === value.rootId })) return null
    var depths = ({})
    for (var i = 0; i < checked.rows.length; i++) {
      var depth = value.rows[i].depth
      var parent = value.rows[i].parent
      if (!Number.isInteger(depth) || depth < 1 || depth > 64 || typeof parent !== "string" || !/^[a-f0-9]{64}$/.test(parent)
          || (parent === value.rootId ? depth !== 1 : !depths.hasOwnProperty(parent) || depths[parent] + 1 !== depth)) return null
      depths[checked.rows[i].id] = depth
      checked.rows[i].depth = depth
      checked.rows[i].parent = parent
    }
    checked.rootId = value.rootId
    checked.category = value.category || ""
    return checked
  }
  function formatTimestamp(seconds) { return Qt.formatDateTime(new Date(seconds * 1000), "yyyy-MM-dd HH:mm:ss t") }
  function formatTime(seconds) { return Qt.formatDateTime(new Date(seconds * 1000), "h:mm AP") }
  function dayKey(seconds) { return Qt.formatDateTime(new Date(seconds * 1000), "yyyy-MM-dd") }
  function formatDay(seconds) {
    var now = new Date()
    var yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1)
    var day = new Date(seconds * 1000)
    if (dayKey(seconds) === Qt.formatDateTime(now, "yyyy-MM-dd")) return "Today"
    if (dayKey(seconds) === Qt.formatDateTime(yesterday, "yyyy-MM-dd")) return "Yesterday"
    return Qt.formatDateTime(day, day.getFullYear() === now.getFullYear() ? "dddd, MMMM d" : "dddd, MMMM d, yyyy")
  }
  readonly property var threadRoot: threadRootId ? historyRows.find(function(row) { return row.id === root.threadRootId }) || null : null
  readonly property string threadCountLabel: threadState === "loading" ? "Loading replies" : threadState !== "snapshot"
    ? (threadCategory === "thread_access_denied" ? "Replies unavailable for this room" : "Replies unavailable")
    : (threadRows.length === 0 ? (threadCategory === "thread_replies_hidden" ? "No visible replies" : "No replies yet")
      : threadHasMore ? "First " + threadRows.length + " replies · more exist"
      : threadRows.length + (threadRows.length === 1 ? " reply" : " replies"))
      + (threadCategory === "thread_replies_hidden" ? " · some hidden" : "")
  function clearRecipients() {
    recipientsRetry.stop()
    pendingRecipientsRequestId = ""
    recipientsRoomId = ""
    recipientEntries = []
    agentProfiles = []
    recipientsState = "unavailable"
    recipientsCategory = ""
    recipientsPartial = false
  }
  function recipientsRetained() {
    return recipientsState === "snapshot" && recipientsRoomId === selectedRoomId && selectedRoomId !== ""
  }
  function refreshRecipients() {
    var available = !sampleMode && recipientsSupported && connection === "authenticated" && selectedRoom
    // Refreshing the roster already on screen keeps its names and mentions visible.
    if (available && recipientsRetained()) {
      recipientsRetry.stop()
      pendingRecipientsRequestId = ""
    } else clearRecipients()
    if (!available) return
    recipientsRetryBudget = 2
    recipientsRetryRoom = selectedRoomId
    recipientsRetryInstance = instanceId
    recipientsRetryGeneration = generation
    if (recipientsState !== "snapshot") recipientsState = "loading"
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
    return Array.isArray(capabilities) && capabilities.length >= 1 && capabilities.length <= 15
      && capabilities.indexOf("connection_status") !== -1
      && capabilities.every(function(cap, index) {
        return ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients", "history_auto_refresh", "room_activity", "agent_profiles", "thread_replies", "thread_send", "thread_summaries", "dm_open", "older_history", "live_updates", "setup_assist"].indexOf(cap) !== -1 && capabilities.indexOf(cap) === index
      })
  }
  // Streams carry no participants and are never hidden. A DM lists 2-9 distinct
  // participant keys (pinned Buzz bounds, self included); hidden is the viewer's
  // NIP-DV state, verified by the helper. Presentation data only, never authority.
  function validRoomKind(room) {
    if (typeof room.hidden !== "boolean" || !Array.isArray(room.participants) || room.participants.length > 9) return false
    var seen = ({})
    for (var i = 0; i < room.participants.length; i++) {
      var key = room.participants[i]
      if (typeof key !== "string" || !/^[a-f0-9]{64}$/.test(key) || seen[key]) return false
      seen[key] = true
    }
    if (room.kind === "stream") return room.participants.length === 0 && room.hidden === false
    return room.kind === "dm" && room.participants.length >= 2
  }
  function validatedCatalog(catalog) {
    if (!catalog || ["unavailable", "loading", "partial", "ready"].indexOf(catalog.state) === -1
        || !Array.isArray(catalog.rooms) || catalog.rooms.length > 20
        || (catalog.category !== null && ["room_catalog_unavailable", "room_catalog_partial", "room_catalog_timeout", "room_catalog_invalid", "room_catalog_unsupported", "relay_identity_unavailable", "relay_identity_changed"].indexOf(catalog.category) === -1)) return null
    var clean = []
    var ids = ({})
    for (var i = 0; i < catalog.rooms.length; i++) {
      var room = catalog.rooms[i]
      if (!room || typeof room !== "object" || Array.isArray(room)
          || Object.keys(room).sort().join(",") !== "description,hidden,id,kind,name,participants"
          || typeof room.id !== "string" || !/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(room.id)
          || ids[room.id] || !boundedString(room.name, 128) || !room.name.trim()
          || !boundedString(room.description, 512) || !validRoomKind(room)) return null
      ids[room.id] = true
      clean.push({id: room.id, name: room.name, description: room.description, kind: room.kind, participants: room.participants.slice(), hidden: room.hidden})
    }
    if (["unavailable", "loading"].indexOf(catalog.state) !== -1 && clean.length !== 0) return null
    return {state: catalog.state, rooms: clean, category: catalog.category || ""}
  }
  function validatedHistory(history, limit) {
    var uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/
    if (!history || ["unavailable", "loading", "snapshot"].indexOf(history.state) === -1
        || (history.roomId !== null && (typeof history.roomId !== "string" || !uuid.test(history.roomId)))
        || !Array.isArray(history.rows) || history.rows.length > (limit || 100)
        || (history.hasMore !== null && typeof history.hasMore !== "boolean")
        || (history.category !== null && ["history_unavailable", "history_timeout", "history_invalid", "history_access_denied", "history_completeness_unknown", "history_older_unheld"].indexOf(history.category) === -1)) return null
    if (history.state === "snapshot" && (history.roomId === null || typeof history.hasMore !== "boolean")) return null
    // Absent from helpers without older pages: no cursor, nothing loading.
    var cursor = history.nextCursor === undefined ? null : history.nextCursor
    var olderState = history.olderState === undefined ? "idle" : history.olderState
    if (cursor !== null && (typeof cursor !== "object" || Array.isArray(cursor) || Object.keys(cursor).length !== 2
        || !Number.isInteger(cursor.createdAt) || cursor.createdAt < 0 || cursor.createdAt > 253402300799
        || typeof cursor.id !== "string" || !/^[a-f0-9]{64}$/.test(cursor.id)
        || history.state !== "snapshot" || history.hasMore !== true)) return null
    if (["idle", "loading", "unavailable"].indexOf(olderState) === -1) return null
    // Absent from helpers without live updates. Only a snapshot can be live.
    var live = history.live === undefined ? false : history.live
    if (typeof live !== "boolean" || (live && history.state !== "snapshot")) return null
    if (history.state !== "snapshot" && history.rows.length !== 0) return null
    var clean = []
    var ids = ({})
    for (var i = 0; i < history.rows.length; i++) {
      var row = history.rows[i]
      var reactions = row && row.reactions
      var thread = row && row.thread
      if (!row || typeof row.id !== "string" || !/^[a-f0-9]{64}$/.test(row.id) || ids[row.id]
          || typeof row.author !== "string" || !/^[a-f0-9]{64}$/.test(row.author)
          || !Number.isInteger(row.time) || row.time < 0 || row.time > 253402300799
          || !boundedString(row.text, 2048) || typeof row.edited !== "boolean"
          || typeof row.truncated !== "boolean" || typeof row.unavailable !== "boolean"
          || (reactions != null && (!Number.isInteger(reactions.seen) || reactions.seen < 0 || reactions.seen > 200
            || !Number.isInteger(reactions.working) || reactions.working < 0 || reactions.working > 200))
          || (thread != null && (typeof thread !== "object" || Array.isArray(thread)
            || !Number.isInteger(thread.replies) || thread.replies < 0 || thread.replies > 1000000
            || (thread.lastReplyAt !== null && (!Number.isInteger(thread.lastReplyAt) || thread.lastReplyAt < 0 || thread.lastReplyAt > 253402300799))
            || !Array.isArray(thread.participants) || thread.participants.length > 10
            || !thread.participants.every(function(key, index) { return typeof key === "string" && /^[a-f0-9]{64}$/.test(key) && thread.participants.indexOf(key) === index })))) return null
      try { if (encodeURIComponent(row.text).replace(/%[A-F0-9]{2}/gi, "x").length > 2048) return null }
      catch (_) { return null }
      ids[row.id] = true
      clean.push({id: row.id, author: row.author, time: row.time, text: row.unavailable ? "" : row.text,
        edited: row.edited, truncated: row.truncated, unavailable: row.unavailable,
        reactions: reactions == null ? null : {seen: reactions.seen, working: reactions.working},
        thread: thread == null ? null : {replies: thread.replies, lastReplyAt: thread.lastReplyAt, participants: thread.participants.slice()}})
    }
    return {state: history.state, roomId: history.roomId, rows: clean, hasMore: history.hasMore, category: history.category || "",
      nextCursor: cursor === null ? null : {createdAt: cursor.createdAt, id: cursor.id}, olderState: olderState, live: live}
  }
  function beginSession() {
    loseSetup()
    setupAssistSupported = false
    losePendingDelivery()
    loseDmOpen()
    dmOpenSupported = false
    sendSupported = false
    clearCatalog()
    instanceId = ""
    relay = ""
    sessionFailed = false
    historySupported = false
    threadSupported = false
    threadSendSupported = false
    threadSummariesSupported = false
    olderHistorySupported = false
    recipientsSupported = false
    generation = 0
    connection = "connecting"
    category = ""
  }
  function fail(reason) {
    loseSetup()
    setupAssistSupported = false
    losePendingDelivery()
    loseDmOpen()
    dmOpenSupported = false
    sendSupported = false
    handshake.stop()
    clearCatalog()
    sessionFailed = true
    historySupported = false
    threadSupported = false
    threadSendSupported = false
    threadSummariesSupported = false
    olderHistorySupported = false
    recipientsSupported = false
    relay = ""
    connection = "unavailable"
    category = reason
    bridge.running = false
  }
  function boundedString(value, limit) { return typeof value === "string" && value.length <= limit }
  function acceptFrame(line) {
    if (sessionFailed) return false
    if (!boundedString(line, 1048576)) { fail("invalid_response"); return false }
    var frame
    try { frame = JSON.parse(line) } catch (_) { fail("invalid_response"); return false }
    if (frame && frame.version === 1 && frame.type === "error" && ["request_busy", "send_busy", "send_scope_changed", "send_request_reused", "send_invalid", "send_unavailable", "send_access_denied", "send_ledger_unavailable", "delivery_unknown",
        "dm_open_busy", "dm_open_scope_changed", "dm_open_request_reused", "dm_open_invalid", "dm_open_unavailable", "dm_open_access_denied", "dm_open_unknown",
        "setup_invalid_relay", "identity_exists", "identity_unavailable", "relay_unavailable", "setup_busy", "setup_not_allowed", "config_unavailable"].indexOf(frame.category) !== -1) {
      if (instanceId === "" || frame.instanceId !== instanceId) return false
      if (!boundedString(frame.id, 128) || !/^ui-[0-9]+$/.test(frame.id) && !uuidValue(frame.id)) { fail("invalid_response"); return false }
      if (frame.id === setupRequestId && setupState === "sending") {
        refuseSetup(frame.category === "request_busy" ? "setup_busy" : frame.category)
        return true
      }
      if (frame.id === dmOpenRequestId && dmOpenState === "sending") {
        // Refusals are known: nothing was signed. A lost helper reply is not.
        dmOpenTimeout.stop()
        dmOpenState = frame.category === "dm_open_unknown" ? "unknown" : "failed"
        dmOpenCategory = frame.category === "request_busy" ? "dm_open_busy" : frame.category
      }
      if (frame.id === submissionId && deliveryState === "sending") {
        deliveryTimeout.stop()
        deliveryState = ["send_scope_changed", "delivery_unknown"].indexOf(frame.category) !== -1 ? "unknown" : "failed"
        deliveryCategory = frame.category === "request_busy" ? "send_busy" : frame.category
      }
      if (frame.id === pendingHistoryRequestId) {
        // A busy refresh of a displayed snapshot leaves it for the next automatic update.
        if (historyState === "snapshot") pendingHistoryRequestId = ""
        else {
          clearHistory()
          historyCategory = "request_busy"
        }
      }
      if (frame.id === pendingOlderRequestId) {
        pendingOlderRequestId = ""
        if (frame.category === "request_busy" && olderRetryBudget > 0) {
          olderRetryBudget--
          olderRetry.restart()
        } else clearOlderRequest()
      }
      if (frame.id === pendingThreadRequestId) {
        pendingThreadRequestId = ""
        if (frame.category === "request_busy" && threadRetryBudget > 0) {
          threadRetryBudget--
          threadRetry.restart()
        } else {
          threadRows = []
          threadState = "unavailable"
          threadCategory = "thread_unavailable"
        }
      }
      if (frame.id === pendingRecipientsRequestId) {
        if (recipientsRetained()) pendingRecipientsRequestId = ""
        else {
          clearRecipients()
          recipientsCategory = "recipients_unavailable"
        }
        if (frame.category === "request_busy" && recipientsRetryBudget > 0) {
          recipientsRetryBudget--
          recipientsRetry.restart()
        }
      }
      // Only the read-only recipient lookup gets bounded, scope-fenced retries.
      // Message submissions are never retried automatically.
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
      if (!history || (history.live && frame.capabilities.indexOf("live_updates") === -1)) { fail("invalid_response"); return false }
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
    var supportsDmOpen = frame.capabilities.indexOf("dm_open") !== -1
    var dmOpen = supportsDmOpen ? validatedDmOpen(state.dmOpen) : null
    if (supportsDmOpen && !dmOpen) { fail("invalid_response"); return false }
    var supportsActivity = frame.capabilities.indexOf("room_activity") !== -1
    if (supportsActivity && (!Array.isArray(state.activity) || state.activity.length > 20 || state.activity.some(function(a, i) {
      return !RoomActivity.valid(a) || !uuidValue(a.roomId) || !catalog.rooms.some(function(r) { return r.id === a.roomId })
        || state.activity.slice(0,i).some(function(b) { return b.roomId === a.roomId })
    }))) { fail("invalid_response"); return false }
    var incomingScope = (state.relay || "") + "|" + (state.identity || "")
    if (draftScopeKey && (incomingScope !== draftScopeKey || (instanceId !== "" && frame.generation !== generation))) {
      losePendingDelivery()
      loseDmOpen()
      dmSelection = []
      clearThread()
      drafts = ({})
      recipientDrafts = ({})
      inlineMentionDrafts = ({})
      replyTargets = ({})
      submissionText = ""
    }
    draftScopeKey = incomingScope
    if (state.connection !== "authenticated") { losePendingDelivery(); loseDmOpen() }
    var previousCatalogState = catalogState
    var resyncingCatalog = resyncStage === "catalog"
    if (state.connection !== "authenticated") catalog = {state: "unavailable", rooms: [], category: ""}
    if (frame.generation !== generation || state.connection !== "authenticated") clearCatalog()
    var selectedBeforeCatalog = selectedRoomId
    var quietCatalog = catalog.state === "loading" && frame.instanceId === instanceId && connection === "authenticated"
      && (resyncStage !== "" || (["partial", "ready"].indexOf(catalogState) !== -1 && selectedRoom !== null && historyState === "snapshot"))
    if (quietCatalog) {
      if (resyncStage === "") {
        resyncStage = "catalog"
        resyncTimeout.restart()
      }
    } else {
      if (!sameProjection(catalogRooms, catalog.rooms)) catalogRooms = catalog.rooms
      catalogState = catalog.state
      catalogCategory = catalog.category
    }
    if (["partial", "ready"].indexOf(catalogState) !== -1
        && !catalogRooms.some(function(room) { return room.id === root.selectedRoomId })) {
      clearHistory()
      clearRecipients()
      var remembered = rememberedScope === incomingScope && visibleRooms.some(function(room) { return room.id === root.rememberedRoom })
      selectedRoomId = remembered ? rememberedRoom : visibleRooms.length ? visibleRooms[0].id : ""
    }
    historySupported = supportsHistory
    if (state.connection !== "authenticated" || !supportsHistory || ["loading", "unavailable"].indexOf(catalogState) !== -1) clearHistory()
    else if (history && history.roomId === selectedRoomId && selectedRoomId !== "") {
      if (!(history.state === "loading" && historyState === "snapshot")) {
        if (!sameProjection(historyRows, history.rows)) historyRows = history.rows
        historyState = history.state
        historyCategory = history.category
        historyHasMore = history.hasMore
        if (!sameProjection(historyNextCursor, history.nextCursor)) historyNextCursor = history.nextCursor
        historyOlderState = history.olderState
        historyLive = history.live
        // The helper took the request over, or the cursor it answered has moved on.
        if (olderRequested && (history.olderState === "loading" || JSON.stringify(history.nextCursor) !== olderRequestCursor)) clearOlderRequest()
      } else historyLive = false // a refresh closes the live subscription; the kept rows are a snapshot again
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
      // Loading frames for a same-scope refresh carry an empty row list.
      // Keep the last validated snapshot visible until the refresh finishes.
      if (!(thread.state === "loading" && threadState === "snapshot")) {
        if (!sameProjection(threadRows, thread.rows)) threadRows = thread.rows
        threadState = thread.state
        threadCategory = thread.category
        threadHasMore = thread.hasMore
      }
      if (thread.state !== "loading") {
        threadRetry.stop()
        pendingThreadRequestId = ""
        if (resyncStage === "thread") endResync(true)
      }
    } else if (thread && thread.rootId === null && threadState !== "loading"
        && (resyncStage === "" || (resyncStage === "thread" && thread.category === "thread_access_denied"))) {
      var resyncing = resyncStage !== ""
      clearThread()
      if (resyncing) endResync(true)
    }
    if (state.connection !== "authenticated" || !supportsRecipients || ["loading", "unavailable"].indexOf(catalogState) !== -1) clearRecipients()
    else if (recipients && recipients.roomId === selectedRoomId && selectedRoomId !== "") {
      // Loading frames for a same-room refresh carry an empty roster.
      if (!(recipients.state === "loading" && recipientsRetained())) {
        recipientsRoomId = recipients.roomId
        if (!sameProjection(recipientEntries, recipients.entries)) recipientEntries = recipients.entries
        if (!sameProjection(agentProfiles, agents)) agentProfiles = agents
        recipientsState = recipients.state
        recipientsCategory = recipients.category
        recipientsPartial = recipients.partial
      }
    }
    roomActivitySupported = supportsActivity
    if (!supportsActivity || state.connection !== "authenticated" || typeof state.identity !== "string" || catalogState === "unavailable") {
      roomActivity = RoomActivity.fresh()
    } else if (catalogState !== "loading" && !quietCatalog) {
      var activityUpdate = RoomActivity.update(roomActivity,
        incomingScope + "|" + frame.instanceId + "|" + frame.generation, state.activity,
        panelOpen && historyState === "snapshot" ? selectedRoomId : "")
      roomActivity = activityUpdate.state
      if (activityUpdate.notify) notifyActivity()
    }
    recipientsSupported = supportsRecipients
    automaticHistorySupported = frame.capabilities.indexOf("history_auto_refresh") !== -1
    threadSummariesSupported = supportsHistory && frame.capabilities.indexOf("thread_summaries") !== -1
    olderHistorySupported = supportsHistory && frame.capabilities.indexOf("older_history") !== -1
    liveUpdatesSupported = supportsHistory && frame.capabilities.indexOf("live_updates") !== -1
    instanceId = frame.instanceId
    generation = frame.generation
    relay = state.relay || ""
    connection = state.connection
    category = state.category || ""
    sendSupported = supportsSend
    identity = state.identity || ""
    dmOpenSupported = supportsDmOpen
    if (!supportsDmOpen) loseDmOpen()
    setupAssistSupported = frame.capabilities.indexOf("setup_assist") !== -1
    if (frame.type === "status" && setupState === "sending" && frame.id === setupRequestId && frame.instanceId === setupInstance) {
      setupTimeout.stop()
      if (setupRequestKind === "create_identity" && identity !== "") createdIdentity = identity
      setupState = "idle"
      setupRequestId = ""
    }
    if (createdIdentity !== "" && createdIdentity !== identity) createdIdentity = ""
    applyDelivery(delivery)
    applyDmOpen(dmOpen)
    handshake.stop()
    if (frame.type === "hello" && bridge.running) send("subscribe")
    // One fetch per first population/catalog refresh completion, never per status echo.
    var catalogSettled = previousCatalogState === "loading" || (resyncingCatalog && !quietCatalog)
    if (supportsHistory && connection === "authenticated" && selectedRoom
        && (selectedBeforeCatalog === "" || catalogSettled)
        && historyState !== "snapshot") refreshHistory()
    if (supportsRecipients && connection === "authenticated" && selectedRoom
        && (selectedBeforeCatalog === "" || catalogSettled)
        && recipientsState !== "snapshot") refreshRecipients()
    if (resyncStage !== "" && (!selectedRoom || historyState !== "snapshot")) endResync()
    else if (resyncStage === "catalog" && !quietCatalog) advanceResync(supportsHistory ? "history" : "")
    else if (resyncStage === "history" && history && history.state === "snapshot" && history.roomId === selectedRoomId)
      advanceResync(supportsRecipients ? "recipients" : "thread")
    else if (resyncStage === "recipients" && recipients && recipients.state !== "loading" && recipients.roomId === selectedRoomId)
      advanceResync("thread")
    selectOpenedDm()
    return true
  }
  function send(kind, roomId, rootId) {
    if (!sessionFailed && bridge.running && instanceId !== "") {
      requestSequence++
      var request = { version: 1, id: "ui-" + requestSequence, type: kind }
      if (kind === "fetch_recent") { request.roomId = roomId; pendingHistoryRequestId = request.id }
      if (kind === "fetch_thread") { request.roomId = roomId; request.rootId = rootId; pendingThreadRequestId = request.id }
      if (kind === "fetch_recipients") { request.roomId = roomId; pendingRecipientsRequestId = request.id }
      if (kind === "fetch_older") { request.roomId = roomId; pendingOlderRequestId = request.id }
      bridge.write(JSON.stringify(request) + "\n")
    }
  }
  function retry() {
    if (sampleMode) return
    if (agentService.autoConnect) agentService.retry()
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
  // The separate agent service (docs/AGENTS_SERVICE.md), reached through the same
  // helper binary. Fixtures without autoConnect start it explicitly.
  readonly property alias agents: agentService
  AgentService {
    id: agentService
    mainService: root
    helperExecutable: root.helperExecutable
    autoConnect: root.autoConnect && !root.sampleMode
  }

  Timer {
    id: threadRetry
    interval: 300
    onTriggered: {
      if (root.sessionFailed || root.connection !== "authenticated" || ["loading", "snapshot"].indexOf(root.threadState) === -1
          || root.selectedRoomId !== root.threadRetryRoom || root.threadRootId !== root.threadRetryRoot
          || root.instanceId !== root.threadRetryInstance || root.generation !== root.threadRetryGeneration
          || !root.canOpenThread(root.threadRetryRoot)) return
      root.send("fetch_thread", root.threadRetryRoom, root.threadRetryRoot)
    }
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
  // While the helper's live subscription is primed it refetches the open thread itself
  // on live replies and every 30 seconds; this panel refresh slows to match.
  Timer {
    id: threadRefresh
    interval: root.threadRefreshInterval
    repeat: true
    running: root.panelOpen && root.threadState === "snapshot" && !root.pendingThreadRequestId && root.resyncStage === "" && root.canOpenThread(root.threadRootId)
    onTriggered: root.refreshThread()
  }
  Timer {
    id: resyncTimeout
    interval: 20000
    onTriggered: {
      // The helper never re-established its views: stop presenting them as current.
      var stage = root.resyncStage
      root.endResync(false)
      if (stage === "catalog" || stage === "history") {
        root.clearHistory()
        root.clearRecipients()
        root.refreshHistory()
        root.refreshRecipients()
      } else if (stage === "thread") {
        root.threadRows = []
        root.threadState = "unavailable"
        root.threadCategory = "thread_timeout"
        root.pendingThreadRequestId = ""
      }
    }
  }
  Timer {
    id: recipientsRetry
    interval: 300
    onTriggered: {
      if (root.sessionFailed || root.connection !== "authenticated"
          || root.selectedRoomId !== root.recipientsRetryRoom
          || root.instanceId !== root.recipientsRetryInstance
          || root.generation !== root.recipientsRetryGeneration) return
      if (!root.recipientsRetained()) root.recipientsState = "loading"
      root.send("fetch_recipients", root.selectedRoomId)
    }
  }
  Timer {
    id: olderRetry
    interval: 300
    onTriggered: {
      if (root.sessionFailed || root.connection !== "authenticated" || !root.olderRequested
          || root.selectedRoomId !== root.olderRetryRoom || root.instanceId !== root.olderRetryInstance
          || root.generation !== root.olderRetryGeneration || !root.canLoadOlder) { root.clearOlderRequest(); return }
      root.send("fetch_older", root.selectedRoomId)
    }
  }
  Timer {
    id: olderTimeout
    // The helper bounds each older read to 15 seconds; stop waiting shortly after.
    interval: 20000
    onTriggered: root.clearOlderRequest()
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
    id: dmOpenTimeout
    // Beyond the helper's own 15-second bound: a missing answer is unknown, never refused.
    interval: 30000
    onTriggered: root.loseDmOpen()
  }
  Timer {
    id: setupTimeout
    // Beyond the helper's own 60-second bound. No answer is not a refusal:
    // the status line shows whether the change was saved.
    interval: 70000
    onTriggered: root.refuseSetup("setup_busy")
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
      root.loseSetup()
      root.setupAssistSupported = false
      root.losePendingDelivery()
      root.loseDmOpen()
      root.dmOpenSupported = false
      root.sendSupported = false
      handshake.stop()
      root.clearCatalog()
      root.historySupported = false
      root.threadSupported = false
      root.threadSummariesSupported = false
      root.olderHistorySupported = false
      root.threadSendSupported = false
      root.recipientsSupported = false
      root.sessionFailed = true
      root.relay = ""
      root.connection = "unavailable"
      if (!root.category) root.category = "helper_unavailable"
    }
  }
}
