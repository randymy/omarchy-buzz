// After joining a community, against a synthetic stdio helper (no relay, key or
// secret store) and synthetic agent service frames (no units): the success
// line and its auto-hide; the welcome pane for a community without joined
// rooms (title, host, open rooms loading then listed inline with Join, invite
// field, Switch community); header and sidebar wording; the pane gone once a
// room is joined; "Switched to …" only for a switch chosen from the menu and
// dismissed by navigation; the account control's key fallback; and agent
// avatars that fall back to the identicon (no art) or show stored plain art
// without its leading blank line. BUZZ_WELCOME_CAPTURE_DIR optionally saves
// welcome.png after the checks pass.
import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as Buzz
import "plugin/Identicon.js" as Identicon

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  property real hiddenAt: 0
  property var arrivedAt: 0
  readonly property string me: "7".repeat(64)
  readonly property string first: "wss://first.example/"
  readonly property string second: "wss://second.example/"
  readonly property string bareAgent: "33333333-3333-4333-8333-333333333333"
  readonly property string artAgent: "44444444-4444-4444-8444-444444444444"
  // The shape of the art stored for the maintainer's agent: plain pasted
  // text that starts with an empty line.
  readonly property string plainArt: "\n xxx  xxx\n  x    x\n     \\\n   ___"
  readonly property string captureDir: Quickshell.env("BUZZ_WELCOME_CAPTURE_DIR")
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  FloatingWindow {
    visible: true
    implicitWidth: 960
    implicitHeight: 640
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  // Grid art whose thumbnail has no cells: never a partial frame.
  Buzz.BuzzAvatar { id: emptyGrid; key: test.me; art: "\u001b[0m" }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
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
  function texts() {
    return findNamed(view, "", []).filter(function(item) {
      for (var p = item; p; p = p.parent) if (!p.visible) return false
      return typeof item.text === "string"
    }).map(function(item) { return item.text })
  }
  function requests() { record.reload(); return JSON.parse(record.text()).requests }
  function agentFrame() {
    function persona(id, name, identity) {
      return {id: id, name: name, description: "Synthetic persona", instructions: "Answer briefly.",
        harness: "claude-code", model: "", acpCommand: "buzz-acp", rooms: ["aaaaaaaa-0000-4000-8000-0000000000f1"],
        respondTo: "owner-only", workspace: "/home/fixture/.local/state/omarchy-buzz-room-workspaces/" + id,
        identity: identity, enrolled: identity !== null, unit: "inactive", startAtLogin: false, answersDms: false,
        published: identity !== null, lastError: null}
    }
    return JSON.stringify({version: 1, type: "hello", id: null, instanceId: "welcome-agents", capabilities: ["agent_manager"],
      status: {harnesses: [{id: "claude-code", bundle: "ready", signedIn: true}, {id: "codex", bundle: "ready", signedIn: true}],
        agents: [persona(test.bareAgent, "vBare", "c".repeat(64)), persona(test.artAgent, "vClaude", "d".repeat(64))],
        pending: null, modelProbe: {agentId: null, state: "idle", model: "", detail: null}}})
  }
  function agentAvatar(name) {
    var rows = shown("buzzAgentRow").filter(function(row) { return row.text.indexOf(name + " ·") === 0 })
    check(rows.length === 1, "Agent row missing: " + name)
    var avatars = findNamed(rows[0].parent, "buzzAvatar", [])
    check(avatars.length === 1, "Agent avatar missing: " + name)
    return avatars[0]
  }
  function expectAvatars(label) {
    var bare = agentAvatar("vBare")
    check(!bare.usesArt && bare.text === Identicon.glyph("c".repeat(64)), label + ": an agent without art is not its identicon: " + JSON.stringify(bare.text))
    var drawn = agentAvatar("vClaude")
    check(drawn.usesArt && !drawn.usesColor && drawn.text === " xxx  xxx\n  x    x\n     \\\n   ___",
      label + ": stored plain art not shown as stored without its blank first line: " + JSON.stringify(drawn.text))
  }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 130) throw new Error("Welcome timed out at stage " + test.stage + " " + service.connection + " " + service.relay)
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.connection === "authenticated" && service.catalogState === "ready" && service.communitiesSupported) {
          // A community with rooms: the room view, no welcome, no success line.
          check(service.selectedRoomId === "aaaaaaaa-0000-4000-8000-0000000000f1", "First room not selected")
          check(shown("buzzWelcomePane").length === 0 && shown("buzzHistoryScroll").length === 1, "Welcome shown with rooms")
          check(one("buzzRoomTitle").text === "# general" && shown("buzzCommunityJoined").length === 0, "Room view wrong")
          check(texts().indexOf("Joined rooms · 2") !== -1, "Sidebar count wrong")
          // No profile name here: the key's short form, as message authors, not "Me".
          check(one("buzzAccountName").text === test.me.slice(0, 12) + "…", "Account name fallback wrong: " + one("buzzAccountName").text)
          check(one("buzzAccount").tooltipText.indexOf("No profile name on this community yet · Online") === 0,
            "Account tooltip wrong: " + one("buzzAccount").tooltipText)
          // Agents: one without art, one with plain art (a blank first line).
          service.agents.sessionFailed = false
          check(service.agents.acceptFrame(test.agentFrame()) && view.agentsVisible, "Agent frame rejected: " + service.agents.category)
          check(service.agents.setAvatarArt(test.artAgent, test.plainArt), "Plain art not kept")
          test.expectAvatars("first community")
          check(!emptyGrid.usesArt && emptyGrid.text === Identicon.glyph(test.me), "Empty grid art drew a partial frame: " + JSON.stringify(emptyGrid.text))
          check(view.arrivalTimeout === 8000, "Success line does not stay about eight seconds")
          // Shortened here so the run fits the preview's time limit.
          view.arrivalTimeout = 3000
          view.openCommunityView("join")
          one("buzzCommunityInput").text = "https://second.example"
          one("buzzCommunityJoin").clicked()
          test.stage = 1
        } else if (test.stage === 1 && service.relay === test.second && service.openRoomsState === "loading") {
          // Joined: back on the rooms, the success line, and the welcome pane.
          check(view.subView === "", "Still in the join view")
          check(one("buzzCommunityJoined").text === "You joined second.", "Success line wrong")
          test.arrivedAt = Date.now()
          check(one("buzzWelcomePane") && one("buzzWelcomeTitle").text === "Welcome to second", "Welcome title wrong")
          check(one("buzzWelcomeHost").text === "second.example", "Host not under the title")
          check(one("buzzWelcomeLead").text === "Pick a room to get started.", "Lead sentence wrong")
          check(one("buzzWelcomeOpenRoomsStatus").text === "Loading open rooms…", "Open rooms loading not shown")
          check(shown("buzzHistoryScroll").length === 0 && shown("buzzRoomTitle").length === 0, "Empty room view still shown")
          check(texts().indexOf("Connect Buzz") === -1, "Stale Connect Buzz heading shown")
          check(service.statusLabel === "Authenticated · no rooms joined yet" && one("buzzHeaderStatus").text.endsWith(service.statusLabel), "Header wording wrong: " + one("buzzHeaderStatus").text)
          check(texts().indexOf("Rooms · none joined") !== -1 && texts().join("|").indexOf("Partial list") === -1, "Sidebar wording wrong")
          check(shown("buzzJoinFooter").length === 1 && !view.joinOpen, "Sidebar open rooms not shown by default with no rooms")
          test.stage = 2
        } else if (test.stage === 2 && service.openRoomsState === "snapshot") {
          var rows = shown("buzzWelcomeOpenRoom")
          check(rows.length === 2 && one("buzzWelcomeOpenRooms").children.some(function(c) { return c.text && c.text.indexOf("no approval") !== -1 }),
            "Open rooms not listed inline")
          check(findNamed(rows[1], "", []).some(function(c) { return c.text === "# welcome-everyone" }), "Room name wrong")
          check(shown("buzzOpenRoom").length === 2, "Sidebar open rooms missing")
          // The invite field and the switcher.
          check(shown("buzzWelcomeInviteInput").length === 1 && texts().indexOf("Have an invite? Paste it below") !== -1, "Invite row missing")
          check(shown("buzzWelcomeOpenRoomsRefresh").length === 1, "Refresh missing")
          one("buzzWelcomeSwitch").clicked()
          check(view.accountMenuOpen && shown("buzzCommunitySwitch").length === 1, "Switch community did not open the account menu")
          view.closeAccountMenu()
          check(one("buzzCommunityJoined").visible, "Opening the menu dismissed the success line")
          test.expectAvatars("second community")
          if (test.captureDir === "") { test.stage = 3; return }
          test.stage = 20
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.captureDir + "/welcome.png")) { console.error("Could not save the welcome pane"); Qt.exit(1); return }
            test.stage = 3
          })
        } else if (test.stage === 3 && shown("buzzCommunityJoined").length === 0) {
          // Hidden by itself after the timeout.
          var waited = Date.now() - test.arrivedAt
          check(waited >= 2500, "Success line hid too early: " + waited)
          check(one("buzzWelcomePane").visible, "Welcome pane went with the success line")
          shown("buzzWelcomeOpenRoomJoin")[1].clicked()
          test.stage = 4
        } else if (test.stage === 4 && service.selectedRoomId === "bbbbbbbb-0000-4000-8000-0000000000b2") {
          // The joined room is selected and the welcome pane is gone.
          check(shown("buzzWelcomePane").length === 0 && one("buzzRoomTitle").text === "# welcome-everyone", "Joined room not shown")
          check(service.statusLabel === "Authenticated · history unavailable" && one("buzzHeaderStatus").text.endsWith(service.statusLabel), "Header wording after joining: " + one("buzzHeaderStatus").text)
          check(texts().indexOf("Rooms · none joined") === -1 && shown("buzzJoinFooter").length === 0, "Sidebar still says none joined")
          view.openAccountMenu()
          shown("buzzCommunitySwitch")[0].clicked()
          test.stage = 5
        } else if (test.stage === 5 && service.relay === test.first && service.selectedRoomId !== "") {
          check(one("buzzCommunityJoined").text === "Switched to first.", "Menu switch not confirmed")
          // Choosing another room is navigation: the line goes.
          service.selectRoom("aaaaaaaa-0000-4000-8000-0000000000f2")
          check(shown("buzzCommunityJoined").length === 0, "Room change did not dismiss the line")
          view.openAccountMenu()
          shown("buzzCommunitySwitch")[0].clicked()
          test.stage = 6
        } else if (test.stage === 6 && service.relay === test.second && service.selectedRoomId !== "") {
          check(one("buzzCommunityJoined").text === "Switched to second.", "Second menu switch not confirmed")
          view.openSettings()
          view.backToRooms()
          check(shown("buzzCommunityJoined").length === 0, "Opening Settings did not dismiss the line")
          check(service.switchCommunity(test.first), "Direct switch not sent")
          test.stage = 7
        } else if (test.stage === 7 && service.relay === test.first && service.selectedRoomId !== "") {
          // Not chosen from the menu: no line.
          check(shown("buzzCommunityJoined").length === 0, "A switch not chosen from the menu was announced")
          check(shown("buzzWelcomePane").length === 0, "Welcome shown in a community with rooms")
          test.expectAvatars("back in the first community")
          var types = requests().map(function(r) { return r.type }).filter(function(t) { return ["subscribe", "get_snapshot", "retry_connection"].indexOf(t) === -1 })
          // The record file may still be reloading; it is read again shortly.
          if (types.join(",") !== "join_community,open_rooms,join_room,switch_community,switch_community,switch_community" && test.ticks < 125) return
          check(types.join(",") === "join_community,open_rooms,join_room,switch_community,switch_community,switch_community", "Unexpected requests: " + types.join(","))
          test.pass()
        }
      } catch (error) { console.error(error.message || error); Qt.exit(1) }
    }
  }
  function pass() {
    console.log("PASS: join success line and auto-hide; welcome pane with open rooms loading then inline Join, invite field and Switch community; header and sidebar wording; pane gone after a room join; menu switch confirmed and dismissed by navigation; other switches silent; account key fallback; agent avatars fall back to the identicon, plain art shown without its blank first line")
    Qt.quit()
  }
}
