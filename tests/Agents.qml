// Synthetic stdio helper, agent service and status frames only; no relay, keys or units.
import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as Buzz
import "plugin/Identicon.js" as Identicon
import "plugin/AnsiArt.js" as AnsiArt

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  readonly property string roomA: "11111111-1111-4111-8111-111111111111"
  readonly property string roomB: "22222222-2222-4222-8222-222222222222"
  readonly property string agentId: "33333333-3333-4333-8333-333333333333"
  readonly property string createdId: "55555555-5555-4555-9555-555555555555"
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  // Frames fed directly: validation and a service without `agent_manager`.
  Buzz.Service { id: offline; autoConnect: false }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 640
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 400
    Buzz.PanelContent { id: offlineView; anchors.fill: parent; service: offline }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  FileView { id: avatarsFile; path: Quickshell.env("XDG_STATE_HOME") + "/omarchy-buzz/avatars.json"; blockLoading: true; blockWrites: true; printErrors: false }
  readonly property string pastedArt: "+----------+\n|  fixture |\n+----------+\n4\n5\n6"
  // A fresh service object reads the avatar file on creation; nothing connects.
  function freshArt(id) {
    var fresh = Qt.createQmlObject('import "plugin" as Buzz\nBuzz.Service { autoConnect: false }', test, "freshService")
    var art = fresh.agents.avatarArtFor(id)
    fresh.destroy()
    return art
  }
  // A new reader each time: the file on disk, not a cached copy.
  function readStore() {
    var reader = Qt.createQmlObject('import Quickshell.Io\nFileView { blockLoading: true; printErrors: false }', test, "storeReader")
    reader.path = avatarsFile.path
    var text = reader.text()
    reader.destroy()
    return text
  }
  function avatarCases() {
    var editor = findNamed(view, "buzzAgentEditor", [])[0]
    var field = one(view, "buzzAgentAvatarArt")
    var header = shown(editor, "buzzAvatar").filter(function(item) { return item !== editor.artPreview })
    var rowAvatar = shown(view, "buzzAvatar").filter(function(item) { return item.name === "Fixture agent" && item !== header[0] })
    if (header.length !== 1 || header[0].key !== "b".repeat(64) || header[0].text !== Identicon.glyph("b".repeat(64))
        || rowAvatar.length !== 1 || rowAvatar[0].key !== "b".repeat(64))
      throw new Error("Enrolled agent avatars are not its public key's identicon")
    if (field.text !== "" || editor.artPreview.usesArt || editor.artPreview.text !== Identicon.glyph("b".repeat(64)))
      throw new Error("Editor without art does not preview the identicon")
    // Pasting is held to 6 x 12 without control characters, on the field itself.
    field.text = "+----------+XYZ\n|  fixture\u0007 |\n+----------+\n4\n5\n6\n7\n8"
    if (field.text !== pastedArt || editor.draftArt !== pastedArt || editor.artPreview.text !== pastedArt)
      throw new Error("Pasted art not clipped or previewed: " + JSON.stringify(field.text))
    if (!one(view, "buzzAgentSave").enabled) throw new Error("Art change cannot be saved")
    var before = requests().length
    one(view, "buzzAgentSave").clicked()
    if (requests().length !== before || agents().requestState !== "idle")
      throw new Error("Saving art alone sent an agent service request")
    if (service.agents.avatarArtFor(agentId) !== pastedArt || header[0].text !== pastedArt || rowAvatar[0].text !== pastedArt
        || service.agents.avatarArtForKey("b".repeat(64)) !== pastedArt || one(view, "buzzAgentSave").enabled)
      throw new Error("Saved art not shown in the header, list and messages")
    avatarsFile.reload()
    var stored = JSON.parse(avatarsFile.text())
    if (Object.keys(stored).sort().join(",") !== "avatars,version" || stored.version !== 1
        || JSON.stringify(stored.avatars) !== JSON.stringify({"33333333-3333-4333-8333-333333333333": pastedArt}))
      throw new Error("Avatar file holds unexpected data: " + avatarsFile.text())
    if (freshArt(agentId) !== pastedArt) throw new Error("Art not restored by a fresh service object")
    // Colored art from a file (reading the file is covered by --ansi-art): previewed,
    // saved locally, shown colored in the header and list; Clear returns to typed art.
    one(view, "buzzAgentAvatarLoad").clicked()
    var loader = one(view, "buzzAgentAvatarPath").parent
    var colored = AnsiArt.sanitize("\x1b[38;2;200;40;40m@@\n\x1b[38;5;33m##")
    loader.artLoaded(colored)
    if (editor.draftArt !== colored || shown(view, "buzzAgentAvatarArt").length || !editor.artPreview.usesColor
        || !one(view, "buzzAgentSave").enabled)
      throw new Error("Loaded colored art not previewed as a draft")
    one(view, "buzzAgentSave").clicked()
    if (service.agents.avatarArtFor(agentId) !== colored || !header[0].usesColor || !rowAvatar[0].usesColor
        || header[0].implicitWidth !== editor.artPreview.implicitWidth || freshArt(agentId) !== colored)
      throw new Error("Colored agent art not saved, shown or restored")
    if (service.agents.avatarBrightnessFor(agentId) !== 1.5 || editor.draftBrightness !== 1.5 || header[0].brightness !== 1.5)
      throw new Error("Colored agent art did not start at brightness 1.5")
    // Brightness is a draft: the preview follows at once, Save keeps it.
    var beforeThumb = JSON.stringify(editor.artPreview.thumbnail)
    one(view, "buzzAgentAvatarBrightnessUp").clicked()
    if (editor.draftBrightness !== 1.75 || editor.artPreview.brightness !== 1.75 || JSON.stringify(editor.artPreview.thumbnail) === beforeThumb
        || header[0].brightness !== 1.5 || !one(view, "buzzAgentSave").enabled)
      throw new Error("Brightness change not previewed as a draft")
    one(view, "buzzAgentSave").clicked()
    if (service.agents.avatarBrightnessFor(agentId) !== 1.75 || header[0].brightness !== 1.75 || rowAvatar[0].brightness !== 1.75
        || JSON.stringify(JSON.parse(readStore()).avatars[agentId]) !== JSON.stringify({art: colored, brightness: 1.75}))
      throw new Error("Agent brightness not saved")
    one(view, "buzzAgentAvatarPathClear").clicked()
    if (editor.draftArt !== "" || one(view, "buzzAgentAvatarArt").text !== "") throw new Error("Clear did not return to typed art")
    one(view, "buzzAgentAvatarArt").text = pastedArt
    one(view, "buzzAgentSave").clicked()
    one(view, "buzzAgentAvatarLoad").clicked()
    if (service.agents.avatarArtFor(agentId) !== pastedArt || shown(view, "buzzAgentAvatarPath").length)
      throw new Error("Typed art not restored after colored art")
    // Damaged or out-of-contract files fail closed.
    var damaged = ["{broken", JSON.stringify({version: 2, avatars: {}}),
      JSON.stringify({version: 1, avatars: {"33333333-3333-4333-8333-333333333333": "bell\u0007"}}),
      JSON.stringify({version: 1, avatars: {"not-an-id": "x"}}),
      JSON.stringify({version: 1, avatars: {"33333333-3333-4333-8333-333333333333": "1\n2\n3\n4\n5\n6\n7"}}),
      JSON.stringify({version: 1, avatars: {"33333333-3333-4333-8333-333333333333": "x"}, extra: true})]
    damaged.forEach(function(text, index) {
      avatarsFile.setText(text)
      if (freshArt(agentId) !== "") throw new Error("Damaged avatar file " + index + " was trusted")
    })
    avatarsFile.setText(JSON.stringify({version: 1, avatars: stored.avatars}) + "\n")
  }
  function agents() { return service.agents }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(item, name) {
    return findNamed(item, name, []).filter(function(entry) {
      for (var node = entry; node; node = node.parent) if (!node.visible) return false
      return true
    })
  }
  function one(item, name) {
    var found = shown(item, name)
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  function requests() {
    record.reload()
    return JSON.parse(record.text()).requests
  }
  function persona(overrides) {
    var value = {id: agentId, name: "Fixture agent", description: "", instructions: "", harness: "codex", model: "",
      acpCommand: "buzz-acp", rooms: [roomA], respondTo: "owner-only", workspace: "/home/fixture/w", identity: "b".repeat(64),
      enrolled: true, unit: "inactive", startAtLogin: false, answersDms: false, published: true, lastError: null}
    return Object.assign(value, overrides || {})
  }
  function agentFrame(type, capabilities, status) {
    return JSON.stringify({version: 1, type: type, id: null, instanceId: "offline-agents", capabilities: capabilities, status: status})
  }
  function validStatus(agents, pending) {
    return {harnesses: [{id: "codex", bundle: "ready", signedIn: true}], agents: agents || [persona()], pending: pending || null}
  }
  function helperFrame() {
    var status = {generation: 1, connection: "authenticated", category: null, identity: "a".repeat(64), relay: "wss://fixture.example/",
      catalog: {state: "ready", category: null, rooms: [{id: roomA, name: "Fixture", description: "", kind: "stream", participants: [], hidden: false}]},
      history: {state: "snapshot", roomId: roomA, rows: [], hasMore: false, category: "history_completeness_unknown"}}
    return JSON.stringify({version: 1, type: "hello", instanceId: "offline-fixture", generation: 1,
      capabilities: ["connection_status", "room_catalog", "room_history"], status: status})
  }
  function validationCases() {
    var agents = offline.agents
    offline.beginSession()
    if (!offline.acceptFrame(helperFrame()) || offline.connection !== "authenticated") throw new Error("Offline helper frame refused")
    // A connected helper without the agent service shows only the one-line caption.
    if (shown(offlineView, "buzzAgentsHeading").length || one(offlineView, "buzzAgentsUnavailable").text !== "Agent manager unavailable")
      throw new Error("Unavailable agent service not captioned")
    agents.beginSession()
    if (!agents.acceptFrame(agentFrame("hello", ["agent_manager"], validStatus())) || !agents.available
        || one(offlineView, "buzzAgentsHeading").text !== "Agents" || shown(offlineView, "buzzAgentsUnavailable").length)
      throw new Error("Valid agent service hello refused")
    // A stale bundle is a documented state; its refusal category is known.
    if (!agents.acceptFrame(agentFrame("status", ["agent_manager"], {harnesses: [{id: "codex", bundle: "stale", signedIn: true}],
        agents: [persona()], pending: {requestId: "00000000-0000-4000-8000-000000000008", type: "start_agent", state: "failed", category: "bundle_stale"}}))
        || !agents.bundleStale("codex") || agents.startAgent(test.agentId)
        || agents.categorySentence("bundle_stale") !== "The harness bundle needs a refresh.")
      throw new Error("Stale bundle status refused")
    // The service's own pending work refuses every mutation.
    var working = {requestId: "00000000-0000-4000-8000-000000000009", type: "start_agent", state: "working", category: null}
    if (!agents.acceptFrame(agentFrame("status", ["agent_manager"], validStatus(null, working))) || agents.canMutate
        || agents.statusLabel !== "Starting agent elsewhere…")
      throw new Error("Service pending work did not block mutations")
    // Without `agent_manager` the section is hidden and nothing can be requested.
    agents.beginSession()
    if (!agents.acceptFrame(agentFrame("hello", [], {})) || agents.capabilitySupported || agents.available
        || agents.agents.length || shown(offlineView, "buzzAgentsHeading").length || shown(offlineView, "buzzNewAgent").length
        || shown(offlineView, "buzzAgentRow").length || shown(offlineView, "buzzAgentsUnavailable").length !== 1
        || offlineView.openAgentEditor("") || agents.startAgent(test.agentId))
      throw new Error("Missing agent_manager capability still offered the section")
    var refused = [
      "{broken",
      agentFrame("hello", ["agent_manager", "teams"], validStatus()),
      agentFrame("hello", ["agent_manager", "agent_manager"], validStatus()),
      agentFrame("status", ["agent_manager"], validStatus()),
      agentFrame("hello", ["agent_manager"], {harnesses: [], agents: []}),
      agentFrame("hello", ["agent_manager"], Object.assign(validStatus(), {extra: true})),
      agentFrame("hello", ["agent_manager"], {harnesses: [{id: "goose", bundle: "ready", signedIn: true}], agents: [], pending: null}),
      agentFrame("hello", ["agent_manager"], {harnesses: [{id: "codex", bundle: "ready", signedIn: "yes"}], agents: [], pending: null}),
      agentFrame("hello", ["agent_manager"], {harnesses: [{id: "codex", bundle: "outdated", signedIn: true}], agents: [], pending: null}),
      agentFrame("hello", ["agent_manager"], {harnesses: [{id: "codex", bundle: "ready", signedIn: true}, {id: "codex", bundle: "missing", signedIn: null}], agents: [], pending: null}),
      agentFrame("hello", ["agent_manager"], validStatus([Object.assign(persona(), {token: "x"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({unit: "running"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({answersDms: "yes"})])),
      agentFrame("hello", ["agent_manager"], validStatus([(function() { var p = persona(); delete p.answersDms; return p })()])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({id: "33333333-3333-1333-8333-333333333333"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({name: ""})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({name: "Evil\u202eagent"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({description: "x".repeat(257)})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({instructions: "x".repeat(16385)})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({model: "gpt 5"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({acpCommand: "sh -c true"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({rooms: []})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({rooms: [roomA, roomA]})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({respondTo: "everyone"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({workspace: "relative/path"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({identity: null})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona({lastError: "segfault"})])),
      agentFrame("hello", ["agent_manager"], validStatus([persona(), persona()])),
      agentFrame("hello", ["agent_manager"], validStatus(null, {requestId: "00000000-0000-4000-8000-000000000009", type: "start_agent", state: "failed", category: null})),
      agentFrame("hello", ["agent_manager"], validStatus(null, {requestId: "00000000-0000-4000-8000-000000000009", type: "rm", state: "working", category: null})),
      agentFrame("hello", ["agent_manager"], validStatus(null, {requestId: "ui-1", type: "start_agent", state: "working", category: null}))
    ]
    refused.forEach(function(line, index) {
      agents.beginSession()
      if (agents.acceptFrame(line) || !agents.sessionFailed || agents.available || agents.agents.length)
        throw new Error("Malformed agent frame " + index + " was accepted")
    })
    // Errors must name a known category and this session's instance.
    var badErrors = [
      {version: 1, type: "error", id: "00000000-0000-4000-8000-000000000009", instanceId: "offline-agents", category: "oops"},
      {version: 1, type: "error", id: "00000000-0000-4000-8000-000000000009", instanceId: "other", category: "agent_busy"},
      {version: 1, type: "error", id: "00000000-0000-4000-8000-000000000009", instanceId: "offline-agents", category: "agent_busy", detail: "x"}
    ]
    badErrors.forEach(function(value, index) {
      agents.beginSession()
      if (!agents.acceptFrame(agentFrame("hello", ["agent_manager"], validStatus())))
        throw new Error("Valid hello refused before error case " + index)
      if (agents.acceptFrame(JSON.stringify(value)) || !agents.sessionFailed)
        throw new Error("Malformed error frame " + index + " was accepted")
    })
  }
  Timer {
    interval: 50
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 240) { console.error("Agents fixture timed out at stage " + test.stage); Qt.exit(1); return }
        var agents = service.agents
        if (test.stage === -1) { service.retry(); agents.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.connection === "authenticated" && service.catalogState === "ready" && agents.available
            && service.historyState === "snapshot") {
          if (test.one(view, "buzzAgentsHeading").text !== "Agents" || test.shown(view, "buzzAgentsUnavailable").length)
            throw new Error("Agents section not shown for a connected agent service")
          var rows = test.shown(view, "buzzAgentRow")
          if (rows.length !== 1 || rows[0].text !== "Fixture agent · stopped") throw new Error("Agent row wrong: " + rows.map(function(r) { return r.text }))
          test.one(view, "buzzNewAgent")
          if (test.shown(view, "buzzAgentEditor").length) throw new Error("Editor open before it was asked for")
          rows[0].clicked()
          test.one(view, "buzzAgentEditor")
          if (test.shown(view, "buzzHistoryScroll").length) throw new Error("Room view still shown beside the editor")
          if (test.one(view, "buzzAgentName").text !== "Fixture agent" || test.one(view, "buzzAgentInstructions").text !== "Answer briefly.")
            throw new Error("Editor did not load the fixture's agent")
          var rooms = test.shown(view, "buzzAgentRoom")
          if (rooms.length !== 2 || !rooms[0].checked || rooms[1].checked) throw new Error("Room choices are not the verified rooms")
          // The Codex bundle's launcher is stale: Start waits for a refresh.
          var start = test.one(view, "buzzAgentStart")
          if (start.enabled || start.tooltipText !== "Harness bundle needs a refresh"
              || test.one(view, "buzzAgentStatus").text !== "Harness bundle needs a refresh"
              || test.one(view, "buzzRefreshBundle").text !== "Refresh bundle" || agents.startAgent(test.agentId)
              || test.shown(view, "buzzAgentHarness").filter(function(b) { return b.harnessId === "codex" })[0].text !== "Codex · needs refresh")
            throw new Error("Stale harness bundle not shown or Start not held")
          if (test.shown(view, "buzzAgentEnroll").length || test.shown(view, "buzzAgentStop").length || test.shown(view, "buzzAgentSignIn").length)
            throw new Error("Enrolled, stopped, signed-in agent offered the wrong actions")
          if (test.one(view, "buzzAgentSave").enabled) throw new Error("Save enabled with nothing changed")
          test.avatarCases()
          // Delete asks for a second click and sends nothing on the first.
          var remove = test.one(view, "buzzAgentDelete")
          remove.clicked()
          if (remove.text !== "Confirm delete" || test.requests().some(function(r) { return r.type !== "subscribe" })) throw new Error("Delete did not wait for confirmation")
          test.one(view, "buzzAgentBack").clicked()
          if (test.shown(view, "buzzAgentEditor").length || !test.shown(view, "buzzHistoryScroll").length)
            throw new Error("Back to rooms did not restore the room view")
          test.one(view, "buzzNewAgent").clicked()
          if (test.one(view, "buzzAgentTitle").text !== "New agent" || test.one(view, "buzzAgentName").text !== "")
            throw new Error("New agent editor is not empty")
          if (test.one(view, "buzzAgentSave").enabled) throw new Error("Create enabled for an empty agent")
          var newEditor = test.findNamed(view, "buzzAgentEditor", [])[0]
          if (test.one(view, "buzzAgentAvatarArt").text !== "" || newEditor.artPreview.usesArt
              || newEditor.artPreview.text !== Identicon.neutralGlyph())
            throw new Error("New agent editor does not preview the identicon before art is pasted")
          test.one(view, "buzzAgentName").text = "Rejected agent"
          var harness = test.shown(view, "buzzAgentHarness").filter(function(b) { return b.harnessId === "claude-code" })[0]
          harness.clicked()
          if (test.one(view, "buzzAgentSignIn").text !== "Sign in to Claude Code") throw new Error("Sign-in not offered for a signed-out harness")
          if (test.one(view, "buzzAgentSave").enabled) throw new Error("Create enabled without a room")
          test.shown(view, "buzzAgentRoom")[1].click()
          if (JSON.stringify(test.findNamed(view, "buzzAgentEditor", [])[0].draftRooms) !== JSON.stringify([test.roomB]))
            throw new Error("Room checkbox did not choose the room")
          test.one(view, "buzzAgentWorkspace").text = "relative/path"
          if (test.one(view, "buzzAgentSave").enabled || test.one(view, "buzzAgentProblem").text !== "Workspace must be an absolute path.")
            throw new Error("Relative workspace accepted")
          test.one(view, "buzzAgentWorkspace").text = ""
          if (!test.one(view, "buzzAgentSave").enabled) throw new Error("Create disabled for a valid agent")
          // Local rules refuse what the contract forbids before anything is written.
          var base = {name: "x", description: "", instructions: "", harness: "codex", model: "", rooms: [test.roomA], respondTo: "owner-only", workspace: "", answersDms: false}
          if (agents.createAgent(Object.assign({}, base, {rooms: ["99999999-9999-4999-8999-999999999999"]}))
              || agents.createAgent(Object.assign({}, base, {name: "bad\u0007"})) || agents.createAgent(Object.assign({}, base, {model: "a b"}))
              || agents.createAgent(Object.assign({}, base, {command: "sh"})) || agents.createAgent(Object.assign({}, base, {harness: "goose"}))
              || agents.createAgent(Object.assign({}, base, {answersDms: "yes"})) || agents.updateAgent(test.agentId, {answersDms: 1})
              || agents.updateAgent(test.agentId, {}) || agents.updateAgent(test.agentId, {workspace: ""}) || agents.signIn("codex"))
            throw new Error("Invalid request passed local validation")
          test.one(view, "buzzAgentSave").clicked()
          if (agents.requestState !== "working") throw new Error("Create was not sent")
          test.stage = 1
        } else if (test.stage === 1 && agents.requestState === "failed") {
          if (agents.requestCategory !== "agent_invalid"
              || test.one(view, "buzzAgentStatus").text !== "Creating agent · failed. The agent service refused these settings.")
            throw new Error("Refused create not shown: " + test.one(view, "buzzAgentStatus").text)
          test.one(view, "buzzAgentName").text = "Created agent"
          test.one(view, "buzzAgentAvatarArt").text = "[new]"
          test.one(view, "buzzAgentSave").clicked()
          test.stage = 2
        } else if (test.stage === 2 && agents.requestState === "done" && view.agentEditorId === test.createdId) {
          if (test.shown(view, "buzzAgentRow").length !== 2 || test.one(view, "buzzAgentTitle").text !== "Created agent"
              || test.one(view, "buzzAgentStatus").text !== "Creating agent · done")
            throw new Error("Created agent not listed or opened")
          if (agents.avatarArtFor(test.createdId) !== "[new]" || agents.avatarArtFor(test.agentId) !== test.pastedArt
              || test.freshArt(test.createdId) !== "[new]" || test.one(view, "buzzAgentAvatarArt").text !== "[new]")
            throw new Error("Art pasted before create was not kept for the new agent")
          test.one(view, "buzzAgentEnroll")
          if (test.shown(view, "buzzAgentStart").length || test.shown(view, "buzzAgentRow")[1].text !== "Created agent · not enrolled")
            throw new Error("Unenrolled agent offered Start")
          test.shown(view, "buzzAgentRow")[0].clicked()
          // Answering direct messages: a toggle saved as one changed field.
          var dms = test.one(view, "buzzAgentDms")
          if (dms.text !== "Answers direct messages: off" || test.shown(view, "buzzAgentRestartNote").length)
            throw new Error("Direct message toggle not loaded off")
          dms.clicked()
          if (dms.text !== "Answers direct messages: on" || !test.one(view, "buzzAgentSave").enabled)
            throw new Error("Direct message toggle did not change the draft")
          test.one(view, "buzzAgentSave").clicked()
          test.stage = 20
        } else if (test.stage === 20 && agents.requestState === "done" && agents.agent(test.agentId).answersDms === true) {
          if (test.one(view, "buzzAgentDms").text !== "Answers direct messages: on" || test.one(view, "buzzAgentSave").enabled)
            throw new Error("Saved direct message choice not shown")
          if (test.one(view, "buzzAgentStart").enabled) throw new Error("Start enabled for a stale bundle")
          test.one(view, "buzzRefreshBundle").clicked()
          if (agents.requestState !== "working" || agents.refreshBundle("codex")) throw new Error("Refresh was not sent once")
          test.stage = 21
        } else if (test.stage === 21 && agents.requestState === "done" && agents.harness("codex").bundle === "ready") {
          if (test.one(view, "buzzAgentStatus").text !== "Refreshing harness bundle · done" || test.shown(view, "buzzRefreshBundle").length
              || !test.one(view, "buzzAgentStart").enabled || test.one(view, "buzzAgentStart").tooltipText !== "")
            throw new Error("Refreshed bundle did not release Start: " + test.one(view, "buzzAgentStatus").text)
          if (agents.refreshBundle("codex")) throw new Error("Refresh offered for a ready bundle")
          test.one(view, "buzzAgentStart").clicked()
          if (agents.requestState !== "working") throw new Error("Start was not sent")
          // One mutating request at a time: every other action is refused meanwhile.
          if (agents.startAgent(test.agentId) || agents.stopAgent(test.agentId) || agents.enrollAgent(test.createdId)
              || agents.deleteAgent(test.agentId, false) || agents.setStartAtLogin(test.agentId, true) || agents.signIn("claude-code")
              || agents.createAgent({name: "y", description: "", instructions: "", harness: "codex", model: "", rooms: [test.roomA], respondTo: "owner-only", workspace: ""})
              || test.one(view, "buzzAgentDelete").enabled)
            throw new Error("A second mutating request was allowed while one is pending")
          test.stage = 3
        } else if (test.stage === 3 && agents.requestState === "done" && agents.agent(test.agentId).unit === "active") {
          if (test.shown(view, "buzzAgentRow")[0].text !== "Fixture agent · running" || test.shown(view, "buzzAgentStart").length)
            throw new Error("Running agent not shown")
          // Like rooms, the choice changes how the agent is launched: saving stops it.
          test.one(view, "buzzAgentDms").clicked()
          if (test.one(view, "buzzAgentRestartNote").text.indexOf("stops the running agent") === -1)
            throw new Error("Restart note not shown for a running agent")
          test.one(view, "buzzAgentDms").clicked()
          if (test.shown(view, "buzzAgentRestartNote").length) throw new Error("Restart note shown without a change")
          var sent = test.requests()
          if (sent.map(function(r) { return r.type }).join(",") !== "subscribe,create_agent,create_agent,update_agent,refresh_bundle,start_agent") return
          var created = sent[2].fields
          if (JSON.stringify(created) !== JSON.stringify({name: "Created agent", description: "", instructions: "", harness: "claude-code",
              model: "", rooms: [test.roomB], respondTo: "owner-only", workspace: "", answersDms: false, startAtLogin: false, acpCommand: "buzz-acp"})
              || sent[1].id === sent[2].id || JSON.stringify(sent[3].fields) !== JSON.stringify({answersDms: true})
              || sent[4].harness !== "codex" || Object.keys(sent[4]).sort().join(",") !== "harness,id,instanceId,type,version"
              || sent[5].agentId !== test.agentId)
            throw new Error("Create or start request fields wrong: " + JSON.stringify(sent))
          test.one(view, "buzzAgentStop").clicked()
          test.stage = 4
        } else if (test.stage === 4 && agents.sessionFailed) {
          // The fixture answered Stop with an undocumented unit state.
          if (agents.category !== "invalid_response" || agents.agents.length || test.shown(view, "buzzAgentsHeading").length
              || test.shown(view, "buzzAgentEditor").length || test.one(view, "buzzAgentsUnavailable").text !== "Agent manager unavailable"
              || !test.shown(view, "buzzHistoryScroll").length || agents.requestState !== "unknown")
            throw new Error("Malformed status did not end the agent session")
          test.validationCases()
          console.log("PASS: Agents section lists the service's agents, edits, creates, starts and shows refusals; one request at a time; malformed frames end the session; no agent_manager hides the section; pasted avatar art is clipped, previewed, kept locally and restored, colored art from a file is previewed, brightened, saved and cleared, damaged avatar files fail closed; the direct message toggle round-trips and notes the restart; a stale harness bundle holds Start until Refresh bundle makes it ready")
          Qt.quit()
        }
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
