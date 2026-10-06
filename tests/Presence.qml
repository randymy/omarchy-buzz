// Presence against a synthetic stdio helper: no relay, key or network. The
// account menu offers "Set yourself as…" (Auto / Away / Appear offline) below
// "Update your status" with the current choice marked; a change sends exactly
// one `set_presence` (mode and the idle hint) and nothing is sent when nothing changed.
// Offscreen there is no ext-idle-notify, so the idle hint falls back to "active
// while the panel is open". Dots: your own on the avatar corner, others' before
// names in DM rows, new-DM candidates, the recipient picker and message
// authors; none when the state is unknown.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  property bool busy: false
  property int wait: 0
  readonly property string me: "5".repeat(64)
  readonly property string online: "8".repeat(64)
  readonly property string away: "9".repeat(64)
  readonly property string unknown: "7".repeat(64)
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  Buzz.Service { id: sample; autoConnect: false; sampleMode: true }
  Buzz.Service { id: idle; autoConnect: false; helperExecutable: "/nonexistent/omarchy-buzz" }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 700
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  TestCase { id: input; when: false; name: "PresenceInput" }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(name) {
    return findNamed(view, name, []).filter(function(item) {
      for (var p = item; p; p = p.parent) if (!p.visible) return false
      return true
    })
  }
  function one(name) {
    var found = shown(name)
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  function click(item) {
    if (typeof item === "string") item = one(item)
    input.mouseClick(item, item.width / 2, item.height / 2)
  }
  function presences(name) { return shown(name).map(function(dot) { return dot.presence }).sort().join(",") }
  // The record is reloaded on every tick, so it is at most one tick old.
  function requests() {
    return JSON.parse(record.text() || '{"requests":[]}').requests.filter(function(r) { return r.type === "set_presence" })
  }
  function lastRequest() { var all = requests(); return all.length ? all[all.length - 1].mode + "|" + all[all.length - 1].active : "" }
  function openMenu() {
    if (!view.accountMenuOpen) click("buzzAccount")
    check(view.accountMenuOpen, "Account menu did not open")
    var row = one("buzzPresenceRow")
    var status = one("buzzAccountStatus")
    var settings = one("buzzAccountSettings")
    check(row.mapToItem(view, 0, 0).y > status.mapToItem(view, 0, 0).y, "Presence row is not below Update your status")
    check(row.mapToItem(view, 0, 0).y < settings.mapToItem(view, 0, 0).y, "Presence row is not above Settings")
    return row
  }
  function marked() {
    return ["auto", "away", "offline"].filter(function(mode) {
      var button = one("buzzPresenceMode_" + mode)
      return button.selected && button.text.indexOf("✓ ") === 0
    }).join(",")
  }
  function ownDot() { return one("buzzAccountPresenceDot") }
  // Frame validation is strict: malformed presence views and roster states are refused.
  function validationChecks() {
    var ok = {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1790000000, category: null,
      peers: [{key: test.online, presence: "online"}]}
    check(service.validatedPresence(ok) !== null, "A valid presence view was refused")
    check(service.validatedPresence({state: "unavailable", mode: null, published: null, lastPublishedAt: null, category: null, peers: []}) !== null, "Default view refused")
    check(service.validatedPresence({state: "failed", mode: "away", published: "away", lastPublishedAt: 1, category: "presence_rejected", peers: []}) !== null, "Failed view refused")
    var bad = [
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: null},
      {state: "busy", mode: "auto", published: "online", lastPublishedAt: 1, category: null, peers: []},
      {state: "ready", mode: "online", published: "online", lastPublishedAt: 1, category: null, peers: []},
      {state: "ready", mode: "auto", published: "busy", lastPublishedAt: 1, category: null, peers: []},
      {state: "ready", mode: "auto", published: null, lastPublishedAt: null, category: null, peers: []},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: null, category: null, peers: []},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1.5, category: null, peers: []},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: "presence_rejected", peers: []},
      {state: "failed", mode: "auto", published: null, lastPublishedAt: null, category: "other", peers: []},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: null, peers: [{key: "A".repeat(64), presence: "online"}]},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: null, peers: [{key: test.online, presence: "idle"}]},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: null, peers: [{key: test.online, presence: null}]},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: null, peers: [{key: test.online, presence: "online", name: "x"}]},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: null, peers: [{key: test.online, presence: "online"}, {key: test.online, presence: "away"}]},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: null, peers: [{key: test.me, presence: "online"}]},
      {state: "ready", mode: "auto", published: "online", lastPublishedAt: 1, category: null,
        peers: Array.from({length: 61}, function(_, i) { return {key: ("0" + i.toString(16)).slice(-2).repeat(32), presence: "away"} })}
    ]
    bad.forEach(function(value, index) { check(service.validatedPresence(value) === null, "Malformed presence view " + index + " accepted") })
    var roster = function(presence) {
      return {state: "snapshot", roomId: "00000000-0000-4000-8000-0000000000c1", partial: false, category: null,
        entries: [{key: test.online, name: "Robin", status: null, presence: presence}]}
    }
    check(service.validatedRecipients(roster("away"), true, true).entries[0].presence === "away", "Roster presence lost")
    check(service.validatedRecipients(roster(null), true, true).entries[0].presence === null, "Null roster presence refused")
    check(service.validatedRecipients(roster("busy"), true, false).entries[0].presence === null, "Presence used without the capability")
    ;["busy", "", 1, undefined].forEach(function(value, index) {
      check(service.validatedRecipients(roster(value), true, true) === null, "Malformed roster presence " + index + " accepted")
    })
    // Sample data shows fixture states and publishes nothing.
    check(sample.presenceOf("Alex") === "online" && sample.presenceOf("Code agent") === "offline", "Sample presence missing")
    check(sample.myPresence === "away" && !sample.syncPresence(), "Sample own presence wrong or published")
    check(sample.setPresenceMode("offline") && sample.myPresence === "offline", "Sample preference not shown")
    // A saved preference is read strictly; nothing is sent without a session.
    idle.loadPresenceSettings('{"version":1,"mode":"offline"}')
    check(idle.presenceMode === "offline", "Saved preference not read")
    ;['{"version":2,"mode":"away"}', '{"version":1,"mode":"online"}', '{broken', ''].forEach(function(raw) {
      idle.loadPresenceSettings(raw)
      check(idle.presenceMode === "offline", "Malformed preference accepted: " + raw)
    })
    check(!idle.syncPresence() && !idle.setPresenceMode("away"), "Presence sent or changed without a session")
    // Offscreen: no idle monitor, so the panel's open state is the hint.
    check(!service.idleMonitorWorking, "Idle monitor unexpectedly active offscreen")
  }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      // mouseClick spins a nested event loop that fires this Timer again; a
      // nested tick would rerun the stage and tear the test down under its own handler.
      if (test.busy) return
      test.busy = true
      try {
        test.ticks++
        record.reload()
        if (test.ticks > 140) throw new Error("Presence timed out at stage " + test.stage + " " + service.connection + " " + JSON.stringify(service.presence) + " " + test.lastRequest())
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.connection === "authenticated" && service.presenceSupported
            && service.historyState === "snapshot" && service.recipientsState === "snapshot" && service.presence.published === "away" && test.lastRequest() === "auto|false") {
          test.validationChecks()
          // On connect: Auto, and not active (the panel is closed).
          check(test.lastRequest() === "auto|false" && test.requests().length === 1, "Connect request wrong: " + JSON.stringify(test.requests()))
          check(test.ownDot().presence === "away" && Qt.colorEqual(test.ownDot().color, service.presenceColors.away) && test.ownDot().label === "Away", "Own dot not amber")
          check(service.presenceColors.online === "#3fb950" && service.presenceColors.away === "#d29922" && service.presenceColors.offline === "#8b949e", "Dot colors changed")
          one("buzzAccountDot")
          service.panelOpen = true
          test.stage = 1
        } else if (test.stage === 1 && service.presence.published === "online" && test.lastRequest() === "auto|true") {
          check(test.lastRequest() === "auto|true" && test.requests().length === 2, "Idle fallback did not send active")
          check(test.ownDot().presence === "online" && test.ownDot().label === "Online", "Own dot not green")
          // Nothing changed: nothing is sent.
          service.panelOpen = true
          check(!service.syncPresence(), "An unchanged preference was sent again")
          // Others: authors, DM rows, new-DM candidates and the picker; none when unknown.
          check(test.presences("buzzAuthorPresence") === "away,online,online", "Author dots wrong: " + test.presences("buzzAuthorPresence"))
          var dm = one("buzzDmPresence")
          check(dm.presence === "offline" && dm.label === "Offline" && Qt.colorEqual(dm.color, service.presenceColors.offline), "DM dot wrong: " + dm.presence)
          check(service.presenceOf(test.unknown) === "" && service.presenceOf("4".repeat(64)) === "", "Unknown keys have a state")
          // The existing DM partner (offline) is listed beside the roster members.
          view.newDmOpen = true
          check(test.presences("buzzNewDmPresence") === "away,offline,online", "New-DM dots wrong: " + test.presences("buzzNewDmPresence"))
          view.newDmOpen = false
          view.recipientPickerExpanded = true
          check(test.presences("buzzRecipientPresence") === "away,online,online", "Picker dots wrong: " + test.presences("buzzRecipientPresence"))
          view.recipientPickerExpanded = false
          check(one("buzzAccount").tooltipText.indexOf(" · Online") !== -1, "Account tooltip lacks the presence")
          test.openMenu()
          check(test.marked() === "auto", "Auto not marked: " + test.marked())
          check(one("buzzPresenceMode_offline").text === "Appear offline", "Offline label wrong")
          click("buzzPresenceMode_away")
          test.stage = 2
        } else if (test.stage === 2 && service.presence.published === "away" && test.lastRequest() === "away|true") {
          check(test.lastRequest() === "away|true", "Away request wrong: " + test.lastRequest())
          check(test.ownDot().presence === "away", "Own dot not amber after Away")
          test.openMenu()
          check(test.marked() === "away", "Away not marked: " + test.marked())
          click("buzzPresenceMode_offline")
          test.stage = 3
        } else if (test.stage === 3 && service.presence.published === "offline" && test.lastRequest() === "offline|true") {
          check(test.lastRequest() === "offline|true", "Offline request wrong")
          check(test.ownDot().presence === "offline" && Qt.colorEqual(test.ownDot().color, service.presenceColors.offline) && test.ownDot().label === "Offline", "Own dot not grey")
          test.openMenu()
          check(test.marked() === "offline", "Offline not marked")
          // Choosing the current mode sends nothing (checked in the final sequence).
          click("buzzPresenceMode_offline")
          test.openMenu()
          click("buzzPresenceMode_auto")
          test.stage = 4
        } else if (test.stage === 4 && service.presence.published === "online" && test.lastRequest() === "auto|true") {
          check(test.lastRequest() === "auto|true", "Auto request wrong")
          // The panel closes: idle under the fallback, so Auto derives away.
          service.panelOpen = false
          test.stage = 5
        } else if (test.stage === 5 && service.presence.published === "away" && test.lastRequest() === "auto|false") {
          check(test.lastRequest() === "auto|false", "Idle request wrong")
          test.openMenu()
          check(test.marked() === "auto", "Auto not marked while idle")
          click("buzzPresenceMode_away")
          test.stage = 6
        } else if (test.stage === 6 && service.presence.mode === "away" && test.lastRequest() === "away|false") {
          test.wait++
          if (test.wait < 3) return
          var sent = test.requests().map(function(r) { return r.mode + "|" + r.active })
          check(sent.join(",") === "auto|false,auto|true,away|true,offline|true,auto|true,auto|false,away|false",
            "Unexpected presence requests: " + sent.join(","))
          check(test.requests().every(function(r) { return Object.keys(r).sort().join(",") === "active,id,mode,type,version" && service.uuidValue(r.id) }),
            "Request shape wrong")
          console.log("PASS: Set yourself as… sits below Update your status with the current choice marked; each change sends one set_presence {mode, active}, an unchanged one sends nothing; offscreen the idle hint follows the open panel; your own dot (green/amber/grey) sits on the avatar corner beside the connection dot; others' dots show before names in DM rows, new-DM candidates, the recipient picker and message authors, and none when unknown; sample mode shows fixture states and publishes nothing")
          Qt.quit()
        }
      } catch (error) { console.error(error.message || error); Qt.exit(1) } finally { test.busy = false }
    }
  }
}
