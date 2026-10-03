import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "SampleData.js" as SampleData
import "ActivityObserver.js" as ActivityObserver
import "RoomActivity.js" as RoomActivity
import "Notifications.js" as Notifications

Item {
  id: root
  property var shell: null
  property var manifest: null
  // Only offscreen fixtures opt into sampleMode; production never loads sample rooms.
  property bool sampleMode: false
  property bool autoConnect: true
  // First setup, in Desktop's order: "join" an existing community (default) or
  // "create" a new one at buzz.xyz. Presentation only.
  property string setupProvider: "join"
  // Which observed messages raise a desktop notification, kept across shell restarts:
  // "direct" (default: mentions of me, DMs, threads I am in), "mentions", "dms",
  // "all" (plus general room activity) or "none". Only this choice and the text
  // flag are stored, never message content.
  property string notificationMode: "direct"
  // Show the sender's text in the notification body. Off: "New message from Alex".
  property bool notificationText: true
  readonly property bool notificationsEnabled: notificationMode !== "none"
  // The panel window has keyboard focus; the panel sets it. With the panel open
  // on a room it suppresses that room's notifications only while focused.
  property bool panelFocused: true
  readonly property string stateHome: Quickshell.env("XDG_STATE_HOME").startsWith("/")
    ? Quickshell.env("XDG_STATE_HOME") : Quickshell.env("HOME") + "/.local/state"
  readonly property string notificationSettingsDir: stateHome + "/omarchy-buzz"
  readonly property string notificationSettingsPath: notificationSettingsDir + "/notifications.json"
  property bool notificationSettingsLoaded: false
  property bool notificationSettingsDirReady: false
  property bool notificationPreferenceDirty: false
  property bool hydratingNotificationPreference: false
  property bool panelOpen: false
  // File choosers (portal dialogs) open right now; the overlay steps aside
  // while one is up, since the overlay layer would cover the dialog.
  property int filePickersOpen: 0
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
    if (panelOpen) reconnectOnOpen()
  }
  // Older helpers report activity without notices: one generic, content-free line.
  function notifyActivity() {
    if (notificationMode === "all" && !sampleMode && !notificationProcess.running && !notificationCooldown.running) {
      notificationProcess.command = ["timeout", "5s", "omarchy", "notification", "send", "--app-name", "Buzz",
        "-u", "normal", "-t", "5000", "Buzz", "New activity in your Buzz rooms."]
      notificationProcess.running = true
      notificationCooldown.start()
    }
  }
  // The helper's notice for one room (RoomActivity.update validated it); the
  // title, body and click target are built here from that bounded, plain data.
  property var notificationQueue: []
  // The queue holds notices, not wording: mode, suppression and the text choice
  // are checked again when each is sent, so a change in Settings applies at once.
  // Items are also bound to the session they came from (relay, identity, generation)
  // and to a room that is still in the catalog, so a community switch, a reconnect
  // or lost access drops them.
  readonly property string notificationScope: relay + "|" + identity + "|" + generation
  function noticeDeliverable(item) {
    if (sampleMode || item.scope !== notificationScope || connection !== "authenticated"
        || !rooms.some(function(r) { return r.id === item.roomId })
        || !Notifications.allows(notificationMode, item.notice.kind)) return false
    return !(panelOpen && panelFocused && historyState === "snapshot" && selectedRoomId === item.roomId)
  }
  function announceNotice(roomId, notice) {
    var item = {roomId: roomId, notice: notice, scope: notificationScope}
    if (!noticeDeliverable(item)) return
    notificationQueue = notificationQueue.concat([item]).slice(-5)
    sendNextNotification()
  }
  function sendNextNotification() {
    if (notificationProcess.running) return
    var item = null
    while (notificationQueue.length > 0 && !item) {
      var candidate = notificationQueue[0]
      notificationQueue = notificationQueue.slice(1)
      if (noticeDeliverable(candidate)) item = candidate
    }
    if (!item) return
    var room = rooms.find(function(r) { return r.id === item.roomId })
    var message = Notifications.compose(item.notice, room.kind === "dm", notificationText)
    var next = {title: message.title, body: message.body,
      target: JSON.stringify(item.notice.threadRoot ? {room: item.roomId, thread: item.notice.threadRoot} : {room: item.roomId})}
    // Fixed argv; relay text is only ever one argument (never a shell string,
    // never an option: Notifications.arg keeps it from starting with "-"), and
    // the click command carries only validated ids.
    notificationProcess.command = ["timeout", "5s", "omarchy", "notification", "send", "--app-name", "Buzz",
      "-u", "normal", "-t", "8000", Notifications.arg(next.title), Notifications.arg(next.body),
      "--exec", "omarchy-shell", "-q", "shell", "summon", "community.buzz", next.target]
    notificationProcess.running = true
  }
  // A notification click (Panel.open payload): show that room, and its thread.
  // Applied once the catalog and history are ready; abandoned after 15 s.
  property string targetRoomId: ""
  property string targetThreadId: ""
  function openNotificationTarget(roomId, threadId) {
    if (!uuidValue(roomId) || (threadId !== "" && !/^[a-f0-9]{64}$/.test(threadId))) return
    targetRoomId = roomId
    targetThreadId = threadId
    targetExpiry.restart()
    applyNotificationTarget()
  }
  function clearNotificationTarget() { targetRoomId = ""; targetThreadId = ""; targetExpiry.stop() }
  function applyNotificationTarget() {
    if (!targetRoomId || ["partial", "ready"].indexOf(catalogState) === -1) return
    if (!rooms.some(function(room) { return room.id === targetRoomId })) { clearNotificationTarget(); return }
    if (selectedRoomId !== targetRoomId) selectRoom(targetRoomId)
    if (targetThreadId === "") { clearNotificationTarget(); return }
    if (selectedRoomId !== targetRoomId || historyState !== "snapshot") return
    // The thread's root may not be in the recent page: the room still opens.
    if (canOpenThread(targetThreadId)) openThread(targetThreadId)
    clearNotificationTarget()
  }
  Timer { id: targetExpiry; interval: 15000; onTriggered: root.clearNotificationTarget() }
  property var activityObservation: ActivityObserver.fresh()
  onNotificationModeChanged: {
    if (notificationMode === "none") notificationQueue = []
    notificationPreferenceChanged()
  }
  onNotificationTextChanged: notificationPreferenceChanged()
  function notificationPreferenceChanged() {
    activityObservation = ActivityObserver.fresh()
    if (hydratingNotificationPreference) return
    notificationPreferenceDirty = true
    if (notificationSettingsLoaded && notificationSettingsDirReady) notificationSettingsSave.restart()
  }

  function loadNotificationSettings(raw) {
    if (notificationSettingsLoaded) return
    // No file: the defaults. A malformed one fails closed (none). Version 1 only
    // had on/off for general activity: on is "all", off stays off.
    var mode = raw === "" ? "direct" : "none"
    var text = true
    try {
      var parsed = JSON.parse(raw)
      if (parsed && parsed.version === 1) mode = parsed.enabled === true ? "all" : "none"
      else if (parsed && parsed.version === 2 && Notifications.modes.indexOf(parsed.mode) !== -1 && typeof parsed.text === "boolean") {
        mode = parsed.mode
        text = parsed.text
      }
    } catch (error) { /* Malformed settings fail closed. */ }
    if (!notificationPreferenceDirty) {
      hydratingNotificationPreference = true
      notificationMode = mode
      notificationText = text
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
      notificationSettingsFile.setText(JSON.stringify({version: 2, mode: root.notificationMode, text: root.notificationText}) + "\n")
      root.notificationPreferenceDirty = false
    }
  }
  // The room last chosen by the user in each community, restored after a
  // switch or a shell restart. Only public room IDs keyed by their relay and
  // identity scope are stored (at most 16 scopes); no message data. Version 1
  // files (one scope) are still read.
  readonly property string viewSettingsPath: notificationSettingsDir + "/view.json"
  property var rememberedRooms: ({})
  readonly property int rememberedScopes: 16
  function rememberedRoomFor(scope) { return Object.prototype.hasOwnProperty.call(rememberedRooms, scope) ? rememberedRooms[scope] : "" }
  function validRememberedScope(scope) { return boundedString(scope, 2200) && /^[^|]+\|[^|]+$/.test(scope) }
  function loadViewSettings(raw) {
    try {
      var parsed = JSON.parse(raw)
      var rooms = ({})
      if (parsed && parsed.version === 1 && uuidValue(parsed.roomId) && validRememberedScope(parsed.scope)) rooms[parsed.scope] = parsed.roomId
      else if (parsed && parsed.version === 2 && parsed.rooms && typeof parsed.rooms === "object" && !Array.isArray(parsed.rooms)) {
        var scopes = Object.keys(parsed.rooms)
        if (scopes.length > rememberedScopes) return
        for (var i = 0; i < scopes.length; i++) {
          if (!validRememberedScope(scopes[i]) || !uuidValue(parsed.rooms[scopes[i]])) return
          rooms[scopes[i]] = parsed.rooms[scopes[i]]
        }
      }
      rememberedRooms = rooms
    } catch (error) { /* Missing or malformed settings select the first room. */ }
  }
  function rememberRoom(roomId) {
    // Both a relay and an identity are required: an incomplete scope is never remembered.
    if (sampleMode || !validRememberedScope(draftScopeKey) || !uuidValue(roomId)
        || rememberedRoomFor(draftScopeKey) === roomId) return
    var rooms = ({})
    var kept = Object.keys(rememberedRooms).filter(function(scope) { return scope !== root.draftScopeKey })
    // The most recent scope is written last; the oldest beyond the bound is dropped.
    kept = kept.slice(Math.max(0, kept.length - (rememberedScopes - 1)))
    for (var i = 0; i < kept.length; i++) rooms[kept[i]] = rememberedRooms[kept[i]]
    rooms[draftScopeKey] = roomId
    rememberedRooms = rooms
    if (notificationSettingsDirReady) viewSettingsFile.setText(JSON.stringify({version: 2, rooms: rememberedRooms}) + "\n")
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
  property var clockSkewSeconds: null
  // `status.reconnecting`: the helper retries a lost relay connection on its
  // own (`disconnected` or `connecting`; `category` names the failure).
  property bool helperReconnecting: false
  // The panel restarts an unexpectedly ended bridge on its own: after 1, 2, 5
  // and 10 seconds, then every 30 seconds. Fixtures leave this off.
  property bool autoRestart: autoConnect && !sampleMode
  property int bridgeRestarts: 0
  property double bridgeStartedAt: 0
  property double reconnectRequestedAt: 0
  readonly property bool reconnecting: !sampleMode && (bridgeRestart.running
    || helperReconnecting && (connection === "disconnected" || connection === "connecting"))
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
  // The pending attachment hashes the helper will add to this submission.
  property string submissionAttachments: ""
  property int submissionGeneration: 0
  property string submissionInstance: ""
  property string acknowledgedRefreshId: ""
  // Message actions (`message_actions`): one edit, delete or reaction at a time,
  // answered through the same delivery receipt as a send. Nothing is shown as
  // done until the relay accepted it; the rows then refresh from the helper.
  property bool messageActionsSupported: false
  property string actionId: ""
  property string actionKind: ""
  property string actionTarget: ""
  property string actionEmoji: ""
  property string actionRoom: ""
  property string actionInstance: ""
  property int actionGeneration: 0
  property string actionState: "idle"
  property string actionCategory: ""
  readonly property bool canAct: messageActionsSupported && !sampleMode && !sessionFailed && connection === "authenticated"
    && resyncStage === "" && actionState !== "sending" && deliveryState !== "sending"
  // Presentation only: the helper decides who may edit or delete (own messages).
  readonly property var quickReactions: ["👍", "❤️", "😂", "🎉", "🙏", "😮", "😢", "🚀"]
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
  // Keys a send from the active composer notifies: the explicit choices, then
  // hand-typed names resolved against this room's roster, at most 20 in all.
  readonly property var typedMentions: resolveTypedMentions(draftText)
  readonly property var outgoingMentions: {
    var keys = selectedRecipients.slice()
    typedMentions.forEach(function(key) { if (keys.indexOf(key) === -1 && keys.length < 20) keys.push(key) })
    return keys
  }
  readonly property string outgoingMentionNames: outgoingMentions.map(function(key) {
    var entry = recipientEntries.find(function(item) { return item.key === key })
    return entry && entry.name && entry.name.trim() ? entry.name.trim() : key.slice(0, 12) + "…"
  }).join(", ")
  readonly property bool recipientPickerLocked: deliveryState === "sending" || deliveryState === "unknown" || deliveryState === "rejected" || deliveryCategory === "send_request_reused"
  readonly property string recipientsLabel: recipientsState === "loading" ? "Loading recipients" : recipientsState === "snapshot" ? (recipientsPartial ? "Partial recipient list · " : "Room recipients · ") + recipientEntries.length : "Recipients unavailable"

  // A message action and a send share the helper's one publication slot.
  readonly property bool canSend: sendSupported && !sampleMode && !sessionFailed && connection === "authenticated"
    && selectedRoom !== null && deliveryState !== "sending" && actionState !== "sending" && deliveryState !== "unknown" && deliveryState !== "rejected" && deliveryCategory !== "send_request_reused"
    && replyReady && recipientIntentValid && (draftText.trim().length > 0 || pendingFor(replyRootId).length > 0)
    && draftText.indexOf("\u0000") === -1 && utf8Size(draftText) <= 4096 && !uploadingFor(replyRootId)
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
      && (text.trim().length > 0 || pendingFor(rootId).length > 0) && text.indexOf("\u0000") === -1 && utf8Size(text) <= 4096
      && !uploadingFor(rootId) && actionState !== "sending"
  }
  // A file still being checked or uploaded for this draft holds Send.
  function uploadingFor(rootId) {
    var scope = draftScopeFor(rootId)
    return (uploadLocal === "sending" && uploadLocalScope === scope) || (upload.state === "uploading" && upload.scope === scope)
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

  // Starting a DM (helper `dm_open`). People come only from what the helper
  // verified: existing DM participants, its people directory or search
  // (`people_search`) and the roster on screen; the relay decides. The viewer's
  // public key is used only to leave the viewer out of the choice.
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
  // The names shown on the chosen people's chips, by key.
  property var dmNames: ({})
  // The people directory or search (helper `people_search`). `peopleText` is
  // what the panel last asked for (empty: the directory); the view answers the
  // request with `peopleRequestId` and is never shown for another one.
  property bool peopleSupported: false
  property string peopleText: ""
  property string peopleRequestId: ""
  property bool peopleLost: false
  property string peopleViewRequestId: ""
  property string peopleViewState: "unavailable"
  property string peopleViewCategory: ""
  property var peopleEntries: []
  // Keys the helper served on this connection, as it allows them (at most 200).
  property var peopleServed: []
  readonly property bool peopleAnswered: peopleRequestId !== "" && peopleViewRequestId === peopleRequestId && peopleViewState !== "loading"
  // idle (not asked), loading, ready or failed.
  readonly property string peopleStatus: peopleRequestId === "" ? "idle"
    : peopleAnswered ? (peopleViewState === "snapshot" ? "ready" : "failed") : peopleLost ? "failed" : "loading"
  readonly property var dmCandidates: dmCandidateList()
  function personName(key) {
    var entry = recipientEntries.find(function(item) { return item.key === key }) || peopleEntries.find(function(item) { return item.key === key })
    return entry && entry.name ? entry.name : knownNames[key] || ""
  }
  // Names the helper has served for this community (any room's members, the
  // people directory), newest last. A roster that is reloading, failed or cut
  // at its bound must not turn names back into keys, so a name is replaced by
  // a newer one but never dropped while the community stays the same.
  property var knownNames: ({})
  property var knownNameOrder: []
  property string knownNamesScope: ""
  readonly property int knownNamesLimit: 1000
  function rememberNames(entries) {
    var next = null
    var order = knownNameOrder
    for (var i = 0; i < entries.length; i++) {
      var key = entries[i].key, name = entries[i].name
      if (typeof key !== "string" || typeof name !== "string" || !name.trim() || knownNames[key] === name) continue
      if (!next) { next = Object.assign({}, knownNames); order = order.slice() }
      if (next[key] === undefined) order.push(key)
      next[key] = name
    }
    if (!next) return
    while (order.length > knownNamesLimit) delete next[order.shift()]
    knownNameOrder = order
    knownNames = next
  }
  function forgetNames(scope) {
    knownNamesScope = scope
    knownNameOrder = []
    knownNames = ({})
  }
  // Existing conversations first, then the relay's directory or search, then
  // the open room's members while that read has not answered. Those typed
  // into the search apply to every source.
  function dmCandidateList() {
    var q = peopleText.toLowerCase()
    var seen = ({})
    seen[identity] = true
    dmSelection.forEach(function(key) { seen[key] = true })
    var list = []
    // `filtered`: the helper already matched and ranked it (name, nip05 or key).
    function add(key, name, entry, filtered) {
      if (seen[key] || (!filtered && q !== "" && name.toLowerCase().indexOf(q) === -1 && key.indexOf(q) !== 0)) return
      seen[key] = true
      list.push({key: key, name: name, label: participantLabel(key), status: entry && entry.status || null, presence: presenceOf(key)})
    }
    dmRooms.forEach(function(room) {
      var others = room.participants.filter(function(key) { return key !== identity })
      others.forEach(function(key) { add(key, personName(key) || (others.length === 1 ? room.name : ""), null) })
    })
    if (peopleStatus === "ready") peopleEntries.forEach(function(entry) { add(entry.key, entry.name, null, true) })
    else if (recipientsRetained()) recipientEntries.forEach(function(entry) { add(entry.key, entry.name, entry) })
    return list
  }
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
    if (peopleServed.indexOf(key) !== -1) return true
    if (recipientsState === "snapshot" && recipientsRoomId === selectedRoomId && selectedRoomId !== ""
        && recipientEntries.some(function(entry) { return entry.key === key })) return true
    return rooms.some(function(room) { return room.kind === "dm" && room.participants.indexOf(key) !== -1 })
  }
  function validDmKeys(keys) {
    return Array.isArray(keys) && keys.length >= 1 && keys.length <= 8
      && keys.every(function(key, index) { return keys.indexOf(key) === index && root.dmKeyAllowed(key) })
  }
  function toggleDmParticipant(key, name) {
    if (!dmOpenAvailable || dmOpenState === "sending") return false
    var copy = dmSelection.slice()
    var names = Object.assign({}, dmNames)
    var index = copy.indexOf(key)
    if (index !== -1) { copy.splice(index, 1); delete names[key] }
    else if (copy.length < 8 && dmKeyAllowed(key)) { copy.push(key); names[key] = typeof name === "string" ? name : "" }
    else return false
    dmSelection = copy
    dmNames = names
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
  // Asks the helper for the directory (empty text) or a prefix search.
  function searchPeople(text) {
    if (!dmOpenAvailable || !peopleSupported || typeof text !== "string") return false
    peopleText = text.trim()
    peopleLost = false
    peopleRequestId = ""
    send("search_people", peopleText)
    peopleLostTimer.restart()
    return peopleRequestId !== ""
  }
  function clearPeople() {
    peopleLostTimer.stop()
    peopleText = ""
    peopleRequestId = ""
    peopleLost = false
    peopleViewRequestId = ""
    peopleViewState = "unavailable"
    peopleViewCategory = ""
    peopleEntries = []
    peopleServed = []
  }
  function validatedPeople(value) {
    if (!value || ["unavailable", "loading", "snapshot"].indexOf(value.state) === -1
        || (value.requestId !== null && (typeof value.requestId !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(value.requestId)))
        || !boundedString(value.query, 64) || utf8Size(value.query) > 64 || /[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069\u200e\u200f\u061c]/.test(value.query)
        || !Array.isArray(value.entries) || value.entries.length > 50
        || (value.category !== null && ["people_unavailable", "people_timeout", "people_invalid"].indexOf(value.category) === -1)
        || (value.category !== null && value.state !== "unavailable")
        || (value.state !== "snapshot" && value.entries.length !== 0)
        || (value.state !== "unavailable" && value.requestId === null)) return null
    var seen = ({})
    var entries = []
    for (var i = 0; i < value.entries.length; i++) {
      var entry = value.entries[i]
      if (!entry || typeof entry.key !== "string" || !/^[a-f0-9]{64}$/.test(entry.key) || seen[entry.key]
          || !boundedString(entry.name, 64) || utf8Size(entry.name) > 64 || /[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069\u200e\u200f\u061c]/.test(entry.name)) return null
      seen[entry.key] = true
      entries.push({key: entry.key, name: entry.name})
    }
    return {state: value.state, requestId: value.requestId, entries: entries, category: value.category || ""}
  }
  function applyPeople(view) {
    var served = view.state === "snapshot" && (view.requestId !== peopleViewRequestId || view.state !== peopleViewState)
    peopleViewRequestId = view.requestId || ""
    peopleViewState = view.state
    peopleViewCategory = view.category
    if (!sameProjection(peopleEntries, view.entries)) peopleEntries = view.entries
    if (served) {
      // Only a newly served read is evidence: the helper repeats its last one.
      rememberNames(view.entries)
      var keys = view.entries.map(function(entry) { return entry.key })
      peopleServed = peopleServed.filter(function(key) { return keys.indexOf(key) === -1 }).concat(keys).slice(-200)
    }
    if (peopleAnswered) peopleLostTimer.stop()
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
  // Paging (`room_manage`): "none", "available", "loading", "failed" or "limit"
  // (the most rooms the panel holds are listed). Absent from older helpers.
  property string catalogMore: "none"
  property string catalogMoreCategory: ""
  // No joined rooms yet ("Rooms · none joined") reads better than an empty partial count.
  readonly property string catalogLabel: sampleMode ? "Sample rooms" : noRoomsJoined ? "Rooms · none joined"
    : ({unavailable: "Rooms unavailable", loading: "Loading rooms", partial: "Partial list · " + catalogRooms.length + " shown" + (catalogMore === "available" || catalogMore === "loading" || catalogMore === "failed" ? " · more available" : catalogMore === "limit" ? " (limit " + maxRooms + ")" : ""), ready: catalogRooms.length ? "Joined rooms · " + catalogRooms.length : "No joined rooms"})[catalogState]
  // The most joined rooms (DMs included) the helper lists (`catalog::MAX_ROOMS`).
  readonly property int maxRooms: 200
  readonly property var rooms: sample ? sample.rooms : catalogRooms
  // Sample rooms predate room kinds and count as streams; validated frames always carry kind.
  readonly property var streamRooms: rooms.filter(function(room) { return room.kind !== "dm" })
  // Hidden DMs stay selectable in the catalog but are not listed.
  readonly property var dmRooms: rooms.filter(function(room) { return room.kind === "dm" && room.hidden !== true })
  readonly property var visibleRooms: streamRooms.concat(dmRooms)
  // Connected to a community whose catalog lists no joined room yet (a fresh
  // join, or every room left): the panel shows its welcome pane instead of an
  // empty room view. Direct messages do not count as joined rooms.
  readonly property bool noRoomsJoined: !sampleMode && !sessionFailed && connection === "authenticated"
    && ["partial", "ready"].indexOf(catalogState) !== -1 && streamRooms.length === 0
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
  readonly property string barLabel: sampleMode ? "TEST" : category === "clock_skew" && connection !== "authenticated" ? "Clock" : reconnecting ? "Reconnecting" : ({unconfigured: "Setup", connecting: "Connecting", authenticated: "Connected", identity_locked: "Locked", disconnected: "Offline", unavailable: "Error"})[connection] || "Error"
  readonly property string barSymbol: sampleMode ? "T" : category === "clock_skew" && connection !== "authenticated" ? "!" : reconnecting ? "…" : ({unconfigured: "?", connecting: "…", authenticated: "✓", identity_locked: "!", disconnected: "○", unavailable: "!"})[connection] || "!"
  readonly property string statusLabel: sampleMode ? "Sample data" : category === "incompatible_response" ? "Incompatible helper" : category === "identity_access_pending" ? "Waiting for secret store unlock" : category === "clock_skew" && connection !== "authenticated" ? clockSkewText(clockSkewSeconds) : reconnecting ? (sessionFailed ? "Helper unavailable · reconnecting…" : "Reconnecting…") : ({
    unconfigured: "Setup required", connecting: "Connecting", authenticated: historyState === "snapshot" ? "Authenticated · recent snapshot" : historyState === "loading" ? "Authenticated · history loading"
      : noRoomsJoined && selectedRoom === null ? "Authenticated · no rooms joined yet" : "Authenticated · history unavailable",
    identity_locked: "Identity locked", disconnected: "Disconnected", unavailable: "Helper unavailable"
  })[connection] || "Unavailable"
  // Desktop's join wording (`AddCommunityDialog.tsx`), and the honest create
  // step: a third-party client cannot create a hosted community itself.
  readonly property string joinCommunityDescription: "Use the community URL or invite link you received."
  readonly property string createCommunityDescription: "New Buzz communities are created at buzz.xyz, in your browser; this panel cannot create one itself. Once yours exists, paste its URL or invite link here to join it."
  readonly property string providerInstructions: setupProvider === "create" ? createCommunityDescription
    : joinCommunityDescription + " That can be a hosted community or one you were invited to; you do not need to run your own relay."
  readonly property string setupInstructions: category === "incompatible_response"
    ? "Install a matching Buzz plugin and helper release, restart the helper service, then Retry. Updating the Omarchy plugin alone does not replace its helper. Your existing identity stays in the secret store."
    : (category === "config_unavailable" || category === "invalid_config")
    ? "Check your local helper configuration, then Retry. Credentials do not belong in that file."
    : category === "relay_resource_limit"
    ? "The relay sent more than this client accepts before sign-in finished (oversized or too many messages), so the connection was closed. The helper keeps retrying on its own; if it keeps happening, the relay may be misbehaving."
    : category === "clock_skew" && connection !== "authenticated"
    ? "This computer's clock disagrees with the relay's, so the relay refuses its sign-in. This often happens after the machine was suspended. On Omarchy run sudo systemctl restart systemd-timesyncd (or check timedatectl), then Retry. The helper keeps trying for a short while on its own."
    : (connection === "identity_locked" || category === "identity_access_pending")
      ? "Unlock your OS secret store, then Retry. Your existing identity is retained."
      : connection === "authenticated"
        ? "Relay authentication succeeded. Choose a joined room to fetch a recent snapshot. Use the composer to send plain text. Select exact room recipients when available; agent execution is configured separately."
        : setupAssistAvailable
          ? "Hosted account sign-in stays in your browser. Never enter keys or account tokens in this panel."
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

  // Joining a community (`community_join`, onboarding step two). The helper
  // parses the pasted invite, shows the relay's terms, claims membership and
  // lists open rooms; the panel only sends the text and the user's choices.
  property bool communityJoinSupported: false
  property var joinSetup: ({state: "idle", inviteCode: null, joinPolicy: null, claim: null, category: null})
  property string inviteState: "idle"
  property string inviteCategory: ""
  property string inviteRequestId: ""
  property string inviteRequestKind: ""
  property string inviteInstance: ""
  property string openRoomsState: "unavailable"
  property var openRooms: []
  property string openRoomsCategory: ""
  property var roomAction: ({state: "idle", action: null, requestId: null, roomId: null, category: null, detail: null})
  property string roomActionRequestId: ""
  property string roomActionLocal: "idle"
  property string roomActionCategory: ""
  // A room joined here is selected once the helper lists it.
  property string joinTarget: ""
  readonly property bool joinAvailable: communityJoinSupported && !sampleMode && !sessionFailed && instanceId !== ""
    && relay !== "" && identity !== "" && (connection === "authenticated" || connection === "disconnected")
  readonly property bool openRoomsAvailable: joinAvailable && connection === "authenticated"
    && ["partial", "ready"].indexOf(catalogState) !== -1
  readonly property bool inviteBusy: inviteState === "sending" || ["checking", "claiming"].indexOf(joinSetup.state) !== -1
  readonly property bool canRedeemInvite: joinAvailable && bridge.running && !inviteBusy
  readonly property bool policyShown: joinSetup.state === "policy" && joinSetup.joinPolicy !== null
  readonly property bool canAcceptInvite: joinAvailable && bridge.running && !inviteBusy && joinSetup.state === "policy"
  readonly property bool roomActionBusy: roomActionLocal === "sending" || roomAction.state === "sending"
  readonly property bool canLeaveRoom: openRoomsAvailable && !roomActionBusy && selectedRoom !== null && selectedRoom.kind === "stream"
  readonly property var inviteMessages: ({
    invite_invalid: "That invite was not recognized. Paste the whole link or code.",
    invite_relay_mismatch: "That invite is for another community. Use Join an existing community to add it.",
    invite_rejected: "The relay refused this invite. It may have expired or been used up.",
    invite_rate_limited: "Too many attempts. Wait a minute, then try again.",
    policy_required: "The community's terms changed. Redeem the invite again to read them.",
    relay_unavailable: "Could not reach the relay. Check the connection and try again.",
    setup_busy: "The helper is busy. Try again in a moment.",
    setup_not_allowed: "Choose a relay and an identity first."
  })
  readonly property string inviteLabel: {
    if (inviteState === "failed") return inviteMessages[inviteCategory] || "The invite was not redeemed. Try again."
    if (inviteState === "sending" && inviteRequestKind === "claim" || joinSetup.state === "checking") return "Checking invite…"
    if (inviteState === "sending" || joinSetup.state === "claiming") return "Joining…"
    if (joinSetup.state === "policy") return joinSetup.joinPolicy ? "Read the terms below, then accept to join." : ""
    if (joinSetup.state === "failed") return inviteMessages[joinSetup.category] || "The invite was not redeemed. Try again."
    if (joinSetup.state === "joined") {
      var claim = joinSetup.claim
      return claim.status === "already_member" ? "This identity is already a member of " + claim.host + "."
        : "Joined " + claim.host + " as " + claim.role + ". Open rooms are listed once connected."
    }
    return ""
  }
  readonly property var roomActionWords: ({
    join: ["Joining room…", "Joined. The room appears once the relay lists it."],
    leave: ["Leaving room…", "Left the room."],
    create: ["Creating room…", "Room created. It opens once the relay lists it."],
    details: ["Saving room details…", "Room details saved."],
    topic: ["Saving topic…", "Topic saved."],
    add_member: ["Adding member…", "Member added."],
    remove_member: ["Removing member…", "Member removed."]
  })
  readonly property var roomRefusalWords: ({
    join: "The relay refused to add you to this room.",
    leave: "The relay refused. If you are this room's only owner, make someone else an owner first.",
    create: "The relay refused to create the room.",
    details: "The relay refused to change the room details.",
    topic: "The relay refused to change the topic.",
    add_member: "The relay refused to add this member.",
    remove_member: "The relay refused to remove this member."
  })
  readonly property string roomActionLabel: {
    if (roomActionLocal === "failed") return ({room_not_open: "That room is no longer open to join. Refresh the list.",
      leave_rejected: "Only a joined room can be left here.", relay_unavailable: "Not connected. Try again when connected.",
      room_invalid: "That change is not valid for this room. Check the room and the text, then try again.",
      room_scope_changed: "Community changed, nothing was sent.",
      setup_busy: "Another room change is in progress."})[roomActionCategory] || "Nothing was sent. Try again."
    if (roomActionRequestId === "" || roomAction.requestId !== roomActionRequestId) return roomActionLocal === "sending" ? "Sending…" : ""
    var words = roomActionWords[roomAction.action]
    if (roomAction.state === "sending") return words[0]
    if (roomAction.state === "acknowledged") return words[1]
    // The relay's own words follow ours, as text, never interpreted.
    if (roomAction.state === "rejected") return roomRefusalWords[roomAction.action] + (roomAction.detail ? " Relay: " + roomAction.detail : "")
    if (roomAction.state === "unknown") return "No answer from the relay. Refresh to see whether it worked."
    return ""
  }
  readonly property string openRoomsLabel: openRoomsState === "loading" ? "Loading open rooms…"
    : openRoomsState === "snapshot" ? (openRooms.length ? "" : "No open rooms to join right now.")
    : openRoomsCategory ? "Open rooms could not be loaded. Try again." : ""

  // Room creation and management (`room_manage`). The helper signs kinds 9007
  // (create), 9002 (details, topic), 9000 and 9001 (members) with the session
  // key and reports the relay's answer; who may do what is the relay's rule.
  // The panel shows controls to owners and admins by the verified roster role,
  // and shows a refusal as the relay words it.
  property bool roomManageSupported: false
  property var roomDetail: roomDetailIdle
  readonly property bool roomManageAvailable: roomManageSupported && openRoomsAvailable && bridge.running
  readonly property bool canManageRooms: roomManageAvailable && !roomActionBusy
  readonly property bool canLoadMoreRooms: roomManageSupported && !sampleMode && !sessionFailed && connection === "authenticated"
    && instanceId !== "" && bridge.running && (catalogMore === "available" || catalogMore === "failed")
  readonly property string catalogMoreLabel: catalogMore === "loading" ? "Loading more rooms…"
    : catalogMore === "failed" ? (catalogMoreCategory === "room_catalog_timeout" ? "Loading more rooms timed out. Try again."
      : "Could not load more rooms. Try again.")
    : catalogMore === "limit" ? "Showing the first " + catalogRooms.length + " rooms."
    : ""
  // Roster and topic of the selected room, once read for it.
  readonly property bool roomDetailShown: roomDetail.state === "snapshot" && roomDetail.roomId === selectedRoomId && selectedRoomId !== ""
  readonly property string roomRole: roomDetailShown ? roomDetail.role : ""
  // The editor is shown to owners and admins (by the verified roster role); it
  // stays shown while a change is pending, read-only, and `canEditRoom` (which
  // also needs no change in flight) lets a save or member change be sent.
  readonly property bool roomEditAllowed: roomManageAvailable && roomDetailShown && selectedRoom !== null && selectedRoom.kind === "stream"
    && (roomRole === "owner" || roomRole === "admin")
  readonly property bool canEditRoom: roomEditAllowed && !roomActionBusy
  readonly property string roomDetailLabel: roomDetailShown ? ""
    : roomDetail.roomId === selectedRoomId && roomDetail.state === "loading" ? "Loading members and topic…"
    : roomDetail.roomId === selectedRoomId && roomDetail.category === "room_detail_access_denied" ? "The relay does not list you in this room."
    : roomDetail.roomId === selectedRoomId && roomDetail.category !== null ? "Members and topic could not be loaded. Try again."
    : ""
  function manageRequest(type, fields) {
    if (!canManageRooms) return false
    roomActionRequestId = correlationUuid()
    roomActionLocal = "sending"
    roomActionCategory = ""
    // Bound to the session this panel is showing, as sends are: the helper
    // refuses it (room_scope_changed) if the community changed meanwhile.
    var request = {version: 1, id: roomActionRequestId, type: type, generation: generation, instanceId: instanceId}
    Object.keys(fields).forEach(function(name) { request[name] = fields[name] })
    bridge.write(JSON.stringify(request) + "\n")
    roomActionTimeout.restart()
    return true
  }
  function validRoomText(text, limit, required) {
    return typeof text === "string" && utf8Size(text.trim()) <= limit && (!required || text.trim() !== "")
      && !/[\u0000-\u001f\u007f]/.test(text.trim())
  }
  function createRoom(name, about, priv) {
    if (!validRoomText(name, 128, true) || !validRoomText(about, roomAboutBytes, false)) return false
    var fields = {name: name.trim(), visibility: priv ? "private" : "open"}
    if (about.trim() !== "") fields.about = about.trim()
    return manageRequest("create_room", fields)
  }
  // The longest room description the helper sends or serves in full
  // (`rooms::ABOUT_BYTES`); the catalog row's copy is cut shorter for display.
  readonly property int roomAboutBytes: 1024
  // What a save would send: only fields the user touched (typed in) that also
  // differ from what the relay lists now. A field left alone is never sent, so
  // a rename or new text made elsewhere in the meantime is not reversed. The
  // description's baseline is the full one from the room detail, never the
  // catalog row's shortened copy; a description past the bound is not editable
  // here at all (it could only be saved back cut).
  function roomEditChanges(name, about, nameTouched, aboutTouched) {
    var changes = {}
    if (!canEditRoom || selectedRoom === null) return changes
    if (nameTouched && name.trim() !== selectedRoom.name) changes.name = name.trim()
    if (aboutTouched && !roomDetail.aboutTruncated && about.trim() !== roomDetail.about.trim()) changes.about = about.trim()
    return changes
  }
  function roomTopicChanged(topic, touched) {
    return canEditRoom && touched && topic.trim() !== roomDetail.topic.trim()
  }
  function updateRoomDetails(name, about, nameTouched, aboutTouched) {
    if (!validRoomText(name, 128, true) || !validRoomText(about, roomAboutBytes, false)) return false
    var changes = roomEditChanges(name, about, nameTouched, aboutTouched)
    if (Object.keys(changes).length === 0) return false
    changes.roomId = selectedRoomId
    return manageRequest("update_room", changes)
  }
  function setRoomTopic(topic, touched) {
    if (!roomTopicChanged(topic, touched) || !validRoomText(topic, 256, false)) return false
    return manageRequest("set_room_topic", {roomId: selectedRoomId, topic: topic.trim()})
  }
  function addRoomMember(key) {
    var value = typeof key === "string" ? key.trim().toLowerCase() : ""
    if (!canEditRoom || !/^[a-f0-9]{64}$/.test(value)) return false
    return manageRequest("add_room_member", {roomId: selectedRoomId, key: value})
  }
  // Only a member of the verified roster, never this identity (that is leaving).
  function removeRoomMember(key) {
    if (!canEditRoom || key === identity || !roomDetail.members.some(function(m) { return m.key === key })) return false
    return manageRequest("remove_room_member", {roomId: selectedRoomId, key: key})
  }
  // Forget the last change's outcome line (a view opens without yesterday's note).
  function resetRoomRequest() {
    if (roomActionBusy) return
    roomActionLocal = "idle"
    roomActionCategory = ""
    roomActionRequestId = ""
  }
  // People the relay's own catalog shows this identity talking to one-to-one:
  // verified keys with the DM's profile-name hint, never typed by anyone.
  readonly property var dmPeople: {
    var people = []
    dmRooms.forEach(function(room) {
      var others = (room.participants || []).filter(function(key) { return key !== identity })
      if (others.length === 1) people.push({key: others[0], name: room.name})
    })
    return people
  }
  function refreshRoomDetail() {
    if (!roomManageAvailable || selectedRoom === null || selectedRoom.kind !== "stream") return false
    send("fetch_room_detail", selectedRoomId)
    return true
  }
  function loadMoreRooms() {
    if (!canLoadMoreRooms) return false
    send("load_more_rooms")
    return true
  }
  // A room just created is selected once the catalog lists it; the relay may
  // take a moment, so the list is read again a few times meanwhile.
  property int createPolls: 0
  property string roomChangeNoted: ""
  function noteRoomChange(action) {
    // Once per accepted change, not once per status frame that repeats it.
    if (action.state !== "acknowledged" || action.requestId !== roomActionRequestId || action.requestId === roomChangeNoted) return
    roomChangeNoted = action.requestId
    if (action.action === "create") { joinTarget = action.roomId; createPolls = 4; createPoll.restart() }
    else if (["details", "topic", "add_member", "remove_member"].indexOf(action.action) !== -1) {
      // Mentions and author names read the recipients list, not the detail view.
      recheckRecipients = action.action === "add_member" || action.action === "remove_member"
      refreshRoomDetail()
      refreshRecipientsAfterChange()
      detailRecheck.restart()
    }
  }
  property bool recheckRecipients: false
  function refreshRecipientsAfterChange() {
    if (recheckRecipients && recipientsSupported && selectedRoomId !== "" && connection === "authenticated")
      send("fetch_recipients", selectedRoomId)
  }

  // Communities (`communities`): join, switch, rename and leave, like Buzz
  // Desktop's account menu. The helper parses what the user pasted, checks the
  // relay, claims invites with the one identity, saves the list and reconnects
  // with a new generation; the panel sends text and choices and shows the
  // helper's validated list. Names are local labels; `hint` is the relay's own
  // NIP-11 name, untrusted and shown as a hint only.
  signal communityRequestDone(string kind, bool ok)
  // A community this device now uses, after a join or switch the user asked
  // for: `joined` when the active community is new to the list, `switched` when
  // it was already listed. A join that first shows the community's terms
  // arrives once they are accepted; first setup (no identity yet) never does.
  signal communityArrived(string kind, string name)
  property var communityRelaysBefore: []
  property string arrivalAwaitingTerms: ""
  property bool communitiesSupported: false
  property var communities: ({state: "ready", active: null, entries: [], category: null, pendingInvite: false, notice: null})
  property string communityLocal: "idle"
  property string communityCategory: ""
  property string communityRequestId: ""
  property string communityRequestKind: ""
  property string communityRequestRelay: ""
  property string communityRequestInput: ""
  property string communityInstance: ""
  // First setup saved a community from an invite link: redeemed once the identity exists.
  property string pendingInviteInput: ""
  property string communityNotice: ""
  property var sampleCommunities: [
    {relay: "wss://sample.example/", name: "Sample", host: "sample.example", active: true, hint: null},
    {relay: "wss://second.example/", name: "Second team", host: "second.example", active: false, hint: "Second Team HQ"}
  ]
  readonly property var communityCategories: ["join_invalid", "join_rejected", "join_rate_limited", "join_busy", "join_last", "join_full", "policy_required",
    "relay_unavailable", "identity_unavailable", "config_unavailable", "community_unknown", "name_invalid", "leave_rejected", "leave_owner"]
  readonly property var communityEntries: sampleMode ? sampleCommunities : communitiesSupported ? communities.entries : []
  readonly property var activeCommunity: communityEntries.find(function(entry) { return entry.active }) || null
  readonly property var otherCommunities: communityEntries.filter(function(entry) { return !entry.active })
  readonly property bool communitiesShown: sampleMode || communitiesSupported
  readonly property bool communityBusy: communityLocal === "sending" || ["joining", "switching", "renaming", "leaving"].indexOf(communities.state) !== -1
  // Requests go to the helper only when it can take them (not while connecting).
  readonly property bool communitiesAvailable: communitiesSupported && !sampleMode && !sessionFailed && instanceId !== ""
    && connection !== "connecting" && category !== "identity_access_pending" && bridge.running
  readonly property bool canJoinCommunity: communitiesAvailable && !communityBusy
  readonly property bool canSwitchCommunity: (sampleMode || communitiesAvailable) && !communityBusy
  readonly property bool canLeaveCommunity: communitiesAvailable && !communityBusy && identity !== "" && communityEntries.length > 1
  readonly property var communityMessages: ({
    join_invalid: "Please enter a valid invite link or community URL",
    join_rejected: "Not a member yet. This relay requires an invitation. Ask a relay admin to add you as a member, then come back and try again.",
    join_rate_limited: "Too many attempts. Wait a moment, then try again.",
    join_busy: "Finish connecting the community already in progress, then try again.",
    join_last: "This is your only community. Join another one before you leave it.",
    join_full: "This device keeps up to 16 communities. Leave one first.",
    policy_required: "The community's terms changed. Join again to read them.",
    relay_unavailable: "Couldn't reach the community. Check the URL and your connection, then try again.",
    identity_unavailable: "Your Buzz identity is not available. Unlock your secret store, then try again.",
    config_unavailable: "The helper could not save its configuration. Check its configuration folder.",
    community_unknown: "That community is no longer on this device.",
    name_invalid: "Enter a name.",
    leave_rejected: "Couldn't leave the community. Try again.",
    leave_owner: "You own this community, so its relay does not let you leave it."
  })
  function communityMessage(kind, category) {
    // Desktop's leave wording for a relay that could not be reached.
    if (kind === "leave" && category === "relay_unavailable") return "Couldn't send the leave request. Check your connection and try again."
    return communityMessages[category] || "That did not work. Try again."
  }
  readonly property string communityLabel: {
    if (communityLocal === "failed") return communityMessage(communityRequestKind, communityCategory)
    if (communityLocal === "sending" || communityBusy) return ({join: "Joining…", "switch": "Switching…", rename: "Saving…", leave: "Leaving…"})[communityRequestKind] || "Working…"
    return ""
  }
  readonly property string communityNoticeLabel: communityNotice === "already_absent"
    ? "Community removed — You were no longer a member, so Buzz removed the community from this device." : ""
  function communityRelayValue(value) { return typeof value === "string" && value.length <= 2048 && /^wss?:\/\/[^\s@\/?#]+\/$/.test(value) }
  function beginCommunity(kind, relay, input) {
    communityRequestId = correlationUuid()
    communityRequestKind = kind
    communityRequestRelay = relay || ""
    communityRequestInput = input || ""
    communityInstance = instanceId
    communityRelaysBefore = communityEntries.map(function(entry) { return entry.relay })
    arrivalAwaitingTerms = ""
    communityLocal = "sending"
    communityCategory = ""
    communityNotice = ""
    communityTimeout.restart()
  }
  function loseCommunity() {
    communityTimeout.stop()
    if (communityLocal === "sending") { communityLocal = "idle"; communityCategory = "" }
    communityRequestId = ""
  }
  function resetCommunityRequest() {
    if (communityLocal === "sending") return
    communityLocal = "idle"
    communityCategory = ""
    communityNotice = ""
  }
  function joinCommunity(text) {
    var value = typeof text === "string" ? text.trim() : ""
    if (!value) return false
    if (!canJoinCommunity) {
      // Helpers without `communities` keep the relay-only first setup.
      if (!communitiesSupported && setupAssistAvailable && /^wss?:\/\/[^\s@]+$/.test(value)) return setupRelay(value)
      return false
    }
    if (value.length > 4096 || value.indexOf("\u0000") !== -1) { communityLocal = "failed"; communityRequestKind = "join"; communityCategory = "join_invalid"; return false }
    beginCommunity("join", "", value)
    bridge.write(JSON.stringify({version: 1, id: communityRequestId, type: "join_community", input: value, generation: generation, instanceId: instanceId}) + "\n")
    return true
  }
  function switchCommunity(relay) {
    if (sampleMode) {
      // Presentation only: sample data has no relay to switch to.
      if (!sampleCommunities.some(function(entry) { return entry.relay === relay })) return false
      sampleCommunities = sampleCommunities.map(function(entry) { return Object.assign({}, entry, {active: entry.relay === relay}) })
      return true
    }
    if (!canSwitchCommunity || !communityRelayValue(relay) || (activeCommunity && activeCommunity.relay === relay)
        || !communityEntries.some(function(entry) { return entry.relay === relay })) return false
    beginCommunity("switch", relay, "")
    bridge.write(JSON.stringify({version: 1, id: communityRequestId, type: "switch_community", relay: relay, generation: generation, instanceId: instanceId}) + "\n")
    return true
  }
  function renameCommunity(relay, name) {
    var value = typeof name === "string" ? name.trim() : ""
    if (!communitiesAvailable || communityBusy || !communityRelayValue(relay) || !communityEntries.some(function(entry) { return entry.relay === relay })) return false
    if (!value || utf8Size(value) > 1024 || value.indexOf("\u0000") !== -1) { communityLocal = "failed"; communityRequestKind = "rename"; communityCategory = "name_invalid"; return false }
    beginCommunity("rename", relay, "")
    bridge.write(JSON.stringify({version: 1, id: communityRequestId, type: "rename_community", relay: relay, name: value}) + "\n")
    return true
  }
  function leaveCommunity(relay) {
    if (!canLeaveCommunity || !communityRelayValue(relay) || !communityEntries.some(function(entry) { return entry.relay === relay })) return false
    beginCommunity("leave", relay, "")
    bridge.write(JSON.stringify({version: 1, id: communityRequestId, type: "leave_community", relay: relay, generation: generation, instanceId: instanceId}) + "\n")
    return true
  }
  function clearCommunities() {
    communities = {state: "ready", active: null, entries: [], category: null, pendingInvite: false, notice: null}
  }
  // `status.communities`: exactly these keys; the active entry is the frame's relay.
  function validatedCommunities(view, relay) {
    if (!view || typeof view !== "object" || Array.isArray(view)
        || Object.keys(view).sort().join(",") !== "active,category,entries,notice,pendingInvite,state"
        || ["ready", "joining", "switching", "renaming", "leaving", "failed"].indexOf(view.state) === -1
        || (view.state === "failed") !== (view.category !== null)
        || (view.category !== null && communityCategories.indexOf(view.category) === -1)
        || typeof view.pendingInvite !== "boolean" || (view.notice !== null && view.notice !== "already_absent")
        || view.active !== relay || (view.active !== null && !communityRelayValue(view.active))
        || !Array.isArray(view.entries) || view.entries.length > 16 || (view.active === null) !== (view.entries.length === 0)) return null
    var clean = []
    var seen = ({})
    var actives = 0
    for (var i = 0; i < view.entries.length; i++) {
      var entry = view.entries[i]
      if (!entry || typeof entry !== "object" || Array.isArray(entry)
          || Object.keys(entry).sort().join(",") !== "active,hint,host,name,relay"
          || !communityRelayValue(entry.relay) || seen[entry.relay]
          || !boundedString(entry.name, 64) || !entry.name.trim() || utf8Size(entry.name) > 64
          || !boundedString(entry.host, 260) || !/^[A-Za-z0-9.:\[\]-]+$/.test(entry.host)
          || typeof entry.active !== "boolean" || entry.active !== (entry.relay === view.active)
          || (entry.hint !== null && (!boundedString(entry.hint, 64) || !entry.hint.trim() || utf8Size(entry.hint) > 64))) return null
      seen[entry.relay] = true
      if (entry.active) actives++
      clean.push({relay: entry.relay, name: entry.name, host: entry.host, active: entry.active, hint: entry.hint})
    }
    if (view.active !== null && actives !== 1) return null
    return {state: view.state, active: view.active, entries: clean, category: view.category, pendingInvite: view.pendingInvite, notice: view.notice}
  }
  function inviteCodeValue(value) { return typeof value === "string" && /^[A-Za-z0-9._-]{1,1024}$/.test(value) }
  function redeemInvite(text) {
    if (!canRedeemInvite) return false
    var value = typeof text === "string" ? text.trim() : ""
    if (!value || value.length > 4096 || value.indexOf("\u0000") !== -1) { inviteState = "failed"; inviteCategory = "invite_invalid"; return false }
    beginInvite("claim")
    bridge.write(JSON.stringify({version: 1, id: inviteRequestId, type: "claim_invite", input: value}) + "\n")
    return true
  }
  function acceptInvite() {
    if (!canAcceptInvite || !inviteCodeValue(joinSetup.inviteCode)) return false
    beginInvite("accept")
    bridge.write(JSON.stringify({version: 1, id: inviteRequestId, type: "accept_invite", code: joinSetup.inviteCode,
      policyVersion: joinSetup.joinPolicy ? joinSetup.joinPolicy.version : null}) + "\n")
    return true
  }
  function beginInvite(kind) {
    inviteRequestId = correlationUuid()
    inviteRequestKind = kind
    inviteInstance = instanceId
    inviteState = "sending"
    inviteCategory = ""
    inviteTimeout.restart()
  }
  function loseInvite() {
    inviteTimeout.stop()
    if (inviteState === "sending") { inviteState = "idle"; inviteCategory = "" }
    inviteRequestId = ""
  }
  function refreshOpenRooms() {
    if (!openRoomsAvailable) return false
    send("open_rooms")
    return true
  }
  function roomActionRequest(kind, roomId) {
    if (!openRoomsAvailable || !bridge.running || roomActionBusy || !uuidValue(roomId)) return false
    roomActionRequestId = correlationUuid()
    roomActionLocal = "sending"
    roomActionCategory = ""
    if (kind === "join_room") joinTarget = roomId
    bridge.write(JSON.stringify({version: 1, id: roomActionRequestId, type: kind, roomId: roomId}) + "\n")
    roomActionTimeout.restart()
    return true
  }
  function joinRoom(roomId) {
    if (!openRooms.some(function(room) { return room.id === roomId })) return false
    return roomActionRequest("join_room", roomId)
  }
  function leaveRoom(roomId) {
    if (!canLeaveRoom || selectedRoomId !== roomId) return false
    return roomActionRequest("leave_room", roomId)
  }
  function loseRoomAction() {
    roomActionTimeout.stop()
    if (roomActionLocal === "sending") roomActionLocal = "idle"
    joinTarget = ""
  }
  function validatedJoinSetup(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)
        || Object.keys(value).sort().join(",") !== "category,claim,inviteCode,joinPolicy,state"
        || ["idle", "checking", "policy", "claiming", "joined", "failed"].indexOf(value.state) === -1) return null
    var policy = value.joinPolicy
    if (value.state === "policy") {
      if (!inviteCodeValue(value.inviteCode) || value.claim !== null || value.category !== null) return null
      if (policy !== null && (typeof policy !== "object" || Array.isArray(policy)
          || Object.keys(policy).sort().join(",") !== "ageRequired,text,truncated,version"
          || !boundedString(policy.text, 65536) || utf8Size(policy.text) > 65536
          || /[\u0000-\u0008\u000b-\u001f\u007f]/.test(policy.text)
          || typeof policy.version !== "string" || policy.version.length < 1 || policy.version.length > 128
          || /[\u0000-\u001f\u007f]/.test(policy.version)
          || typeof policy.ageRequired !== "boolean" || typeof policy.truncated !== "boolean")) return null
      return {state: "policy", inviteCode: value.inviteCode, claim: null, category: null,
        joinPolicy: policy === null ? null : {text: policy.text, version: policy.version, ageRequired: policy.ageRequired, truncated: policy.truncated}}
    }
    if (value.inviteCode !== null || policy !== null) return null
    var claim = value.claim
    if (value.state === "joined") {
      if (value.category !== null || !claim || typeof claim !== "object" || Array.isArray(claim)
          || Object.keys(claim).sort().join(",") !== "communityId,host,role,status"
          || ["joined", "already_member"].indexOf(claim.status) === -1 || !uuidValue(claim.communityId)
          || typeof claim.host !== "string" || !/^[A-Za-z0-9.:\[\]-]{1,255}$/.test(claim.host)
          || typeof claim.role !== "string" || !/^[a-z_]{1,32}$/.test(claim.role)) return null
      return {state: "joined", inviteCode: null, joinPolicy: null, category: null,
        claim: {status: claim.status, communityId: claim.communityId, host: claim.host, role: claim.role}}
    }
    if (claim !== null) return null
    if (value.state === "failed" ? Object.keys(inviteMessages).indexOf(value.category) === -1 : value.category !== null) return null
    return {state: value.state, inviteCode: null, joinPolicy: null, claim: null, category: value.category}
  }
  function validatedOpenRooms(value, catalog) {
    if (!value || typeof value !== "object" || Array.isArray(value)
        || Object.keys(value).sort().join(",") !== "category,rooms,state"
        || ["unavailable", "loading", "snapshot"].indexOf(value.state) === -1
        || !Array.isArray(value.rooms) || value.rooms.length > 50
        || (value.category !== null && value.category !== "relay_unavailable")
        || (value.state !== "snapshot" && value.rooms.length !== 0)) return null
    var clean = []
    var ids = ({})
    for (var i = 0; i < value.rooms.length; i++) {
      var room = value.rooms[i]
      if (!room || typeof room !== "object" || Array.isArray(room)
          || Object.keys(room).sort().join(",") !== "description,id,kind,name"
          || !uuidValue(room.id) || ids[room.id] || room.kind !== "stream"
          || !boundedString(room.name, 128) || !room.name.trim() || !boundedString(room.description, 512)
          || catalog.rooms.some(function(joined) { return joined.id === room.id })) return null
      ids[room.id] = true
      clean.push({id: room.id, name: room.name, description: room.description, kind: "stream"})
    }
    return {state: value.state, rooms: clean, category: value.category || ""}
  }
  readonly property var roomRefusals: ({join: "join_rejected", leave: "leave_rejected", create: "create_rejected", details: "edit_rejected",
    topic: "edit_rejected", add_member: "member_add_rejected", remove_member: "member_remove_rejected"})
  function validatedRoomAction(value) {
    // `detail` (the relay's refusal text) is absent from helpers without `room_manage`.
    var keys = Object.keys(value || {}).sort().join(",")
    if (!value || typeof value !== "object" || Array.isArray(value)
        || (keys !== "action,category,requestId,roomId,state" && keys !== "action,category,detail,requestId,roomId,state")
        || ["idle", "sending", "acknowledged", "rejected", "unknown"].indexOf(value.state) === -1) return null
    var detail = value.detail === undefined ? null : value.detail
    if (detail !== null && (value.state !== "rejected" || !boundedString(detail, 200) || utf8Size(detail) > 200 || !detail.trim()
        || /[\u0000-\u001f\u007f]/.test(detail))) return null
    if (value.state === "idle")
      return value.action === null && value.requestId === null && value.roomId === null && value.category === null && detail === null
        ? {state: "idle", action: null, requestId: null, roomId: null, category: null, detail: null} : null
    if (Object.keys(roomRefusals).indexOf(value.action) === -1 || !uuidValue(value.requestId) || !uuidValue(value.roomId)) return null
    var expected = ({sending: null, acknowledged: null, rejected: roomRefusals[value.action], unknown: "relay_unavailable"})[value.state]
    if (value.category !== expected) return null
    return {state: value.state, action: value.action, requestId: value.requestId, roomId: value.roomId, category: value.category, detail: detail}
  }
  readonly property var memberRoles: ["owner", "admin", "member", "guest", "bot", "unknown"]
  readonly property var roomDetailIdle: ({state: "unavailable", roomId: null, topic: "", visibility: "", about: "", aboutTruncated: false, role: "", members: [], truncated: false, category: null})
  // The verified roster, roles and topic of a joined stream room. Names are
  // profile hints; roles are the relay's, shown as said.
  function validatedRoomDetail(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)
        || Object.keys(value).sort().join(",") !== "about,aboutTruncated,category,members,role,roomId,state,topic,truncated,visibility"
        || ["unavailable", "loading", "snapshot"].indexOf(value.state) === -1
        || (value.roomId !== null && !uuidValue(value.roomId))
        || (value.category !== null && ["room_detail_unavailable", "room_detail_invalid", "room_detail_access_denied"].indexOf(value.category) === -1)
        || !boundedString(value.topic, 1024) || utf8Size(value.topic) > 256 || /[\u0000-\u001f\u007f]/.test(value.topic)
        || !boundedString(value.about, 4096) || utf8Size(value.about) > roomAboutBytes || /[\u0000-\u001f\u007f]/.test(value.about)
        || typeof value.aboutTruncated !== "boolean"
        || typeof value.truncated !== "boolean" || !Array.isArray(value.members) || value.members.length > 100) return null
    if (value.state !== "snapshot") {
      return value.members.length === 0 && value.topic === "" && value.visibility === "" && value.role === "" && !value.truncated
        && value.about === "" && !value.aboutTruncated
        && (value.state !== "loading" || value.roomId !== null) ? {state: value.state, roomId: value.roomId, topic: "", visibility: "", about: "",
          aboutTruncated: false, role: "", members: [], truncated: false, category: value.category} : null
    }
    if (value.roomId === null || value.category !== null || ["open", "private"].indexOf(value.visibility) === -1
        || memberRoles.indexOf(value.role) === -1) return null
    var seen = ({})
    var clean = []
    for (var i = 0; i < value.members.length; i++) {
      var member = value.members[i]
      if (!member || typeof member !== "object" || Array.isArray(member) || Object.keys(member).sort().join(",") !== "key,name,role"
          || typeof member.key !== "string" || !/^[a-f0-9]{64}$/.test(member.key) || seen[member.key]
          || !boundedString(member.name, 256) || utf8Size(member.name) > 64 || /[\u0000-\u001f\u007f]/.test(member.name)
          || memberRoles.indexOf(member.role) === -1) return null
      seen[member.key] = true
      clean.push({key: member.key, name: member.name, role: member.role})
    }
    return {state: "snapshot", roomId: value.roomId, topic: value.topic, visibility: value.visibility, about: value.about, aboutTruncated: value.aboutTruncated, role: value.role,
      members: clean, truncated: value.truncated, category: null}
  }
  function selectJoinedRoom() {
    if (joinTarget === "" || !streamRooms.some(function(room) { return room.id === root.joinTarget })) return
    var target = joinTarget
    joinTarget = ""
    selectRoom(target)
  }

  // Inviting people (`invite_mint`, Settings → Invite people). The relay lets
  // only its owner or admins mint; the helper signs the request and publishes
  // the code, and the panel turns it into the two links Buzz Desktop accepts
  // and a message for newcomers. The code reaches only the clipboard, on Copy.
  readonly property string omarchyBuzzUrl: "https://github.com/randymy/omarchy-buzz"
  property bool inviteMintSupported: false
  property var invites: ({state: "idle", code: null, expiresAt: null, maxUses: null, role: null, category: null})
  property string mintState: "idle"
  property string mintCategory: ""
  property string mintRequestId: ""
  property string mintInstance: ""
  property string inviteCopied: ""
  readonly property bool inviteMintAvailable: inviteMintSupported && !sampleMode && !sessionFailed && instanceId !== "" && connection === "authenticated"
  readonly property bool canMintInvite: inviteMintAvailable && bridge.running && mintState !== "sending" && invites.state !== "minting"
  readonly property var mintMessages: ({
    invite_forbidden: "Only the relay's owner or admins can create invites.",
    invite_rejected: "The relay refused to create this invite. Try again.",
    invite_rate_limited: "Too many attempts. Wait a minute, then try again.",
    relay_unavailable: "Could not reach the relay. Check the connection and try again.",
    setup_busy: "The helper is busy. Try again in a moment."
  })
  readonly property string mintLabel: {
    if (mintState === "failed") return mintMessages[mintCategory] || "No invite was created. Try again."
    if (mintState === "sending" || invites.state === "minting") return "Creating invite…"
    if (invites.state === "failed") return mintMessages[invites.category] || "No invite was created. Try again."
    return ""
  }
  // `wss://host[:port]` without the trailing slash, as Buzz's invite page links it.
  readonly property string inviteRelay: /^wss?:\/\/[^\/\s@?#]+\/?$/.test(relay) ? relay.replace(/\/$/, "") : ""
  readonly property string inviteHost: inviteRelay.replace(/^wss?:\/\//, "")
  readonly property bool inviteShown: invites.state === "minted" && inviteRelay !== "" && inviteMintAvailable
  readonly property string inviteAppLink: inviteShown
    ? "buzz://join?relay=" + encodeURIComponent(inviteRelay) + "&code=" + encodeURIComponent(invites.code) : ""
  readonly property string inviteWebLink: inviteShown
    ? (inviteRelay.indexOf("wss://") === 0 ? "https://" : "http://") + inviteHost + "/invite/" + invites.code : ""
  readonly property string inviteBlurb: inviteShown
    ? "Join me on Buzz at " + inviteHost + ": chat for people and their AI agents. On Omarchy, ask your coding agent to install "
      + omarchyBuzzUrl + ", then open the Buzz panel, choose the relay " + inviteRelay
      + ", create an identity and paste the invite below. Anywhere else, use Buzz Desktop and paste the same invite.\n\n" + inviteWebLink
    : ""
  readonly property string inviteDetails: inviteShown
    ? (invites.maxUses === 1 ? "One use" : invites.maxUses + " uses") + " · expires "
      + Qt.formatDateTime(new Date(invites.expiresAt * 1000), "d MMM yyyy, hh:mm") + " · joins as " + invites.role : ""
  function mintInvite(maxUses, hours) {
    if (!canMintInvite || [1, 5, 25].indexOf(maxUses) === -1 || [24, 168, 720].indexOf(hours) === -1) return false
    mintRequestId = correlationUuid()
    mintInstance = instanceId
    mintState = "sending"
    mintCategory = ""
    inviteCopied = ""
    bridge.write(JSON.stringify({version: 1, id: mintRequestId, type: "mint_invite", maxUses: maxUses, expiresInHours: hours}) + "\n")
    mintTimeout.restart()
    return true
  }
  function copyInvite(kind) {
    var text = ({app: inviteAppLink, web: inviteWebLink, blurb: inviteBlurb})[kind]
    if (!text) return false
    Quickshell.clipboardText = text
    inviteCopied = kind
    return true
  }
  function loseMint() {
    mintTimeout.stop()
    if (mintState === "sending") { mintState = "idle"; mintCategory = "" }
    mintRequestId = ""
  }
  function clearInvites() {
    invites = {state: "idle", code: null, expiresAt: null, maxUses: null, role: null, category: null}
    inviteCopied = ""
  }
  function validatedInvites(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)
        || Object.keys(value).sort().join(",") !== "category,code,expiresAt,maxUses,role,state"
        || ["idle", "minting", "minted", "failed"].indexOf(value.state) === -1) return null
    if (value.state === "minted") {
      if (typeof value.code !== "string" || !/^v2\.[A-Za-z0-9_-]{43}$/.test(value.code)
          || !Number.isInteger(value.expiresAt) || value.expiresAt < 1 || value.expiresAt > 4102444800
          || !Number.isInteger(value.maxUses) || value.maxUses < 1 || value.maxUses > 100
          || value.role !== "member" || value.category !== null) return null
      return {state: "minted", code: value.code, expiresAt: value.expiresAt, maxUses: value.maxUses, role: "member", category: null}
    }
    if (value.code !== null || value.expiresAt !== null || value.maxUses !== null || value.role !== null) return null
    if (value.state === "failed" ? Object.keys(mintMessages).indexOf(value.category) === -1 : value.category !== null) return null
    return {state: value.state, code: null, expiresAt: null, maxUses: null, role: null, category: value.category}
  }

  // "Update your status" (`user_status`, account menu). The helper signs and
  // publishes the kind 30315 status and verifies every status it reads; the
  // panel sends only text, an emoji and hours, and shows validated projections.
  property bool userStatusSupported: false
  property var userStatus: ({state: "unavailable", mine: null, category: null})
  property string statusRequestState: "idle"
  property string statusRequestCategory: ""
  property string statusRequestId: ""
  property string statusRequestInstance: ""
  // A helper failure is shown for a request made since the view was opened.
  property bool statusOutcomeShown: false
  // A small fixed set of common statuses, as Buzz Desktop offers presets.
  readonly property var statusPresetEmoji: ["💬", "🗓️", "🍽️", "🚌", "🤒", "🌴", "🏠", "🎧", "🧑‍💻", "☕", "🎉", "👀"]
  // 1 hour, 4 hours, 1 day, 1 week.
  readonly property var statusDurations: [1, 4, 24, 168]
  readonly property int statusTextBytes: 200
  readonly property bool userStatusAvailable: userStatusSupported && !sampleMode && !sessionFailed && instanceId !== "" && connection === "authenticated"
  readonly property bool canSetStatus: userStatusAvailable && bridge.running && statusRequestState !== "sending" && userStatus.state !== "sending"
  // Sample mode shows a fixture status; nothing is ever published from it.
  readonly property var sampleStatuses: ({me: {text: "Exploring the sample", emoji: "🧭", expiresAt: null}, Alex: {text: "In a meeting", emoji: "🗓️"}})
  readonly property var myStatus: sampleMode ? sampleStatuses.me : userStatusAvailable ? userStatus.mine : null
  readonly property var userStatusMessages: ({
    status_invalid: "Check the status: up to 200 characters and one emoji.",
    status_rate_limited: "Status changed a moment ago. Wait a few seconds, then try again.",
    status_rejected: "The relay refused this status. Try again.",
    relay_unavailable: "Could not reach the relay. The status may not have changed."
  })
  readonly property string userStatusLabel: {
    if (statusRequestState === "failed") return userStatusMessages[statusRequestCategory] || "The status was not changed. Try again."
    if (statusRequestState === "sending" || userStatus.state === "sending") return "Updating status…"
    if (userStatus.state === "failed" && statusOutcomeShown) return userStatusMessages[userStatus.category] || "The status was not changed. Try again."
    return ""
  }
  // A native emoji sequence (1-8 code points, no ASCII, controls, whitespace,
  // bidi or invisible characters, not starting with a joiner or modifier) or a
  // `:shortcode:`. The helper applies the same rule to what it sends and reads.
  function validStatusEmoji(value) {
    if (typeof value !== "string" || value.length > 64) return false
    if (/^:[A-Za-z0-9_+-]{1,32}:$/.test(value)) return true
    var points = []
    for (var i = 0; i < value.length; i++) {
      var point = value.codePointAt(i)
      if (point > 0xffff) i++
      points.push(point)
    }
    if (points.length < 1 || points.length > 8) return false
    var first = points[0]
    if (first === 0x200d || (first >= 0xfe00 && first <= 0xfe0f) || first === 0x20e3 || (first >= 0x1f3fb && first <= 0x1f3ff)
        || (first >= 0xe0020 && first <= 0xe007f) || (first >= 0x300 && first <= 0x36f)) return false
    return points.every(function(p) {
      return p >= 0x80 && !(p >= 0x80 && p <= 0x9f) && !/\s/.test(String.fromCodePoint(p))
        && [0x61c, 0x200e, 0x200f, 0x200b, 0x200c, 0xfeff].indexOf(p) === -1 && !(p >= 0x202a && p <= 0x202e)
        && !(p >= 0x2066 && p <= 0x2069) && !(p >= 0x2060 && p <= 0x2064) && !(p >= 0xe000 && p <= 0xf8ff) && p < 0xf0000
    })
  }
  // Status text as the helper projects it: at most 200 bytes, no controls or bidi.
  function validStatusText(value) {
    return boundedString(value, 200) && utf8Size(value) <= 200
      && !/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069\u200e\u200f\u061c]/.test(value)
  }
  // `{text, emoji}` beside another member's name, or null.
  function validatedRecipientStatus(value) {
    if (value === null) return null
    if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).sort().join(",") !== "emoji,text"
        || !validStatusText(value.text) || (value.emoji !== null && !validStatusEmoji(value.emoji))
        || (value.text.trim() === "" && value.emoji === null)) return undefined
    return {text: value.text, emoji: value.emoji}
  }
  function validatedUserStatus(value) {
    if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).sort().join(",") !== "category,mine,state"
        || ["unavailable", "ready", "sending", "failed"].indexOf(value.state) === -1) return null
    if (value.state === "failed" ? Object.keys(userStatusMessages).indexOf(value.category) === -1
        : value.category !== null && (value.state !== "unavailable" || Object.keys(userStatusMessages).indexOf(value.category) === -1)) return null
    var mine = null
    if (value.mine !== null) {
      var m = value.mine
      if (!m || typeof m !== "object" || Array.isArray(m) || Object.keys(m).sort().join(",") !== "emoji,expiresAt,text"
          || value.state === "unavailable" || !validStatusText(m.text) || (m.emoji !== null && !validStatusEmoji(m.emoji))
          || (m.text.trim() === "" && m.emoji === null)
          || (m.expiresAt !== null && (!Number.isInteger(m.expiresAt) || m.expiresAt < 1 || m.expiresAt > 4102444800))) return null
      mine = {text: m.text, emoji: m.emoji, expiresAt: m.expiresAt}
    }
    return {state: value.state, mine: mine, category: value.category}
  }
  // A member's status for a name shown from the verified roster (or the sample).
  function authorStatus(key) {
    if (sampleMode) return sampleStatuses[key] || null
    if (!userStatusSupported || recipientsState !== "snapshot" || recipientsRoomId !== selectedRoomId || selectedRoomId === "") return null
    if (key === identity && userStatus.state !== "unavailable") return userStatus.mine ? {text: userStatus.mine.text, emoji: userStatus.mine.emoji} : null
    var entry = recipientEntries.find(function(item) { return item.key === key })
    return entry && entry.status ? entry.status : null
  }
  function statusEmojiText(status) { return status ? (status.emoji || "💬") : "" }
  function setStatus(text, emoji, hours) {
    if (!canSetStatus || typeof text !== "string" || statusDurations.indexOf(hours) === -1) return false
    var trimmed = text.trim()
    var chosen = typeof emoji === "string" ? emoji.trim() : ""
    if (utf8Size(trimmed) > statusTextBytes || /[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069\u200e\u200f\u061c]/.test(trimmed)
        || (chosen !== "" && !validStatusEmoji(chosen)) || (trimmed === "" && chosen === "")) return false
    var request = {version: 1, id: correlationUuid(), type: "set_status", text: trimmed, expiresInHours: hours}
    if (chosen !== "") request.emoji = chosen
    return writeStatusRequest(request)
  }
  function clearStatus() {
    if (!canSetStatus) return false
    return writeStatusRequest({version: 1, id: correlationUuid(), type: "clear_status"})
  }
  function writeStatusRequest(request) {
    statusRequestId = request.id
    statusRequestInstance = instanceId
    statusRequestState = "sending"
    statusRequestCategory = ""
    statusOutcomeShown = true
    bridge.write(JSON.stringify(request) + "\n")
    statusTimeout.restart()
    return true
  }
  // A view opened afresh shows no earlier outcome.
  function resetStatusRequest() {
    if (statusRequestState !== "sending") { statusRequestState = "idle"; statusRequestCategory = "" }
    statusOutcomeShown = false
  }
  function loseStatusRequest() {
    statusTimeout.stop()
    if (statusRequestState === "sending") { statusRequestState = "idle"; statusRequestCategory = "" }
    statusRequestId = ""
  }
  function clearUserStatus() {
    loseStatusRequest()
    statusRequestState = "idle"
    statusRequestCategory = ""
    userStatus = {state: "unavailable", mine: null, category: null}
  }

  // Presence (`presence`, account menu "Set yourself as…"). The helper derives,
  // signs and publishes the kind 20001 heartbeat and verifies every state it
  // reads; the panel sends only the person's preference and an idle hint, and
  // shows validated projections.
  property bool presenceSupported: false
  property var presence: ({state: "unavailable", mode: null, published: null, lastPublishedAt: null, category: null, peers: []})
  readonly property var presenceModes: ["auto", "away", "offline"]
  readonly property var presenceModeLabels: ({auto: "Auto", away: "Away", offline: "Appear offline"})
  readonly property var presenceLabels: ({online: "Online", away: "Away", offline: "Offline"})
  readonly property var presenceColors: ({online: "#3fb950", away: "#d29922", offline: "#8b949e"})
  // The person's preference, kept across shell restarts (presence.json).
  property string presenceMode: "auto"
  readonly property string presenceSettingsPath: notificationSettingsDir + "/presence.json"
  // Idle hint: Wayland ext-idle-notify through Quickshell's IdleMonitor (10
  // minutes, Desktop's PRESENCE_IDLE_TIMEOUT_MS). Without it (offscreen, or a
  // compositor without the protocol) the panel counts as active while open.
  readonly property int presenceIdleSeconds: 600
  readonly property bool idleMonitorWanted: !sampleMode && Qt.platform.pluginName === "wayland"
  readonly property bool idleMonitorWorking: idleMonitor.enabled
  readonly property bool presenceActive: idleMonitorWorking ? !idleMonitor.isIdle : panelOpen
  readonly property bool presenceAvailable: presenceSupported && !sampleMode && !sessionFailed && instanceId !== "" && connection === "authenticated"
  // The `mode|active` last sent in this session; a repeat is never sent.
  property string presenceSentKey: ""
  property string presenceRequestId: ""
  // Sample mode shows fixture states and publishes nothing.
  readonly property var samplePresence: ({Alex: "online", "Research agent": "away", "Code agent": "offline"})
  function derivedPresence(mode, active) { return mode === "offline" ? "offline" : mode === "away" || !active ? "away" : "online" }
  // This identity's own state: what the relay accepted (or the sample's).
  readonly property string myPresence: sampleMode ? derivedPresence(presenceMode, presenceActive)
    : presenceAvailable && presence.published ? presence.published : ""
  // A key's verified state, or "" when unknown.
  function presenceOf(key) {
    if (sampleMode) return samplePresence[key] || ""
    if (!presenceAvailable || typeof key !== "string") return ""
    if (key === identity) return myPresence
    var entry = recipientEntries.find(function(item) { return item.key === key })
    if (entry && entry.presence) return entry.presence
    var peer = presence.peers.find(function(item) { return item.key === key })
    return peer ? peer.presence : ""
  }
  function presenceLabel(state) { return presenceLabels[state] || "" }
  function validPresenceState(value) { return ["online", "away", "offline"].indexOf(value) !== -1 }
  function validatedPresence(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)
        || Object.keys(value).sort().join(",") !== "category,lastPublishedAt,mode,peers,published,state"
        || ["unavailable", "ready", "failed"].indexOf(value.state) === -1
        || (value.mode !== null && presenceModes.indexOf(value.mode) === -1)
        || (value.published !== null && !validPresenceState(value.published))
        || (value.lastPublishedAt !== null && (!Number.isInteger(value.lastPublishedAt) || value.lastPublishedAt < 1 || value.lastPublishedAt > 4102444800))
        || ((value.published === null) !== (value.lastPublishedAt === null))
        || (value.state === "failed" ? ["presence_rejected", "relay_unavailable"].indexOf(value.category) === -1 : value.category !== null)
        || (value.state === "ready" && value.published === null)
        || !Array.isArray(value.peers) || value.peers.length > 60) return null
    var peers = []
    var seen = ({})
    for (var i = 0; i < value.peers.length; i++) {
      var peer = value.peers[i]
      if (!peer || typeof peer !== "object" || Array.isArray(peer) || Object.keys(peer).sort().join(",") !== "key,presence"
          || typeof peer.key !== "string" || !/^[a-f0-9]{64}$/.test(peer.key) || seen[peer.key] || peer.key === identity
          || !validPresenceState(peer.presence)) return null
      seen[peer.key] = true
      peers.push({key: peer.key, presence: peer.presence})
    }
    return {state: value.state, mode: value.mode, published: value.published, lastPublishedAt: value.lastPublishedAt,
      category: value.category, peers: peers}
  }
  // Sends the preference and idle hint when either changed (or on connect).
  function syncPresence() {
    if (!presenceAvailable || !bridge.running) { if (!presenceAvailable) presenceSentKey = ""; return false }
    var key = presenceMode + "|" + presenceActive
    if (key === presenceSentKey) return false
    var request = {version: 1, id: correlationUuid(), type: "set_presence", mode: presenceMode, active: presenceActive}
    presenceSentKey = key
    presenceRequestId = request.id
    bridge.write(JSON.stringify(request) + "\n")
    return true
  }
  function setPresenceMode(mode) {
    if (presenceModes.indexOf(mode) === -1 || (!presenceSupported && !sampleMode)) return false
    if (mode === presenceMode) return true
    presenceMode = mode
    if (!sampleMode && notificationSettingsDirReady) presenceSettingsFile.setText(JSON.stringify({version: 1, mode: mode}) + "\n")
    return true
  }
  function loadPresenceSettings(raw) {
    try {
      var parsed = JSON.parse(raw)
      if (parsed && parsed.version === 1 && presenceModes.indexOf(parsed.mode) !== -1) presenceMode = parsed.mode
    } catch (error) { /* Missing or malformed settings mean Auto. */ }
  }
  function clearPresence() {
    presenceSentKey = ""
    presenceRequestId = ""
    presenceRetry.stop()
    presence = {state: "unavailable", mode: null, published: null, lastPublishedAt: null, category: null, peers: []}
  }
  onPresenceModeChanged: syncPresence()
  onPresenceActiveChanged: syncPresence()
  IdleMonitor {
    id: idleMonitor
    enabled: root.idleMonitorWanted
    timeout: root.presenceIdleSeconds
    respectInhibitors: true
  }
  FileView {
    id: presenceSettingsFile
    path: root.sampleMode ? "" : root.presenceSettingsPath
    watchChanges: false
    atomicWrites: true
    printErrors: false
    blockLoading: true
    onLoaded: root.loadPresenceSettings(text())
  }
  Timer {
    id: presenceRetry
    // A refused or lost `set_presence` is sent again a little later.
    interval: 5000
    onTriggered: root.syncPresence()
  }

  // File attachments (`attachments`). The helper downloads, verifies and saves
  // files, uploads the path the user types and signs every request; the panel
  // only shows validated projections, the verified preview files the helper
  // names, and asks it to open a file it saved.
  property bool attachmentsSupported: false
  property var download: ({state: "idle", eventId: null, hash: null, path: null, received: 0, size: null, category: null})
  property var thumbnails: []
  property var pendingAttachments: []
  property var upload: ({state: "idle", scope: null, name: null, category: null})
  property string downloadRequestId: ""
  property string downloadLocal: "idle"
  property string downloadLocalCategory: ""
  property string downloadTarget: ""
  property string uploadRequestId: ""
  property string uploadLocal: "idle"
  property string uploadLocalCategory: ""
  property string uploadLocalScope: ""
  property string openRequestId: ""
  property string openCategory: ""
  property var thumbnailWanted: []
  property var thumbnailAsked: ({})
  property string thumbnailRequestId: ""
  property string thumbnailRequestHash: ""
  property bool thumbnailBackoff: false
  readonly property bool attachmentsAvailable: attachmentsSupported && !sampleMode && !sessionFailed && instanceId !== "" && bridge.running
  readonly property var attachmentMessages: ({
    attachment_unknown: "This attachment is no longer on screen. Refresh and try again.",
    attachment_forbidden: "The relay refused access to this file.",
    attachment_mismatch: "The file did not match what the message describes, so it was not kept.",
    attachment_too_large: "The file is too large.",
    attachment_invalid: "Choose an existing file of yours by its full path (at most four per message).",
    attachment_type_refused: "This type of file cannot be attached.",
    attachment_storage_unavailable: "The file could not be written. Check free space and permissions.",
    relay_unavailable: "Could not reach the relay. Check the connection and try again.",
    setup_busy: "Another transfer is running. Try again when it finishes."
  })
  function mediaOrigin(relay) {
    var match = typeof relay === "string" ? /^(wss?):\/\/([^\/\s@?#]+)\/$/.exec(relay) : null
    return match ? (match[1] === "wss" ? "https://" : "http://") + match[2] : ""
  }
  function attachmentName(value) {
    // Sanitized by the helper: at most 128 characters, no separators, controls or leading dot.
    return typeof value === "string" && value.length >= 1 && value.length <= 256 && utf8Size(value) <= 255
      && value.indexOf(".") !== 0 && !/[\/\\\u0000-\u001f\u007f-\u009f]/.test(value)
  }
  function attachmentFields(value, origin) {
    return typeof value.mime === "string" && value.mime.length <= 64 && /^[a-z0-9.+-]+\/[a-z0-9.+-]+$/.test(value.mime)
      && Number.isInteger(value.size) && value.size >= 1 && value.size <= 1073741824
      && typeof value.hash === "string" && /^[a-f0-9]{64}$/.test(value.hash)
      && typeof value.url === "string" && value.url.length <= 360 && origin !== ""
      && new RegExp("^" + origin.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "/media/" + value.hash + "\\.[a-z0-9]{1,8}$").test(value.url)
      && attachmentName(value.name)
      && (value.dim === null || typeof value.dim === "string" && /^[1-9][0-9]{0,4}x[1-9][0-9]{0,4}$/.test(value.dim)
        && value.dim.split("x").every(function(n) { return Number(n) <= 16384 }))
  }
  function attachmentKind(mime) {
    return ["image/jpeg", "image/png", "image/gif", "image/webp"].indexOf(mime) !== -1 ? "image" : mime.indexOf("video/") === 0 ? "video" : "file"
  }
  function validatedAttachment(value, origin) {
    if (!value || typeof value !== "object" || Array.isArray(value)
        || Object.keys(value).sort().join(",") !== "dim,hash,kind,mime,name,size,url"
        || !attachmentFields(value, origin) || value.kind !== attachmentKind(value.mime)) return null
    return {name: value.name, mime: value.mime, size: value.size, url: value.url, hash: value.hash, dim: value.dim, kind: value.kind}
  }
  function scopeValue(value) { return typeof value === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}(:[a-f0-9]{64})?$/.test(value) }
  function plainPath(value) {
    return typeof value === "string" && value.length > 1 && value.length <= 4096 && value.charAt(0) === "/"
      && !/[\u0000-\u001f\u007f-\u009f]/.test(value)
      && value.slice(1).split("/").every(function(part) { return part !== "" && part !== "." && part !== ".." })
  }
  // download, thumbnails, pendingAttachments and upload, exactly as the helper publishes them.
  function validatedTransfers(state, origin) {
    var d = state.download
    var failures = Object.keys(attachmentMessages)
    if (!d || typeof d !== "object" || Array.isArray(d)
        || Object.keys(d).sort().join(",") !== "category,eventId,hash,path,received,size,state"
        || ["idle", "downloading", "done", "failed"].indexOf(d.state) === -1
        || !Number.isInteger(d.received) || d.received < 0) return null
    if (d.state === "idle") {
      if (d.eventId !== null || d.hash !== null || d.path !== null || d.size !== null || d.category !== null || d.received !== 0) return null
    } else if (typeof d.eventId !== "string" || !/^[a-f0-9]{64}$/.test(d.eventId) || typeof d.hash !== "string" || !/^[a-f0-9]{64}$/.test(d.hash)
        || !Number.isInteger(d.size) || d.size < 1 || d.size > 1073741824 || d.received > d.size
        || (d.state === "done" ? !plainPath(d.path) || d.received !== d.size || d.category !== null
          : d.path !== null || (d.state === "failed" ? failures.indexOf(d.category) === -1 : d.category !== null))) return null
    var thumbs = state.thumbnails
    if (!Array.isArray(thumbs) || thumbs.length > 64) return null
    var seen = ({})
    for (var t = 0; t < thumbs.length; t++) {
      var thumb = thumbs[t]
      if (!thumb || typeof thumb !== "object" || Array.isArray(thumb) || Object.keys(thumb).sort().join(",") !== "hash,path"
          || typeof thumb.hash !== "string" || !/^[a-f0-9]{64}$/.test(thumb.hash) || seen[thumb.hash]
          || !plainPath(thumb.path) || thumb.path.length > 256
          || !new RegExp("/" + thumb.hash + "\\.(jpg|png|gif|webp)$").test(thumb.path)) return null
      seen[thumb.hash] = true
    }
    var pending = state.pendingAttachments
    if (!Array.isArray(pending) || pending.length > 16) return null
    var perScope = ({})
    var cleanPending = []
    for (var p = 0; p < pending.length; p++) {
      var item = pending[p]
      if (!item || typeof item !== "object" || Array.isArray(item)
          || Object.keys(item).sort().join(",") !== "dim,hash,mime,name,scope,size,url"
          || !scopeValue(item.scope) || !attachmentFields(item, origin)) return null
      var bucket = perScope[item.scope] || []
      if (bucket.length >= 4 || bucket.indexOf(item.hash) !== -1) return null
      bucket.push(item.hash)
      perScope[item.scope] = bucket
      cleanPending.push({scope: item.scope, name: item.name, mime: item.mime, size: item.size, url: item.url, hash: item.hash, dim: item.dim,
        kind: attachmentKind(item.mime)})
    }
    var u = state.upload
    if (!u || typeof u !== "object" || Array.isArray(u) || Object.keys(u).sort().join(",") !== "category,name,scope,state"
        || ["idle", "uploading", "done", "failed"].indexOf(u.state) === -1
        || (u.scope !== null && !scopeValue(u.scope)) || (u.name !== null && !attachmentName(u.name))) return null
    if (u.state === "idle" ? u.scope !== null || u.name !== null || u.category !== null
        : u.state === "failed" ? failures.indexOf(u.category) === -1
        : u.scope === null || u.name === null || u.category !== null) return null
    return {download: {state: d.state, eventId: d.eventId, hash: d.hash, path: d.path, received: d.received, size: d.size, category: d.category},
      thumbnails: thumbs.map(function(x) { return {hash: x.hash, path: x.path} }), pending: cleanPending,
      upload: {state: u.state, scope: u.scope, name: u.name, category: u.category}}
  }
  function applyTransfers(value, frame) {
    if (!value) { clearTransfers(); return }
    if (!sameProjection(download, value.download)) download = value.download
    var thumbsChanged = !sameProjection(thumbnails, value.thumbnails)
    if (thumbsChanged) thumbnails = value.thumbnails
    if (!sameProjection(pendingAttachments, value.pending)) pendingAttachments = value.pending
    if (!sameProjection(upload, value.upload)) upload = value.upload
    if (frame.type !== "status") return
    if (downloadLocal === "sending" && frame.id === downloadRequestId) { downloadLocal = "idle"; downloadRequestId = "" }
    if (uploadLocal === "sending" && frame.id === uploadRequestId) { uploadLocal = "idle"; uploadRequestId = "" }
    if (openRequestId !== "" && frame.id === openRequestId) openRequestId = ""
    if (thumbnailRequestId !== "" && frame.id === thumbnailRequestId) { thumbnailRequestId = ""; thumbnailRequestHash = "" }
    if (thumbsChanged) thumbnailBackoff = false
    pumpThumbnails()
  }
  function clearTransfers() {
    attachmentsSupported = false
    download = {state: "idle", eventId: null, hash: null, path: null, received: 0, size: null, category: null}
    thumbnails = []
    pendingAttachments = []
    upload = {state: "idle", scope: null, name: null, category: null}
    downloadRequestId = ""; downloadLocal = "idle"; downloadLocalCategory = ""; downloadTarget = ""
    uploadRequestId = ""; uploadLocal = "idle"; uploadLocalCategory = ""; uploadLocalScope = ""
    openRequestId = ""; openCategory = ""
    thumbnailWanted = []; thumbnailAsked = ({}); thumbnailRequestId = ""; thumbnailRequestHash = ""; thumbnailBackoff = false
  }
  // A refusal of one of this panel's attachment requests.
  function attachmentRefused(frame) {
    var category = frame.category === "request_busy" ? "setup_busy" : frame.category
    if (frame.id === downloadRequestId && downloadLocal === "sending") {
      downloadLocal = "failed"; downloadLocalCategory = category; downloadRequestId = ""
      return true
    }
    if (frame.id === uploadRequestId && uploadLocal === "sending") {
      uploadLocal = "failed"; uploadLocalCategory = category; uploadRequestId = ""
      return true
    }
    if (frame.id === openRequestId) { openRequestId = ""; openCategory = category; return true }
    if (frame.id === thumbnailRequestId) {
      // A full helper queue is retried once a preview arrives; anything else is final.
      if (category === "setup_busy") {
        var asked = Object.assign({}, thumbnailAsked)
        delete asked[thumbnailRequestHash]
        thumbnailAsked = asked
        thumbnailBackoff = true
      }
      thumbnailRequestId = ""; thumbnailRequestHash = ""
      pumpThumbnails()
      return true
    }
    return false
  }
  function attachmentRequest(kind, fields) {
    requestSequence++
    var request = {version: 1, id: "ui-" + requestSequence, type: kind}
    for (var key in fields) request[key] = fields[key]
    bridge.write(JSON.stringify(request) + "\n")
    return request.id
  }
  function thumbnailFor(hash) {
    var found = thumbnails.find(function(entry) { return entry.hash === hash })
    return found ? found.path : ""
  }
  function thumbnailUrl(hash) {
    var path = thumbnailFor(hash)
    return path ? "file://" + path.split("/").map(encodeURIComponent).join("/") : ""
  }
  // Images of at most 8 MiB on a shown row get a verified preview, one request at a time.
  function wantThumbnail(eventId, attachment) {
    if (!attachmentsAvailable || !attachment || attachment.kind !== "image" || attachment.size > 8388608
        || thumbnailFor(attachment.hash) !== "" || thumbnailAsked[attachment.hash]
        || thumbnailWanted.some(function(entry) { return entry.hash === attachment.hash })) return false
    thumbnailWanted = thumbnailWanted.concat([{eventId: eventId, hash: attachment.hash}]).slice(-64)
    pumpThumbnails()
    return true
  }
  function pumpThumbnails() {
    if (!attachmentsAvailable || connection !== "authenticated" || thumbnailRequestId !== "" || thumbnailBackoff || thumbnailWanted.length === 0) return
    var next = thumbnailWanted[0]
    thumbnailWanted = thumbnailWanted.slice(1)
    if (thumbnailFor(next.hash) !== "" || thumbnailAsked[next.hash]) { pumpThumbnails(); return }
    var asked = Object.assign({}, thumbnailAsked)
    asked[next.hash] = true
    thumbnailAsked = asked
    thumbnailRequestHash = next.hash
    thumbnailRequestId = attachmentRequest("thumbnail_attachment", {eventId: next.eventId, hash: next.hash})
  }
  readonly property bool downloadBusy: downloadLocal === "sending" || download.state === "downloading"
  function downloadAttachment(eventId, hash) {
    if (!attachmentsAvailable || connection !== "authenticated" || downloadBusy
        || typeof eventId !== "string" || !/^[a-f0-9]{64}$/.test(eventId) || typeof hash !== "string" || !/^[a-f0-9]{64}$/.test(hash)) return false
    downloadTarget = eventId + ":" + hash
    downloadLocal = "sending"
    downloadLocalCategory = ""
    openCategory = ""
    downloadRequestId = attachmentRequest("download_attachment", {eventId: eventId, hash: hash})
    return true
  }
  function downloadedPath(eventId, hash) {
    return download.state === "done" && download.eventId === eventId && download.hash === hash ? download.path : ""
  }
  function openDownload(path) {
    if (!attachmentsAvailable || !plainPath(path) || path !== download.path || download.state !== "done") return false
    openCategory = ""
    openRequestId = attachmentRequest("open_download", {path: path})
    return true
  }
  function downloadLabelFor(eventId, hash) {
    var target = eventId + ":" + hash
    if (downloadTarget === target && downloadLocal === "failed") return attachmentMessages[downloadLocalCategory] || "The download did not start."
    if (download.eventId !== eventId || download.hash !== hash) return downloadTarget === target && downloadLocal === "sending" ? "Starting download…" : ""
    if (download.state === "downloading") return "Downloading " + formatSize(download.received) + " of " + formatSize(download.size) + "…"
    if (download.state === "failed") return attachmentMessages[download.category] || "The download failed."
    if (download.state === "done") return openCategory !== "" ? "Saved, but it could not be opened." : "Saved to " + download.path
    return ""
  }
  function formatSize(bytes) {
    if (!Number.isInteger(bytes) || bytes < 0) return ""
    if (bytes < 1000) return bytes + " B"
    var units = ["KB", "MB", "GB"]
    var value = bytes / 1000
    var unit = 0
    while (value >= 1000 && unit < units.length - 1) { value /= 1000; unit++ }
    return value.toFixed(1) + " " + units[unit]
  }
  function draftScopeFor(rootId) { return selectedRoomId + (rootId ? ":" + rootId : "") }
  function pendingFor(rootId) {
    var scope = draftScopeFor(rootId)
    return selectedRoomId === "" ? [] : pendingAttachments.filter(function(item) { return item.scope === scope })
  }
  function uploadAttachment(path, rootId) {
    if (!attachmentsAvailable || connection !== "authenticated" || uploadLocal === "sending" || upload.state === "uploading"
        || !selectedRoom || typeof path !== "string") return false
    path = path.trim()
    if (!plainPath(path)) { uploadLocal = "failed"; uploadLocalCategory = "attachment_invalid"; uploadLocalScope = draftScopeFor(rootId); return false }
    if (rootId && !canReplyTo(rootId)) return false
    if (pendingFor(rootId).length >= 4) { uploadLocal = "failed"; uploadLocalCategory = "attachment_invalid"; uploadLocalScope = draftScopeFor(rootId); return false }
    var fields = {roomId: selectedRoomId, path: path}
    if (rootId) fields.rootId = rootId
    uploadLocal = "sending"
    uploadLocalCategory = ""
    uploadLocalScope = draftScopeFor(rootId)
    uploadRequestId = attachmentRequest("upload_attachment", fields)
    return true
  }
  function removePendingAttachment(hash) {
    if (!attachmentsAvailable || deliveryState === "sending" || !pendingAttachments.some(function(item) { return item.hash === hash })) return false
    attachmentRequest("remove_pending_attachment", {hash: hash})
    return true
  }
  function uploadLabelFor(rootId) {
    var scope = draftScopeFor(rootId)
    if (uploadLocal === "failed" && uploadLocalScope === scope) return attachmentMessages[uploadLocalCategory] || "The file was not attached."
    if (uploadLocal === "sending" && uploadLocalScope === scope) return "Checking the file…"
    if (upload.scope !== scope) return ""
    if (upload.state === "uploading") return "Uploading " + upload.name + "…"
    if (upload.state === "failed") return attachmentMessages[upload.category] || "The file was not attached."
    return ""
  }

  function chooseSetupProvider(provider) {
    // Presentation only: choosing a provider never writes config or sends IPC.
    // "custom" and "hosted" are the names older panels used.
    var chosen = ({join: "join", create: "create", custom: "join", hosted: "create"})[provider]
    if (chosen) setupProvider = chosen
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
  // A hand-typed `@name` notifies someone only when it cannot mean anyone else.
  // At an `@` that starts a word (same boundaries as picker tokens), take the
  // longest roster form that follows, compared case-insensitively and ending at
  // a word boundary. A form is an entry's trimmed name, or that name with its
  // spaces removed or replaced by dashes. It resolves only if exactly one key
  // has it; an ambiguous longest form resolves nothing at that position. Only
  // this room's verified roster snapshot is used.
  function resolveTypedMentions(text) {
    if (!recipientsSupported || recipientsState !== "snapshot" || recipientsRoomId !== selectedRoomId || selectedRoomId === ""
        || typeof text !== "string" || text.indexOf("@") === -1) return []
    var forms = new Map()
    recipientEntries.forEach(function(entry) {
      var name = (entry.name || "").trim().toLowerCase()
      if (!name) return
      var variants = [name, name.replace(/\s+/g, ""), name.replace(/\s+/g, "-")]
      variants.forEach(function(form) {
        var keys = forms.get(form) || []
        if (keys.indexOf(entry.key) === -1) keys.push(entry.key)
        forms.set(form, keys)
      })
    })
    var names = Array.from(forms.keys()).sort(function(a, b) { return b.length - a.length })
    var found = []
    for (var at = text.indexOf("@"); at !== -1; at = text.indexOf("@", at + 1)) {
      if (at > 0 && !/[\s([{,;:]/.test(text[at - 1])) continue
      for (var i = 0; i < names.length; i++) {
        var form = names[i]
        // A name whose lower case changes length never matches: fail closed.
        if (text.substr(at + 1, form.length).toLowerCase() !== form) continue
        var after = text[at + 1 + form.length] || ""
        if (after && !/[\s.,!?;:)\]}]/.test(after)) continue
        var keys = forms.get(form)
        if (keys.length === 1 && found.indexOf(keys[0]) === -1) found.push(keys[0])
        break
      }
    }
    return found
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
    var attached = pendingFor(replyRootId).map(function(item) { return item.hash }).join(",")
    if (!submissionId || submissionRoom !== selectedRoomId || submissionRoot !== replyRootId || submissionText !== draftText || JSON.stringify(submissionMentions) !== JSON.stringify(outgoingMentions)
        || submissionAttachments !== attached) submissionId = correlationUuid()
    submissionAttachments = attached
    submissionRoom = selectedRoomId
    submissionRoot = replyRootId
    submissionDraftKey = composerKey
    submissionText = draftText
    submissionMentions = outgoingMentions.slice()
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
    loseAction()
  }
  function validatedDelivery(delivery) {
    var categories = ["delivery_unknown","send_rejected","send_unavailable","send_invalid","send_busy","send_access_denied","send_ledger_unavailable","send_request_reused","send_scope_changed","send_not_author","send_unsupported"]
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
  // A native emoji the helper accepts as a reaction: never a :shortcode:, and
  // never the agents' 👀/💬 acknowledgements.
  function validReactionEmoji(value) {
    return typeof value === "string" && value.charAt(0) !== ":" && value !== "👀" && value !== "💬" && validStatusEmoji(value)
  }
  function isOwnRow(row) { return !!row && identity !== "" && row.author === identity && row.unavailable !== true }
  function canEditRow(row) {
    return canAct && isOwnRow(row) && row.truncated !== true && row.attachmentsUnavailable !== true
      && !(Array.isArray(row.attachments) && row.attachments.length > 0)
  }
  function canDeleteRow(row) { return canAct && isOwnRow(row) }
  function canReactTo(row) { return canAct && !!row && !!row.reactions && row.unavailable !== true }
  function submitAction(kind, id, fields) {
    if (!canAct || !bridge.running || !selectedRoomId || typeof id !== "string" || !/^[a-f0-9]{64}$/.test(id)) return false
    actionId = correlationUuid()
    actionKind = kind
    actionTarget = id
    actionEmoji = fields.emoji || ""
    actionRoom = selectedRoomId
    actionGeneration = generation
    actionInstance = instanceId
    actionState = "sending"
    actionCategory = ""
    actionClear.stop()
    var request = {version: 1, id: actionId, type: kind, roomId: actionRoom, eventId: id, generation: generation, instanceId: instanceId}
    if (fields.text !== undefined) request.text = fields.text
    if (fields.emoji !== undefined) request.emoji = fields.emoji
    bridge.write(JSON.stringify(request) + "\n")
    actionTimeout.restart()
    return true
  }
  function editMessage(id, text) {
    var trimmed = typeof text === "string" ? text.trim() : ""
    if (trimmed === "" || utf8Size(trimmed) > 4096) return false
    return submitAction("edit_message", id, {text: trimmed})
  }
  function deleteMessage(id) { return submitAction("delete_message", id, {}) }
  function toggleReaction(id, emoji, mine) {
    if (!validReactionEmoji(emoji)) return false
    return submitAction(mine ? "remove_reaction" : "add_reaction", id, {emoji: emoji})
  }
  function clearAction() {
    actionTimeout.stop()
    actionClear.stop()
    actionId = ""
    actionKind = ""
    actionTarget = ""
    actionEmoji = ""
    actionState = "idle"
    actionCategory = ""
  }
  // A lost session or answer is not a refusal: the change may have been applied.
  function loseAction() {
    actionTimeout.stop()
    if (actionState === "sending") { actionState = "unknown"; actionCategory = "delivery_unknown" }
  }
  function applyActionDelivery(delivery) {
    if (!delivery || actionState !== "sending" || delivery.state === "idle" || delivery.requestId !== actionId
        || delivery.roomId !== actionRoom || generation !== actionGeneration || instanceId !== actionInstance) return
    actionState = delivery.state
    actionCategory = delivery.category || ""
    if (actionState === "sending") return
    actionTimeout.stop()
    if (actionState === "acknowledged") {
      actionClear.restart()
      if (selectedRoomId === actionRoom) {
        refreshHistory()
        if (threadRootId !== "" && threadState === "snapshot") refreshThread()
      }
    }
  }
  function actionNote(id) {
    if (actionTarget === "" || actionTarget !== id || actionState === "idle") return ""
    var notes = {send_not_author: "Only your own messages can be changed.", send_unsupported: "This cannot be changed here.",
      send_access_denied: "That message is not available to change.", send_busy: "Another send is in progress. Try again in a moment.",
      send_invalid: "That was not sent.", send_unavailable: "Not connected. Nothing was sent.", send_scope_changed: "Outcome unknown. The room changed while sending.",
      send_ledger_unavailable: "Local send ledger unavailable. Nothing was sent."}
    var what = ({edit_message: "Edit", delete_message: "Delete", add_reaction: "Reaction", remove_reaction: "Reaction"})[actionKind] || "Change"
    if (actionState === "sending") return what + " sending…"
    if (actionState === "acknowledged") return what + " accepted by the relay"
    if (actionState === "rejected") return "The relay refused this " + what.toLowerCase() + ". Nothing changed."
    if (actionState === "unknown") return what + " outcome unknown. It may or may not have applied; check the message before trying again."
    return notes[actionCategory] || (what + " failed. Nothing changed.")
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
  function validatedThread(value, media) {
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
    var checked = validatedHistory({state:value.state, roomId:value.roomId, rows:value.rows, hasMore:value.hasMore, category:null}, 200, media)
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
  function validatedRecipients(value, withStatus, withPresence) {
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
      var status = withStatus ? validatedRecipientStatus(entry.status) : null
      if (status === undefined) return null
      if (withPresence && entry.presence !== null && !validPresenceState(entry.presence)) return null
      seen[entry.key] = true
      entries.push({key:entry.key,name:entry.name,status:status,presence:withPresence ? entry.presence : null})
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
  // An author the room's member list does not know (someone who joined after
  // it was read, or an agent just added): read the list again, at most once
  // every 30 s per room, so names and the @ picker catch up without leaving.
  property string unknownAuthorRoom: ""
  Timer { id: unknownAuthorCooldown; interval: 30000 }
  function noteUnknownAuthors(rows) {
    if (!recipientsSupported || recipientsState !== "snapshot" || recipientsRoomId !== selectedRoomId || selectedRoomId === "") return
    if (unknownAuthorCooldown.running && unknownAuthorRoom === selectedRoomId) return
    var known = {}
    for (var i = 0; i < recipientEntries.length; i++) known[recipientEntries[i].key] = true
    if (!rows.some(function(row) { return typeof row.author === "string" && row.author !== "" && !known[row.author] })) return
    unknownAuthorRoom = selectedRoomId
    unknownAuthorCooldown.restart()
    send("fetch_recipients", selectedRoomId)
  }
  function messageAuthorName(key) {
    // Signed profile labels are presentation only; mentions still use exact keys.
    if (recipientsState === "snapshot" && recipientsRoomId === selectedRoomId && selectedRoomId !== "") {
      var recipient = recipientEntries.find(function(entry) { return entry.key === key })
      if (recipient && recipient.name.trim()) return recipient.name
    }
    if (knownNames[key]) return knownNames[key]
    return key.slice(0, 12) + "…"
  }
  function messageAuthorLabel(key) {
    var label = messageAuthorName(key)
    if (recipientsState === "snapshot" && recipientsRoomId === selectedRoomId
        && agentProfiles.some(function(agent) { return agent.key === key })) label += " · Self-described agent"
    return label
  }
  function clearCatalog() {
    notificationQueue = []
    roomActivity = RoomActivity.fresh()
    activityObservation = ActivityObserver.fresh()
    clearHistory()
    clearRecipients()
    catalogRooms = []
    catalogState = "unavailable"
    catalogCategory = ""
    catalogMore = "none"
    catalogMoreCategory = ""
    if (!sampleMode) selectedRoomId = ""
  }
  readonly property var knownCapabilities: ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients", "history_auto_refresh", "room_activity", "agent_profiles", "thread_replies", "thread_send", "thread_summaries", "dm_open", "older_history", "live_updates", "setup_assist", "community_join", "invite_mint", "attachments", "user_status", "presence", "communities", "people_search", "message_actions", "room_manage"]
  // Distinct known names only, so the length bound follows the list.
  function validCapabilities(capabilities) {
    return Array.isArray(capabilities) && capabilities.length >= 1 && capabilities.length <= knownCapabilities.length
      && capabilities.indexOf("connection_status") !== -1
      && capabilities.every(function(cap, index) {
        return knownCapabilities.indexOf(cap) !== -1 && capabilities.indexOf(cap) === index
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
        || !Array.isArray(catalog.rooms) || catalog.rooms.length > maxRooms
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
    // Paging fields are absent from helpers without `room_manage`.
    var more = catalog.more === undefined ? "none" : catalog.more
    var moreCategory = catalog.moreCategory === undefined ? null : catalog.moreCategory
    if (["none", "available", "loading", "failed", "limit"].indexOf(more) === -1
        || (more === "failed") !== (moreCategory !== null)
        || (moreCategory !== null && ["room_catalog_unavailable", "room_catalog_timeout"].indexOf(moreCategory) === -1)
        || (more !== "none" && ["partial", "ready"].indexOf(catalog.state) === -1)) return null
    return {state: catalog.state, rooms: clean, category: catalog.category || "", more: more, moreCategory: moreCategory || ""}
  }
  // Reaction chips: absent from helpers without `message_actions`; else at most
  // 16 distinct native emoji with a count and whether this identity reacted.
  function validChips(chips) {
    if (chips === undefined) return true
    return Array.isArray(chips) && chips.length <= 16 && chips.every(function(chip, index) {
      return !!chip && typeof chip === "object" && Object.keys(chip).sort().join(",") === "count,emoji,mine"
        && validReactionEmoji(chip.emoji) && Number.isInteger(chip.count) && chip.count >= 1 && chip.count <= 1000
        && typeof chip.mine === "boolean" && chips.findIndex(function(other) { return other.emoji === chip.emoji }) === index
    })
  }
  function validatedHistory(history, limit, media) {
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
            || !Number.isInteger(reactions.working) || reactions.working < 0 || reactions.working > 200
            || !validChips(reactions.chips)))
          || (thread != null && (typeof thread !== "object" || Array.isArray(thread)
            || !Number.isInteger(thread.replies) || thread.replies < 0 || thread.replies > 1000000
            || (thread.lastReplyAt !== null && (!Number.isInteger(thread.lastReplyAt) || thread.lastReplyAt < 0 || thread.lastReplyAt > 253402300799))
            || !Array.isArray(thread.participants) || thread.participants.length > 10
            || !thread.participants.every(function(key, index) { return typeof key === "string" && /^[a-f0-9]{64}$/.test(key) && thread.participants.indexOf(key) === index })))) return null
      try { if (encodeURIComponent(row.text).replace(/%[A-F0-9]{2}/gi, "x").length > 2048) return null }
      catch (_) { return null }
      // Present exactly with the `attachments` capability; bounded per frame.
      var attached = []
      if (media) {
        if (typeof row.attachmentsUnavailable !== "boolean" || !Array.isArray(row.attachments) || row.attachments.length > 4
            || ((row.unavailable || row.attachmentsUnavailable) && row.attachments.length !== 0)
            || (row.unavailable && row.attachmentsUnavailable)) return null
        for (var a = 0; a < row.attachments.length; a++) {
          var attachment = validatedAttachment(row.attachments[a], media.origin)
          if (!attachment || attached.some(function(other) { return other.hash === attachment.hash })) return null
          attached.push(attachment)
        }
        media.count += attached.length
        if (media.count > 48) return null
      } else if (row.attachments !== undefined || row.attachmentsUnavailable !== undefined) return null
      ids[row.id] = true
      clean.push({id: row.id, author: row.author, time: row.time, text: row.unavailable ? "" : row.text,
        edited: row.edited, truncated: row.truncated, unavailable: row.unavailable,
        reactions: reactions == null ? null : {seen: reactions.seen, working: reactions.working,
          chips: (reactions.chips || []).map(function(chip) { return {emoji: chip.emoji, count: chip.count, mine: chip.mine} })},
        thread: thread == null ? null : {replies: thread.replies, lastReplyAt: thread.lastReplyAt, participants: thread.participants.slice()},
        attachments: attached, attachmentsUnavailable: media ? row.attachmentsUnavailable : false})
    }
    return {state: history.state, roomId: history.roomId, rows: clean, hasMore: history.hasMore, category: history.category || "",
      nextCursor: cursor === null ? null : {createdAt: cursor.createdAt, id: cursor.id}, olderState: olderState, live: live}
  }
  function beginSession() {
    clearTransfers()
    loseSetup()
    setupAssistSupported = false
    loseInvite()
    loseRoomAction()
    communityJoinSupported = false
    clearJoin()
    loseMint()
    inviteMintSupported = false
    clearInvites()
    clearUserStatus()
    userStatusSupported = false
    clearPresence()
    presenceSupported = false
    loseCommunity()
    communitiesSupported = false
    clearCommunities()
    losePendingDelivery()
    loseDmOpen()
    dmOpenSupported = false
    clearPeople()
    peopleSupported = false
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
    clockSkewSeconds = null
    helperReconnecting = false
    bridgeStartedAt = 0
    bridgeRestart.stop()
  }
  // `status.clockSkewSeconds`: relay minus local seconds, informational only.
  // Absent from helpers older than `clock_skew`; bounded to ±10 years.
  function validClockSkew(value) {
    return value === undefined || value === null || (Number.isInteger(value) && Math.abs(value) <= 315576000)
  }
  function clockSkewText(seconds) {
    if (!Number.isInteger(seconds)) return "Clock is off"
    var minutes = Math.round(Math.abs(seconds) / 60)
    var amount = minutes < 120 ? minutes + " min" : Math.round(minutes / 60) + " h"
    return "Clock is off by " + amount + " " + (seconds > 0 ? "behind" : "ahead")
  }
  function fail(reason) {
    clearTransfers()
    loseSetup()
    setupAssistSupported = false
    loseInvite()
    loseRoomAction()
    communityJoinSupported = false
    clearJoin()
    loseMint()
    inviteMintSupported = false
    clearInvites()
    clearUserStatus()
    userStatusSupported = false
    clearPresence()
    presenceSupported = false
    loseCommunity()
    communitiesSupported = false
    clearCommunities()
    losePendingDelivery()
    loseDmOpen()
    dmOpenSupported = false
    clearPeople()
    peopleSupported = false
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
    helperReconnecting = false
    bridge.running = false
    scheduleBridgeRestart()
  }
  // A bridge that ended (helper restarted or reinstalled, daemon gone, a bad
  // frame or no hello) is started again after a delay. An incompatible helper
  // needs a new release first, and a relative helper path never works.
  function scheduleBridgeRestart() {
    if (!autoRestart || bridgeRestart.running || bridge.running || category === "incompatible_response"
        || !helperExecutable.startsWith("/")) return
    if (bridgeStartedAt > 0 && Date.now() - bridgeStartedAt >= 60000) bridgeRestarts = 0
    bridgeRestart.interval = [1000, 2000, 5000, 10000][bridgeRestarts] || 30000
    bridgeRestarts = Math.min(bridgeRestarts + 1, 4)
    bridgeRestart.start()
  }
  function startBridge() {
    if (!helperExecutable.startsWith("/")) { fail("helper_unavailable"); return }
    beginSession()
    bridge.running = true
    handshake.restart()
  }
  // Opening the panel reconnects at once, at most every 5 seconds, when the
  // relay or the bridge is lost (not when setup, a locked identity or an
  // incompatible helper needs the user).
  function reconnectOnOpen() {
    if (sampleMode || !autoRestart || Date.now() - reconnectRequestedAt < 5000) return
    var lost = sessionFailed ? !bridge.running && category !== "incompatible_response"
      : connection === "disconnected" && bridge.running && instanceId !== ""
    if (!lost) return
    reconnectRequestedAt = Date.now()
    if (sessionFailed) startBridge()
    else send("retry_connection")
  }
  function clearJoin() {
    joinSetup = {state: "idle", inviteCode: null, joinPolicy: null, claim: null, category: null}
    openRooms = []
    openRoomsState = "unavailable"
    openRoomsCategory = ""
    roomAction = {state: "idle", action: null, requestId: null, roomId: null, category: null, detail: null}
    roomDetail = roomDetailIdle
  }
  function boundedString(value, limit) { return typeof value === "string" && value.length <= limit }
  function acceptFrame(line) {
    if (sessionFailed) return false
    if (!boundedString(line, 2097152)) { fail("invalid_response"); return false }
    var frame
    try { frame = JSON.parse(line) } catch (_) { fail("invalid_response"); return false }
    if (frame && frame.version === 1 && frame.type === "error" && ["request_busy", "send_busy", "send_scope_changed", "send_request_reused", "send_invalid", "send_unavailable", "send_access_denied", "send_ledger_unavailable", "delivery_unknown",
        "dm_open_busy", "dm_open_scope_changed", "dm_open_request_reused", "dm_open_invalid", "dm_open_unavailable", "dm_open_access_denied", "dm_open_unknown",
        "setup_invalid_relay", "identity_exists", "identity_unavailable", "relay_unavailable", "setup_busy", "setup_not_allowed", "config_unavailable",
        "invite_invalid", "invite_relay_mismatch", "invite_rejected", "invite_rate_limited", "policy_required", "room_not_open", "join_rejected", "leave_rejected", "room_invalid", "room_scope_changed",
        "invite_forbidden", "attachment_unknown", "attachment_forbidden", "attachment_mismatch", "attachment_too_large", "attachment_invalid",
        "attachment_type_refused", "attachment_storage_unavailable", "status_invalid", "status_rate_limited", "status_rejected",
        "join_invalid", "join_rate_limited", "join_busy", "join_last", "join_full", "community_unknown", "name_invalid", "leave_owner"].indexOf(frame.category) !== -1) {
      if (instanceId === "" || frame.instanceId !== instanceId) return false
      if (!boundedString(frame.id, 128) || !/^ui-[0-9]+$/.test(frame.id) && !uuidValue(frame.id)) { fail("invalid_response"); return false }
      if (frame.id === presenceRequestId && presenceRequestId !== "") {
        // Not taken (no session yet, or busy): sent again shortly.
        presenceRequestId = ""
        presenceSentKey = ""
        presenceRetry.restart()
        return true
      }
      if (frame.id === setupRequestId && setupState === "sending") {
        refuseSetup(frame.category === "request_busy" ? "setup_busy" : frame.category)
        return true
      }
      if (frame.id === communityRequestId && communityLocal === "sending") {
        // Refused, failed, or the helper lost the request; nothing is assumed saved.
        communityTimeout.stop()
        communityLocal = "failed"
        communityCategory = communityCategories.indexOf(frame.category) !== -1 ? frame.category : "join_busy"
        communityRequestId = ""
        communityRequestDone(communityRequestKind, false)
        return true
      }
      if (frame.id === statusRequestId && statusRequestState === "sending") {
        // Refused before anything was signed, or the helper lost the request.
        statusTimeout.stop()
        statusRequestState = "failed"
        statusRequestCategory = frame.category === "request_busy" ? "status_rate_limited" : frame.category
        statusRequestId = ""
        return true
      }
      if (frame.id === mintRequestId && mintState === "sending") {
        // A refusal or a failed mint; the status view carries the same category.
        mintTimeout.stop()
        mintState = "failed"
        mintCategory = frame.category === "request_busy" ? "setup_busy" : frame.category
        mintRequestId = ""
        return true
      }
      if (attachmentRefused(frame)) return true
      if (frame.id === inviteRequestId && inviteState === "sending") {
        // A refusal or a failed redemption; the status view carries the same category.
        inviteTimeout.stop()
        inviteState = "failed"
        inviteCategory = frame.category === "request_busy" ? "setup_busy" : frame.category
        inviteRequestId = ""
        return true
      }
      if (frame.id === roomActionRequestId && roomActionLocal === "sending") {
        // Refused before anything was signed.
        roomActionTimeout.stop()
        roomActionLocal = "failed"
        roomActionCategory = frame.category === "request_busy" ? "setup_busy" : frame.category
        joinTarget = ""
        return true
      }
      if (frame.id === dmOpenRequestId && dmOpenState === "sending") {
        // Refusals are known: nothing was signed. A lost helper reply is not.
        dmOpenTimeout.stop()
        dmOpenState = frame.category === "dm_open_unknown" ? "unknown" : "failed"
        dmOpenCategory = frame.category === "request_busy" ? "dm_open_busy" : frame.category
      }
      if (frame.id === actionId && actionState === "sending") {
        // Refused before anything was signed, unless the helper's answer was lost.
        actionTimeout.stop()
        actionState = ["send_scope_changed", "delivery_unknown"].indexOf(frame.category) !== -1 ? "unknown" : "failed"
        actionCategory = frame.category === "request_busy" ? "send_busy" : frame.category
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
    var categories = ["identity_access_pending", "identity_missing", "identity_locked", "identity_invalid", "identity_unavailable", "auth_rejected", "clock_skew", "relay_timeout", "relay_unavailable", "relay_resource_limit", "relay_protocol_error", "config_unavailable", "invalid_config"]
    if (states.indexOf(state.connection) === -1
        || (state.category !== null && categories.indexOf(state.category) === -1)
        || !validClockSkew(state.clockSkewSeconds)
        // Absent from helpers older than automatic reconnection.
        || (state.reconnecting !== undefined && typeof state.reconnecting !== "boolean")
        || (state.reconnecting === true && state.connection === "authenticated")
        || (state.identity !== null && (!boundedString(state.identity, 64) || !/^[a-f0-9]{64}$/.test(state.identity)))
        || (state.relay !== null && (!boundedString(state.relay, 2048) || !/^wss?:\/\//.test(state.relay) || state.relay.indexOf("@") !== -1))) {
      fail("invalid_response"); return false
    }
    var catalog = {state: "unavailable", rooms: [], category: "", more: "none", moreCategory: ""}
    if (frame.capabilities.indexOf("room_catalog") !== -1) {
      catalog = validatedCatalog(state.catalog)
      if (!catalog) { fail("invalid_response"); return false }
    }
    var history = null
    var supportsHistory = frame.capabilities.indexOf("room_history") !== -1
    var supportsAttachments = supportsHistory && frame.capabilities.indexOf("attachments") !== -1
    var origin = mediaOrigin(state.relay)
    if (frame.capabilities.indexOf("attachments") !== -1 && (!supportsHistory || (state.relay !== null && origin === ""))) { fail("invalid_response"); return false }
    if (supportsHistory) {
      history = validatedHistory(state.history, 100, supportsAttachments ? {origin: origin, count: 0} : null)
      if (!history || (history.live && frame.capabilities.indexOf("live_updates") === -1)) { fail("invalid_response"); return false }
    }
    var supportsThread = frame.capabilities.indexOf("thread_replies") !== -1
    var thread = supportsThread ? validatedThread(state.thread, supportsAttachments ? {origin: origin, count: 0} : null) : null
    if (supportsThread && (!supportsHistory || !thread)) { fail("invalid_response"); return false }
    var supportsRecipients = frame.capabilities.indexOf("room_recipients") !== -1
    var supportsStatus = frame.capabilities.indexOf("user_status") !== -1
    var supportsPresence = frame.capabilities.indexOf("presence") !== -1
    var recipients = supportsRecipients ? validatedRecipients(state.recipients, supportsStatus, supportsPresence) : null
    if (supportsRecipients && !recipients) { fail("invalid_response"); return false }
    var agents = frame.capabilities.indexOf("agent_profiles") !== -1 ? validatedAgents(state.recipients && state.recipients.agents, recipients) : []
    if (agents === null) { fail("invalid_response"); return false }

    var supportsSend = frame.capabilities.indexOf("message_send") !== -1
    var delivery = supportsSend ? validatedDelivery(state.delivery) : null
    if (supportsSend && !delivery) { fail("invalid_response"); return false }
    var supportsDmOpen = frame.capabilities.indexOf("dm_open") !== -1
    var dmOpen = supportsDmOpen ? validatedDmOpen(state.dmOpen) : null
    if (supportsDmOpen && !dmOpen) { fail("invalid_response"); return false }
    var supportsPeople = frame.capabilities.indexOf("people_search") !== -1
    var people = supportsPeople ? validatedPeople(state.people) : null
    if (supportsPeople && !people) { fail("invalid_response"); return false }
    var supportsJoin = frame.capabilities.indexOf("community_join") !== -1
    var join = supportsJoin ? validatedJoinSetup(state.setup) : null
    var open = supportsJoin ? validatedOpenRooms(state.openRooms, catalog) : null
    var action = supportsJoin ? validatedRoomAction(state.roomAction) : null
    if (supportsJoin && (!join || !open || !action)) { fail("invalid_response"); return false }
    var supportsRoomManage = frame.capabilities.indexOf("room_manage") !== -1
    var detail = supportsRoomManage ? validatedRoomDetail(state.roomDetail) : roomDetailIdle
    if (supportsRoomManage && !detail) { fail("invalid_response"); return false }
    var shownStatus = supportsStatus ? validatedUserStatus(state.userStatus) : null
    if (supportsStatus && !shownStatus) { fail("invalid_response"); return false }
    var shownPresence = supportsPresence ? validatedPresence(state.presence) : null
    if (supportsPresence && !shownPresence) { fail("invalid_response"); return false }
    var supportsCommunities = frame.capabilities.indexOf("communities") !== -1
    var shownCommunities = supportsCommunities ? validatedCommunities(state.communities, state.relay) : null
    if (supportsCommunities && !shownCommunities) { fail("invalid_response"); return false }
    var supportsMint = frame.capabilities.indexOf("invite_mint") !== -1
    var minted = supportsMint ? validatedInvites(state.invites) : null
    if (supportsMint && !minted) { fail("invalid_response"); return false }
    var transfers = supportsAttachments ? validatedTransfers(state, origin) : null
    if (supportsAttachments && !transfers) { fail("invalid_response"); return false }
    var supportsActivity = frame.capabilities.indexOf("room_activity") !== -1
    if (supportsActivity && (!Array.isArray(state.activity) || state.activity.length > RoomActivity.MAX_SUMMARIES || state.activity.some(function(a, i) {
      return !RoomActivity.valid(a) || !uuidValue(a.roomId) || !catalog.rooms.some(function(r) { return r.id === a.roomId })
        || state.activity.slice(0,i).some(function(b) { return b.roomId === a.roomId })
    }))) { fail("invalid_response"); return false }
    var incomingScope = (state.relay || "") + "|" + (state.identity || "")
    var scopeBefore = draftScopeKey
    // A frame without a relay or identity (setup, disconnected) keeps the names.
    if (state.relay && state.identity && incomingScope !== knownNamesScope) forgetNames(incomingScope)
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
    var generationBefore = generation
    var resyncingCatalog = resyncStage === "catalog"
    if (state.connection !== "authenticated") catalog = {state: "unavailable", rooms: [], category: "", more: "none", moreCategory: ""}
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
      catalogMore = catalog.more
      catalogMoreCategory = catalog.moreCategory
    }
    if (["partial", "ready"].indexOf(catalogState) !== -1
        && !catalogRooms.some(function(room) { return room.id === root.selectedRoomId })) {
      clearHistory()
      clearRecipients()
      var rememberedId = rememberedRoomFor(incomingScope)
      var remembered = rememberedId !== "" && visibleRooms.some(function(room) { return room.id === rememberedId })
      selectedRoomId = remembered ? rememberedId : visibleRooms.length ? visibleRooms[0].id : ""
    }
    historySupported = supportsHistory
    if (state.connection !== "authenticated" || !supportsHistory || ["loading", "unavailable"].indexOf(catalogState) !== -1) clearHistory()
    else if (history && history.roomId === selectedRoomId && selectedRoomId !== "") {
      if (!(history.state === "loading" && historyState === "snapshot")) {
        if (!sameProjection(historyRows, history.rows)) { historyRows = history.rows; noteUnknownAuthors(historyRows) }
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
    messageActionsSupported = supportsSend && supportsHistory && frame.capabilities.indexOf("message_actions") !== -1
    if (!messageActionsSupported) clearAction()
    threadSendSupported = supportsThread && frame.capabilities.indexOf("message_send") !== -1 && frame.capabilities.indexOf("thread_send") !== -1
    threadSupported = supportsThread
    if (!supportsThread || state.connection !== "authenticated" || historyState !== "snapshot"
        || !historyRows.some(function(row) { return row.id === root.threadRootId && !row.unavailable })) clearThread()
    else if (thread && thread.roomId === selectedRoomId && thread.rootId === threadRootId) {
      // Loading frames for a same-scope refresh carry an empty row list.
      // Keep the last validated snapshot visible until the refresh finishes.
      if (!(thread.state === "loading" && threadState === "snapshot")) {
        if (!sameProjection(threadRows, thread.rows)) { threadRows = thread.rows; noteUnknownAuthors(threadRows) }
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
        // Only a changed member list is new evidence; repeats must not undo a newer name.
        var rosterChanged = !sameProjection(recipientEntries, recipients.entries) || recipientsState !== recipients.state
        if (!sameProjection(recipientEntries, recipients.entries)) recipientEntries = recipients.entries
        if (!sameProjection(agentProfiles, agents)) agentProfiles = agents
        if (recipients.state === "snapshot" && rosterChanged) rememberNames(recipients.entries)
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
      activityUpdate.notices.forEach(function(item) { announceNotice(item.roomId, item.notice) })
    }
    recipientsSupported = supportsRecipients
    automaticHistorySupported = frame.capabilities.indexOf("history_auto_refresh") !== -1
    threadSummariesSupported = supportsHistory && frame.capabilities.indexOf("thread_summaries") !== -1
    olderHistorySupported = supportsHistory && frame.capabilities.indexOf("older_history") !== -1
    liveUpdatesSupported = supportsHistory && frame.capabilities.indexOf("live_updates") !== -1
    if (frame.type === "hello") bridgeStartedAt = Date.now()
    instanceId = frame.instanceId
    generation = frame.generation
    applyNotificationTarget()
    relay = state.relay || ""
    connection = state.connection
    category = state.category || ""
    clockSkewSeconds = state.clockSkewSeconds === undefined ? null : state.clockSkewSeconds
    helperReconnecting = state.reconnecting === true
    sendSupported = supportsSend
    identity = state.identity || ""
    dmOpenSupported = supportsDmOpen
    if (!supportsDmOpen) loseDmOpen()
    // The directory belongs to one authenticated connection and community.
    if (!supportsPeople || state.connection !== "authenticated" || frame.generation !== generationBefore
        || incomingScope !== scopeBefore) clearPeople()
    peopleSupported = supportsPeople
    if (people) applyPeople(people)
    setupAssistSupported = frame.capabilities.indexOf("setup_assist") !== -1
    if (frame.type === "status" && setupState === "sending" && frame.id === setupRequestId && frame.instanceId === setupInstance) {
      setupTimeout.stop()
      if (setupRequestKind === "create_identity" && identity !== "") createdIdentity = identity
      setupState = "idle"
      setupRequestId = ""
    }
    if (createdIdentity !== "" && createdIdentity !== identity) createdIdentity = ""
    communityJoinSupported = supportsJoin
    if (!supportsJoin) { loseInvite(); loseRoomAction(); clearJoin(); roomManageSupported = false }
    else {
      if (!sameProjection(joinSetup, join)) joinSetup = join
      if (!sameProjection(openRooms, open.rooms)) openRooms = open.rooms
      openRoomsState = open.state
      openRoomsCategory = open.category
      if (!sameProjection(roomAction, action)) roomAction = action
      roomManageSupported = supportsRoomManage
      if (!sameProjection(roomDetail, detail)) roomDetail = detail
      noteRoomChange(action)
      if (frame.type === "status" && inviteState === "sending" && frame.id === inviteRequestId && frame.instanceId === inviteInstance) {
        inviteTimeout.stop()
        inviteState = "idle"
        inviteRequestId = ""
        // Nothing to read: the user already chose to redeem, so claim at once.
        if (inviteRequestKind === "claim" && join.state === "policy" && join.joinPolicy === null) acceptInvite()
      }
      if (roomActionLocal === "sending" && action.requestId === roomActionRequestId && action.state !== "idle") {
        roomActionTimeout.stop()
        roomActionLocal = "idle"
      }
      if (action.requestId === roomActionRequestId && ["rejected", "unknown"].indexOf(action.state) !== -1) joinTarget = ""
    }
    communitiesSupported = supportsCommunities
    if (!supportsCommunities) { loseCommunity(); clearCommunities(); pendingInviteInput = "" }
    else {
      if (!sameProjection(communities, shownCommunities)) communities = shownCommunities
      if (frame.type === "status" && communityLocal === "sending" && frame.id === communityRequestId && frame.instanceId === communityInstance) {
        communityTimeout.stop()
        communityLocal = "idle"
        communityRequestId = ""
        communityNotice = shownCommunities.notice || ""
        // First setup with an invite link: redeem it once the identity exists.
        if (communityRequestKind === "join" && shownCommunities.pendingInvite) pendingInviteInput = communityRequestInput
        communityRequestDone(communityRequestKind, true)
        var arrived = activeCommunity
        if (arrived && identity !== "" && !shownCommunities.pendingInvite) {
          if (communityRequestKind === "switch") communityArrived("switched", arrived.name)
          else if (communityRequestKind === "join") {
            if (communityRelaysBefore.indexOf(arrived.relay) !== -1) communityArrived("switched", arrived.name)
            else if (joinSetup.state === "policy") arrivalAwaitingTerms = arrived.relay
            else communityArrived("joined", arrived.name)
          }
        }
      }
      // A join that showed terms: arrived once they are accepted and claimed.
      if (arrivalAwaitingTerms !== "") {
        if (relay !== arrivalAwaitingTerms || joinSetup.state === "failed" || joinSetup.state === "idle") arrivalAwaitingTerms = ""
        else if (joinSetup.state === "joined" && activeCommunity) {
          arrivalAwaitingTerms = ""
          communityArrived("joined", activeCommunity.name)
        }
      }
    }
    // The invite saved during first setup is redeemed as soon as it can be.
    if (pendingInviteInput !== "" && canRedeemInvite && !inviteBusy) {
      var pendingInput = pendingInviteInput
      pendingInviteInput = ""
      redeemInvite(pendingInput)
    }
    inviteMintSupported = supportsMint
    if (!supportsMint) { loseMint(); clearInvites() }
    else {
      if (minted.code !== invites.code) inviteCopied = ""
      if (!sameProjection(invites, minted)) invites = minted
      if (frame.type === "status" && mintState === "sending" && frame.id === mintRequestId && frame.instanceId === mintInstance) {
        mintTimeout.stop()
        mintState = "idle"
        mintRequestId = ""
      }
    }
    userStatusSupported = supportsStatus
    if (!supportsStatus || state.connection !== "authenticated") {
      // Without a session nothing about the status is shown or pending.
      loseStatusRequest()
      if (userStatus.state !== "unavailable" || userStatus.mine !== null) userStatus = {state: "unavailable", mine: null, category: null}
    } else {
      if (!sameProjection(userStatus, shownStatus)) userStatus = shownStatus
      if (frame.type === "status" && statusRequestState === "sending" && frame.id === statusRequestId && frame.instanceId === statusRequestInstance) {
        statusTimeout.stop()
        statusRequestState = "idle"
        statusRequestId = ""
      }
    }
    presenceSupported = supportsPresence
    if (!supportsPresence || state.connection !== "authenticated") {
      // Without a session nothing is shown; the preference is sent again on connect.
      if (presence.state !== "unavailable" || presence.published !== null || presence.peers.length) clearPresence()
      presenceSentKey = ""
    } else {
      if (!sameProjection(presence, shownPresence)) presence = shownPresence
      if (frame.type === "status" && frame.id === presenceRequestId) presenceRequestId = ""
    }
    attachmentsSupported = supportsAttachments
    applyTransfers(transfers, frame)
    applyDelivery(delivery)
    applyActionDelivery(delivery)
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
    selectJoinedRoom()
    // A connected identity with no rooms sees what it can join at once, once per
    // community (a new generation is another community or configuration).
    if (connection !== "authenticated" || generationBefore !== generation) openRoomsRequested = false
    if (openRoomsAvailable && noRoomsJoined && openRoomsState === "unavailable" && openRoomsCategory === ""
        && !openRoomsRequested) { openRoomsRequested = true; refreshOpenRooms() }
    // On connect (and whenever the preference or idleness changed meanwhile).
    syncPresence()
    return true
  }
  property bool openRoomsRequested: false
  function send(kind, roomId, rootId) {
    if (!sessionFailed && bridge.running && instanceId !== "") {
      requestSequence++
      var request = { version: 1, id: "ui-" + requestSequence, type: kind }
      if (kind === "fetch_recent") { request.roomId = roomId; pendingHistoryRequestId = request.id }
      if (kind === "fetch_thread") { request.roomId = roomId; request.rootId = rootId; pendingThreadRequestId = request.id }
      if (kind === "fetch_recipients") { request.roomId = roomId; pendingRecipientsRequestId = request.id }
      if (kind === "fetch_older") { request.roomId = roomId; pendingOlderRequestId = request.id }
      // The first argument is the search text here, not a room.
      if (kind === "search_people") { request.query = roomId; peopleRequestId = request.id }
      if (kind === "fetch_room_detail") request.roomId = roomId
      bridge.write(JSON.stringify(request) + "\n")
    }
  }
  function retry() {
    if (sampleMode) return
    if (agentService.autoConnect) agentService.retry()
    if (!sessionFailed && bridge.running && instanceId !== "") send("retry_connection")
    else startBridge()
  }
  Timer {
    id: bridgeRestart
    onTriggered: if (root.autoRestart && root.sessionFailed && !bridge.running) root.startBridge()
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
    // An agent joined or left rooms: reload the open room's members so the
    // @ picker and author names know it without leaving the room.
    onMembershipChanged: if (root.recipientsSupported && root.selectedRoomId !== "" && root.connection === "authenticated") root.send("fetch_recipients", root.selectedRoomId)
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
    // Fixed argv built by notifyActivity/sendNextNotification: no shell, credentials
    // or executable supplied by events.
    stdout: SplitParser { onRead: function(line) {} }
    stderr: SplitParser { onRead: function(line) {} }
    onRunningChanged: if (!running) root.sendNextNotification()
  }
  Timer {
    id: handshake
    interval: 5000
    onTriggered: root.fail("handshake_timeout")
  }
  Timer {
    id: peopleLostTimer
    // Beyond the helper's own 15-second bound: no answer is shown as failed.
    interval: 20000
    onTriggered: root.peopleLost = true
  }
  Timer {
    id: dmOpenTimeout
    // Beyond the helper's own 15-second bound: a missing answer is unknown, never refused.
    interval: 30000
    onTriggered: root.loseDmOpen()
  }
  Timer {
    id: inviteTimeout
    // Beyond the helper's own 60-second bound for the three invite requests.
    interval: 70000
    onTriggered: { if (root.inviteState === "sending") { root.inviteState = "failed"; root.inviteCategory = "setup_busy" }; root.inviteRequestId = "" }
  }
  Timer {
    id: communityTimeout
    // Beyond the helper's own 120-second bound for a join or a leave.
    interval: 130000
    onTriggered: {
      if (root.communityLocal === "sending") { root.communityLocal = "failed"; root.communityCategory = "join_busy"; root.communityRequestDone(root.communityRequestKind, false) }
      root.communityRequestId = ""
    }
  }
  Timer {
    id: statusTimeout
    // The helper answers at once; the relay's OK arrives in the status view.
    interval: 30000
    onTriggered: { if (root.statusRequestState === "sending") { root.statusRequestState = "failed"; root.statusRequestCategory = "relay_unavailable" }; root.statusRequestId = "" }
  }
  Timer {
    id: mintTimeout
    // Beyond the helper's own 60-second bound for one HTTP request.
    interval: 70000
    onTriggered: { if (root.mintState === "sending") { root.mintState = "failed"; root.mintCategory = "setup_busy" }; root.mintRequestId = "" }
  }
  Timer {
    id: thumbnailRetry
    // A full helper preview queue is asked again a little later.
    interval: 3000
    running: root.thumbnailBackoff
    onTriggered: { root.thumbnailBackoff = false; root.pumpThumbnails() }
  }
  Timer {
    id: transferTimeout
    // The helper answers transfer requests at once; its outcome is in the status view.
    interval: 15000
    running: root.downloadLocal === "sending" || root.uploadLocal === "sending" || root.thumbnailRequestId !== ""
    onTriggered: {
      if (root.downloadLocal === "sending") { root.downloadLocal = "failed"; root.downloadLocalCategory = "setup_busy"; root.downloadRequestId = "" }
      if (root.uploadLocal === "sending") { root.uploadLocal = "failed"; root.uploadLocalCategory = "setup_busy"; root.uploadRequestId = "" }
      root.thumbnailRequestId = ""
      root.thumbnailRequestHash = ""
      root.pumpThumbnails()
    }
  }
  Timer {
    id: createPoll
    interval: 2000
    repeat: true
    onTriggered: {
      if (root.joinTarget === "" || root.createPolls <= 0 || !root.openRoomsAvailable) { stop(); return }
      root.createPolls--
      root.send("refresh_rooms")
    }
  }
  Timer {
    // The relay refreshes its roster and metadata snapshots a moment after it
    // accepts a change; read them once more.
    id: detailRecheck
    interval: 1500
    // The relay's member snapshot may lag the accepted change: read both again.
    onTriggered: { root.refreshRoomDetail(); root.refreshRecipientsAfterChange(); root.recheckRecipients = false }
  }
  Timer {
    id: roomActionTimeout
    // The helper answers at once; the relay's OK arrives in the status view.
    interval: 30000
    onTriggered: root.loseRoomAction()
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
  Timer {
    id: actionTimeout
    interval: 30000
    onTriggered: root.loseAction()
  }
  // An accepted action's note stays briefly; failures and unknowns stay until the next action.
  Timer {
    id: actionClear
    interval: 4000
    onTriggered: if (root.actionState === "acknowledged") root.clearAction()
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
      root.loseInvite()
      root.loseRoomAction()
      root.communityJoinSupported = false
      root.clearJoin()
      root.loseMint()
      root.inviteMintSupported = false
      root.clearInvites()
      root.clearUserStatus()
      root.userStatusSupported = false
      root.clearPresence()
      root.presenceSupported = false
      root.losePendingDelivery()
      root.loseDmOpen()
      root.dmOpenSupported = false
      root.clearPeople()
      root.peopleSupported = false
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
      root.helperReconnecting = false
      if (!root.category) root.category = "helper_unavailable"
      root.scheduleBridgeRestart()
    }
  }
}
