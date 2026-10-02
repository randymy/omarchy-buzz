// Header navigation against synthetic helper and agent service frames only: no
// helper process, relay, keys or units. The header shows Buzz, the room and ×
// on the room view and "← Back to rooms" with the view's title on every view
// that replaces it; Escape closes a menu, then the thread, then the view, and
// only then Buzz. BUZZ_NAVIGATION_CAPTURE_DIR optionally saves PNGs.
import QtQuick
import QtTest
import Quickshell
import "plugin" as Buzz

ShellRoot {
  id: test
  property real stage: 0
  property int ticks: 0
  property int closeRequests: 0
  readonly property string me: "b".repeat(64)
  readonly property string other: "a".repeat(64)
  readonly property string room: "11111111-1111-4111-8111-111111111111"
  readonly property string agentId: "33333333-3333-4333-8333-333333333333"
  readonly property string rootId: "1".repeat(64)
  readonly property string captureDir: Quickshell.env("BUZZ_NAVIGATION_CAPTURE_DIR")
  Buzz.Service { id: service; autoConnect: false; manifest: ({id: "community.buzz", version: "0.0.22"}) }
  // The smallest normal window (720 × 500 logical), on the same service;
  // declared first so the main window below is the active one.
  FloatingWindow {
    visible: true
    implicitWidth: 720
    implicitHeight: 500
    Buzz.PanelContent { id: narrow; anchors.fill: parent; service: service }
  }
  FloatingWindow {
    id: window
    visible: true
    implicitWidth: 1000
    implicitHeight: 640
    Buzz.PanelContent {
      id: view
      anchors.fill: parent
      service: service
      onCloseRequested: test.closeRequests++
    }
  }
  TestCase { id: input; when: false; name: "NavigationInput" }

  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(name, within) {
    return findNamed(within || view, name, []).filter(function(item) {
      for (var p = item; p; p = p.parent) if (!p.visible) return false
      return true
    })
  }
  function one(name, within) {
    var found = shown(name, within)
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  function click(item) {
    if (typeof item === "string") item = one(item)
    input.mouseClick(item, item.width / 2, item.height / 2)
  }
  function row(id, time) {
    return {id: id, author: test.other, time: time, text: "Synthetic", edited: false, truncated: false, unavailable: false}
  }
  function frame(kind, withThread) {
    var reply = row("2".repeat(64), 1790000060)
    reply.depth = 1
    reply.parent = test.rootId
    return JSON.stringify({version: 1, type: kind, instanceId: "navigation-fixture", generation: 1,
      capabilities: ["connection_status", "room_catalog", "room_history", "room_recipients", "message_send",
        "thread_replies", "thread_send", "user_status"],
      status: {generation: 1, connection: "authenticated", category: null, identity: test.me, relay: "wss://fixture.example/",
        catalog: {state: "ready", category: null, rooms: [{id: test.room, name: "Fixture room", description: "", kind: "stream",
          participants: [], hidden: false}]},
        history: {state: "snapshot", roomId: test.room, rows: [row(test.rootId, 1790000000)], hasMore: false, category: "history_completeness_unknown"},
        recipients: {state: "snapshot", roomId: test.room, partial: false, category: null,
          entries: [{key: test.me, name: "Fixture Me", status: null}, {key: test.other, name: "Fiona", status: null}]},
        delivery: {state: "idle", requestId: null, roomId: null, eventId: null, category: null},
        thread: withThread ? {state: "snapshot", roomId: test.room, rootId: test.rootId, rows: [reply], hasMore: false,
          category: "thread_completeness_unknown"}
          : {state: "unavailable", roomId: null, rootId: null, rows: [], hasMore: null, category: null},
        userStatus: {state: "ready", mine: null, category: null}}})
  }
  function agentFrame() {
    return JSON.stringify({version: 1, type: "hello", id: null, instanceId: "navigation-agents", capabilities: ["agent_manager"],
      status: {harnesses: [{id: "claude-code", bundle: "ready", signedIn: true}, {id: "codex", bundle: "ready", signedIn: true}],
        agents: [{id: test.agentId, name: "vClaude", description: "Synthetic persona", instructions: "Answer briefly.",
          harness: "claude-code", model: "", acpCommand: "buzz-acp", rooms: [test.room], respondTo: "owner-only",
          workspace: "/home/fixture/.local/state/omarchy-buzz-room-workspaces/" + test.agentId, identity: "c".repeat(64),
          enrolled: true, unit: "inactive", startAtLogin: false, answersDms: false, published: true, lastError: null,
          relay: "wss://fixture.example/", community: "Fixture",
          instances: [{relay: "wss://fixture.example/", community: "Fixture", rooms: [test.room],
            unit: "omarchy-buzz-agent-" + test.agentId + ".service", startAtLogin: false, published: true, lastError: null, state: "inactive"}]}],
        pending: null, modelProbe: {agentId: null, state: "idle", model: "", detail: null}, activeRelay: "wss://fixture.example/"}})
  }
  function composerField() { return one("buzzComposer") }
  // The room view: Buzz and the room on the left, × on the right, no Back.
  function expectRooms(label) {
    check(!view.subViewOpen && shown("buzzHeaderBack").length === 0, label + ": Back shown on the room view")
    // The room's title stays in the timeline row only; the header names no room.
    check(one("buzzHeaderTitle").text === "Buzz" && one("buzzHeaderPlace").text === "",
      label + ": header should show Buzz without the room: " + one("buzzHeaderPlace").text)
    // × belongs to the overlay only; a normal window closes like any other (Super+W).
    check(shown("buzzHeaderClose").length === (view.windowMode ? 0 : 1), label + ": × shown in window mode, or missing in the overlay")
    if (!view.windowMode) {
      var close = one("buzzHeaderClose")
      check(close.text === "×" && close.tooltipText === (view.threadOpen ? "Close Buzz" : "Close Buzz · Esc"),
        label + ": close control wrong: " + close.text + " / " + close.tooltipText)
    }
    check(shown("buzzHistoryScroll").length === 1, label + ": room history not shown")
    check(shown("buzzClose").length === 0, label + ": the old Close · Esc button is still shown")
  }
  // A view that replaced the room view: Back to rooms and its title, then ×.
  function expectSubView(name, title) {
    check(view.subView === name, "Expected view " + name + ", found " + view.subView)
    var back = one("buzzHeaderBack")
    check(back.text === "← Back to rooms" && back.focusable && back.tooltipText === "Return to the room view · Esc",
      name + ": Back control wrong: " + back.text + " / " + back.tooltipText)
    check(shown("buzzHeaderTitle").length === 0, name + ": Buzz title still shown beside Back")
    check(one("buzzHeaderPlace").text === title, name + ": header title " + one("buzzHeaderPlace").text + ", expected " + title)
    check(back.mapToItem(view, 0, 0).x < one("buzzHeaderPlace").mapToItem(view, 0, 0).x, name + ": title is not after Back")
    if (!view.windowMode) {
      var close = one("buzzHeaderClose")
      check(close.text === "×" && close.tooltipText === "Close Buzz", name + ": close tooltip wrong: " + close.tooltipText)
    } else check(shown("buzzHeaderClose").length === 0, name + ": × shown in window mode")
    check(shown("buzzHistoryScroll").length === 0, name + ": room history still shown")
    // Older in-page Back links are gone; the header is the one Back.
    var oldBacks = ["buzzSettingsBack", "buzzStatusBack", "buzzAgentBack"]
    oldBacks.forEach(function(old) { check(shown(old).length === 0, name + ": in-page " + old + " still shown") })
  }
  // Header fits: nothing overlaps and the rightmost control (× in the overlay,
  // the status text in a window) stays inside the panel.
  function expectHeaderFits(label, within) {
    var panel = within || view
    var close = shown("buzzHeaderClose", panel)[0] || one("buzzHeaderStatus", panel)
    var right = close.mapToItem(panel, close.width, 0).x
    check(right <= panel.width, label + ": header runs outside the panel (" + right + " > " + panel.width + ")")
    // The leftmost header item: Back on a sub-view, the Buzz title on the room view.
    var title = shown("buzzHeaderBack", panel)[0] || one("buzzHeaderTitle", panel)
    var status = one("buzzHeaderStatus", panel)
    var closeShown = shown("buzzHeaderClose", panel).length === 1
    check(title.width > 0 && title.mapToItem(panel, title.width, 0).x <= status.mapToItem(panel, 0, 0).x + 1
      && (!closeShown || status.mapToItem(panel, status.width, 0).x <= close.mapToItem(panel, 0, 0).x + 1), label + ": header items overlap")
    check(status.text.length > 0, label + ": header status text missing")
  }
  function backByClick(label) {
    click("buzzHeaderBack")
    check(!view.subViewOpen, label + ": Back to rooms did not return")
    test.expectRooms(label + " after Back")
    check(test.composerField().activeFocus, label + ": focus did not return to the composer")
  }
  function key(code) { input.keyClick(code) }

  Timer {
    interval: 100
    running: true
    repeat: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 100) throw new Error("Timed out at stage " + test.stage + " (view " + view.width + " × " + view.height + ")")
        if (test.stage === 0) {
          service.beginSession()
          check(service.acceptFrame(test.frame("hello", false)), "Helper frame rejected: " + service.category)
          service.agents.sessionFailed = false
          check(service.agents.acceptFrame(test.agentFrame()) && view.agentsVisible, "Agent frame rejected: " + service.agents.category)
          test.stage = 1
        } else if (test.stage === 1) {
          // Room view.
          test.expectRooms("room view")
          test.expectHeaderFits("room view")
          check(view.statusEntryShown, "Update your status not offered")

          // Settings from the account menu, back by click.
          click("buzzAccount")
          click("buzzAccountSettings")
          test.expectSubView("settings", "Settings")
          test.stage = 2
        } else if (test.stage === 2) {
          // Opening Settings puts keyboard focus on Back; Enter returns to the rooms.
          check(one("buzzHeaderBack").activeFocus, "Back did not take focus when Settings opened")
          test.expectHeaderFits("settings")
          key(Qt.Key_Return)
          check(!view.subViewOpen && test.composerField().activeFocus, "Enter on Back did not return to the composer")
          view.openSettings()
          test.backByClick("settings")

          // Update your status.
          click("buzzAccount")
          click("buzzAccountStatus")
          test.expectSubView("status", "Update your status")
          test.backByClick("status")

          // An agent (by name) and New agent.
          click(test.shown("buzzAgentRow").filter(function(item) { return item.agentId === test.agentId })[0])
          test.expectSubView("agent", "vClaude")
          check(one("buzzAgentTitle").text === "vClaude", "Agent editor did not open")
          test.stage = 2.5
        } else if (test.stage === 2.5) {
          // Measured one tick after opening: the header row lays out after the view swap.
          test.expectHeaderFits("agent")
          test.backByClick("agent")
          click("buzzNewAgent")
          test.expectSubView("new-agent", "New agent")
          test.backByClick("new agent")

          // Narrowest window: the header still fits on both kinds of view.
          check(Math.abs(narrow.width - 720) <= 1 && Math.abs(narrow.height - 500) <= 1, "Narrow window is " + narrow.width + " × " + narrow.height)
          test.expectHeaderFits("720 room view", narrow)
          narrow.openAgentEditor(test.agentId)
          test.stage = 3
        } else if (test.stage === 3) {
          // One tick for the editor to open; a second for its header row to lay out
          // (measuring on the first tick raced the layout about one run in three).
          check(one("buzzHeaderPlace", narrow).text === "vClaude" && one("buzzHeaderBack", narrow).visible, "Narrow agent header wrong")
          test.stage = 4
        } else if (test.stage === 4) {
          test.expectHeaderFits("720 agent", narrow)
          narrow.backToRooms()
          // Escape order, one step per press. Start in the composer with a mention list open.
          var field = test.composerField()
          field.forceActiveFocus()
          field.text = "@Fi"
          field.cursorPosition = 3
          check(view.mentionOpen, "Mention list did not open")
          key(Qt.Key_Escape)
          check(!view.mentionOpen && test.closeRequests === 0 && field.activeFocus, "Escape did not close only the mention list")
          field.text = ""
          // The recipient picker (@) is a popover too.
          view.recipientPickerExpanded = true
          key(Qt.Key_Escape)
          check(!view.recipientPickerExpanded && test.closeRequests === 0, "Escape did not close only the recipient picker")

          // Thread open, then the account menu over it.
          service.openThread(test.rootId)
          check(service.acceptFrame(test.frame("status", true)) && view.threadOpen, "Thread did not open")
          test.expectRooms("thread open")
          // An open thread may hide the room list; the menu opens the same way.
          check(view.openAccountMenu(), "Account menu refused")
          check(view.accountMenuOpen, "Account menu did not open")
          key(Qt.Key_Escape)
          check(!view.accountMenuOpen && view.threadOpen && test.closeRequests === 0, "Escape 1 did not close only the menu")
          // The profile card is a popover too.
          view.openAvatarCard(test.other, "Fiona", "", 0)
          check(shown("buzzAvatarCard").length === 1, "Profile card did not open")
          key(Qt.Key_Escape)
          check(shown("buzzAvatarCard").length === 0 && view.threadOpen && test.closeRequests === 0, "Escape on the profile card closed more than the card")
          // Sidebar pickers close before the thread.
          view.newDmOpen = true
          check(view.escapeKey() === "menu" && !view.newDmOpen && view.threadOpen, "Escape did not close the new message picker first")
          view.joinOpen = true
          check(view.escapeKey() === "menu" && !view.joinOpen && view.threadOpen, "Escape did not close the join section first")
          test.composerField().forceActiveFocus()
          key(Qt.Key_Escape)
          check(!view.threadOpen && test.closeRequests === 0, "Escape 2 did not close the thread")
          check(test.composerField().activeFocus, "Focus did not return to the composer after the thread")

          // A view over an open thread: Escape returns to the rooms, the thread comes back.
          service.openThread(test.rootId)
          check(service.acceptFrame(test.frame("status", true)) && view.threadOpen, "Thread did not reopen")
          view.openSettings()
          check(!view.threadOpen && view.subView === "settings", "Settings did not replace the room view")
          click("buzzAccount")
          key(Qt.Key_Escape)
          check(!view.accountMenuOpen && view.subView === "settings" && test.closeRequests === 0, "Escape did not close the menu over Settings first")
          key(Qt.Key_Escape)
          check(!view.subViewOpen && view.threadOpen && test.closeRequests === 0, "Escape 3 did not return from Settings to the room and thread")
          key(Qt.Key_Escape)
          check(!view.threadOpen && test.closeRequests === 0, "Escape did not close the restored thread")

          // Each view closes on Escape from its own fields.
          view.openAgentEditor(test.agentId)
          one("buzzAgentName").forceActiveFocus()
          key(Qt.Key_Escape)
          check(!view.subViewOpen && test.closeRequests === 0 && test.composerField().activeFocus, "Escape from the agent editor did not go back")
          view.openStatus()
          test.stage = 6
        } else if (test.stage === 6) {
          check(one("buzzStatusText").activeFocus, "Status text did not take focus")
          key(Qt.Key_Escape)
          check(!view.subViewOpen && test.closeRequests === 0, "Escape from the status view did not go back")

          // Only then does Escape close Buzz; × closes it from any view.
          test.composerField().forceActiveFocus()
          key(Qt.Key_Escape)
          check(test.closeRequests === 1, "Escape on the room view did not close Buzz")
          // × exists only in the overlay; there it closes Buzz from any view.
          view.openSettings()
          check(shown("buzzHeaderClose").length === 0, "× shown in window mode")
          view.windowMode = false
          click("buzzHeaderClose")
          check(test.closeRequests === 2, "× did not close Buzz from Settings in the overlay")
          view.windowMode = true
          view.backToRooms()
          test.composerField().forceActiveFocus()

          if (test.captureDir === "") {
            test.pass()
            return
          }
          test.stage = 20
        } else if (test.stage === 20) {
          test.stage = 21
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.captureDir + "/navigation-rooms.png")) { console.error("Could not save the room view"); Qt.exit(1); return }
            view.openAgentEditor(test.agentId)
            test.stage = 22
          })
        } else if (test.stage === 22) {
          test.stage = 23
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.captureDir + "/navigation-agent.png")) { console.error("Could not save the agent editor"); Qt.exit(1); return }
            test.pass()
          })
        }
      } catch (error) {
        console.error(error.message || error)
        Qt.exit(1)
      }
    }
  }
  function pass() {
    console.log("PASS: the header shows Buzz, the room and × (Close Buzz · Esc) on the room view and ← Back to rooms with the title for Settings, Update your status, an agent and New agent; Back returns by click or Enter with focus in the composer; Escape closes a menu, card or picker, then the thread, then the view, and only then Buzz; the header fits at 720 × 500")
    Qt.quit()
  }
}
