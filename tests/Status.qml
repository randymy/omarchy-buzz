// "Update your status" against a synthetic stdio helper: no relay, key or
// network. The account menu entry shows the placeholder, then the status; the
// status view offers chips, a free emoji field and durations; a relay refusal
// and a rate-limit refusal are explained; a set and a clear are accepted; the
// other member's status emoji sits beside their name. Escape and Back return
// to the rooms without closing the panel.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "plugin" as Buzz
import "plugin/PlainText.js" as PlainText

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  property bool busy: false
  property int closeRequests: 0
  readonly property string me: "5".repeat(64)
  readonly property string other: "8".repeat(64)
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  Buzz.Service { id: sample; autoConnect: false; sampleMode: true }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 700
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service; onCloseRequested: test.closeRequests++ }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  TestCase { id: input; when: false; name: "StatusInput" }
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
  function openEntry() {
    click("buzzAccount")
    check(view.accountMenuOpen, "Account menu did not open")
    var entry = one("buzzAccountStatus")
    var settings = one("buzzAccountSettings")
    check(entry.mapToItem(view, 0, 0).y < settings.mapToItem(view, 0, 0).y, "Status entry is not above Settings")
    return entry
  }
  function authorEmoji() {
    return shown("buzzAuthorStatus").map(function(item) { return item.text }).sort().join(",")
  }
  // Frame validation is strict: malformed status views and roster statuses are refused.
  function validationChecks() {
    var ok = {state: "ready", mine: {text: "Lunch", emoji: "🍽️", expiresAt: 1790000000}, category: null}
    check(service.validatedUserStatus(ok) !== null, "A valid status view was refused")
    check(service.validatedUserStatus({state: "failed", mine: null, category: "status_rejected"}) !== null, "A failed view was refused")
    var bad = [
      {state: "ready", mine: null},
      {state: "busy", mine: null, category: null},
      {state: "ready", mine: null, category: "status_rejected"},
      {state: "failed", mine: null, category: "other"},
      {state: "unavailable", mine: ok.mine, category: null},
      {state: "ready", mine: {text: "x", emoji: "x", expiresAt: null}, category: null},
      {state: "ready", mine: {text: "x", emoji: null, expiresAt: -1}, category: null},
      {state: "ready", mine: {text: "x", emoji: null}, category: null},
      {state: "ready", mine: {text: "a‮b", emoji: null, expiresAt: null}, category: null},
      {state: "ready", mine: {text: "é".repeat(101), emoji: null, expiresAt: null}, category: null},
      {state: "ready", mine: {text: "", emoji: null, expiresAt: null}, category: null},
      {state: "ready", mine: {text: "x", emoji: null, expiresAt: null, key: "a".repeat(64)}, category: null}
    ]
    bad.forEach(function(value, index) { check(service.validatedUserStatus(value) === null, "Malformed status view " + index + " accepted") })
    var roster = function(status) {
      return {state: "snapshot", roomId: "00000000-0000-4000-8000-0000000000b1", partial: false, category: null,
        entries: [{key: test.other, name: "Robin", status: status}]}
    }
    check(service.validatedRecipients(roster({text: "Out", emoji: "🤒"}), true).entries[0].status.emoji === "🤒", "Roster status lost")
    check(service.validatedRecipients(roster(null), true).entries[0].status === null, "Null roster status refused")
    ;[undefined, {text: "Out"}, {text: "Out", emoji: "ab"}, {text: "Out\n", emoji: null}, {text: "", emoji: null},
      {text: "Out", emoji: null, expiresAt: 1}].forEach(function(status, index) {
      check(service.validatedRecipients(roster(status), true) === null, "Malformed roster status " + index + " accepted")
    })
    ;["🙂", "🗓️", "👩‍💻", "🏳️‍🌈", ":tada:", ":+1:"].forEach(function(e) { check(service.validStatusEmoji(e), "Emoji refused: " + e) })
    ;["", "x", "1", "🙂a", "🙂 ", "‍🙂", "🙂‮", "🙂".repeat(9), ":a b:", ":" + "a".repeat(33) + ":"].forEach(function(e) {
      check(!service.validStatusEmoji(e), "Emoji accepted: " + JSON.stringify(e))
    })
    // Sample data shows a fixture status and publishes nothing.
    check(sample.myStatus && sample.myStatus.emoji === "🧭" && sample.authorStatus("Alex").emoji === "🗓️", "Sample status missing")
    // Tooltips render AutoText: a member's status with markup must reach them escaped,
    // shown literally, never as an image that fetches a remote host on hover.
    var beacon = '<img src="https://tracker.example/p.png"> Out & about'
    check(service.validatedRecipients(roster({text: beacon, emoji: null}), true).entries[0].status.text === beacon,
      "Markup status was not accepted as plain text")
    var shownTip = PlainText.tip(beacon)
    check(shownTip.indexOf("<img") === -1 && shownTip.indexOf("&lt;img src=&quot;https://tracker.example/p.png&quot;&gt; Out &amp; about") !== -1,
      "Tooltip text is not escaped: " + shownTip)
    check(PlainText.tip("") === "" && PlainText.tip(null) === "" && PlainText.tip(undefined) === "", "Empty tooltip is not empty")
    check(!sample.canSetStatus && !sample.setStatus("x", "", 24), "Sample mode can publish a status")
  }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      // keyClick spins a nested event loop that fires this Timer again; a nested
      // tick would rerun the stage and tear the test down under its own handler.
      if (test.busy) return
      test.busy = true
      try {
        test.ticks++
        if (test.ticks > 150) throw new Error("Status timed out at stage " + test.stage + " " + service.connection + " " + JSON.stringify(service.userStatus) + " " + service.statusRequestState)
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.connection === "authenticated" && service.userStatusSupported
            && service.historyState === "snapshot" && service.recipientsState === "snapshot") {
          test.validationChecks()
          check(shown("buzzAccountStatusEmoji").length === 0, "A status emoji shows without a status")
          check(test.authorEmoji() === "🤒", "Others' status beside names wrong: " + test.authorEmoji())
          var entry = test.openEntry()
          check(/Update your status$/.test(entry.text), "Placeholder wrong: " + entry.text)
          click(entry)
          check(view.statusOpen && !view.accountMenuOpen && shown("buzzHistoryScroll").length === 0, "Status view did not replace the room")
          one("buzzStatusView")
          check(one("buzzStatusCurrent").text === "No status set.", "Current status wrong: " + one("buzzStatusCurrent").text)
          var chips = shown("buzzStatusChip")
          check(chips.length === 12 && chips.every(function(c) { return service.validStatusEmoji(c.emoji) }), "Expected 12 valid chips, found " + chips.length)
          check(!one("buzzStatusSet").enabled && !one("buzzStatusClear").enabled, "Set or Clear enabled with nothing to do")
          check(one("buzzStatusHours24").selected, "Default duration is not one day")
          one("buzzStatusText").text = "In a meeting"
          var meeting = chips.filter(function(c) { return c.emoji === "🗓️" })[0]
          click(meeting)
          check(one("buzzStatusEmojiField").text === "🗓️" && meeting.selected, "Chip did not choose its emoji")
          // A free emoji is checked like the helper checks it.
          one("buzzStatusEmojiField").text = "x"
          check(!one("buzzStatusSet").enabled && one("buzzStatusDraftNote").text === "Use one emoji or a :shortcode:.", "Invalid emoji accepted")
          one("buzzStatusEmojiField").text = ":tada:"
          check(one("buzzStatusSet").enabled && shown("buzzStatusDraftNote").length === 0, "Shortcode refused")
          one("buzzStatusText").text = "é".repeat(101)
          check(!one("buzzStatusSet").enabled && one("buzzStatusDraftNote").text === "Up to 200 characters.", "Oversized text accepted")
          one("buzzStatusText").text = "In a meeting"
          click(meeting)
          check(one("buzzStatusEmojiField").text === "🗓️", "Chip did not replace the free emoji")
          click("buzzStatusHours4")
          check(view.statusHours === 4, "Duration not taken")
          check(!service.setStatus("x", "", 5) && !service.setStatus("", "", 24) && !service.setStatus("x", "x", 24), "An invalid status was sent")
          click("buzzStatusSet")
          check(!service.canSetStatus && !service.clearStatus(), "A second request was sent while the first was pending")
          test.stage = 1
        } else if (test.stage === 1 && service.userStatus.state === "failed" && service.statusRequestState === "idle") {
          check(view.statusOpen, "A refused status closed the view")
          check(one("buzzStatusMessage").text === "The relay refused this status. Try again.", "Refusal message wrong: " + one("buzzStatusMessage").text)
          check(shown("buzzAccountStatusEmoji").length === 0, "A refused status shows on the account")
          click("buzzStatusSet")
          test.stage = 2
        } else if (test.stage === 2 && service.statusRequestState === "failed") {
          check(one("buzzStatusMessage").text === "Status changed a moment ago. Wait a few seconds, then try again.",
            "Rate-limit message wrong: " + one("buzzStatusMessage").text)
          click("buzzStatusSet")
          test.stage = 3
        } else if (test.stage === 3 && !view.statusOpen && service.myStatus) {
          check(one("buzzAccountStatusEmoji").text === "🗓️", "Account control lacks the status emoji")
          check(one("buzzAccount").tooltipText.indexOf("🗓️ In a meeting") !== -1, "Account tooltip lacks the status")
          check(test.authorEmoji() === "🗓️,🤒", "My status is not beside my name: " + test.authorEmoji())
          var entry = test.openEntry()
          check(entry.text === "🗓️ In a meeting", "Menu entry does not show the status: " + entry.text)
          click(entry)
          check(view.statusOpen && one("buzzStatusText").text === "In a meeting" && one("buzzStatusEmojiField").text === "🗓️"
            && one("buzzStatusCurrent").text === "Now: 🗓️ In a meeting", "Status view not prefilled")
          check(shown("buzzStatusMessage").length === 0, "An earlier outcome is shown in a fresh view")
          input.keyClick(Qt.Key_Escape)
          check(!view.statusOpen && test.closeRequests === 0 && shown("buzzHistoryScroll").length === 1, "Escape did not return to the rooms")
          click(test.openEntry())
          click("buzzHeaderBack")
          check(!view.statusOpen && shown("buzzHistoryScroll").length === 1, "Back did not return to the rooms")
          click(test.openEntry())
          check(one("buzzStatusClear").enabled, "Clear disabled with a status set")
          click("buzzStatusClear")
          test.stage = 4
        } else if (test.stage === 4 && !view.statusOpen && !service.myStatus) {
          check(shown("buzzAccountStatusEmoji").length === 0, "Cleared status still on the account")
          check(test.authorEmoji() === "🤒", "Cleared status still beside my name")
          var entry = test.openEntry()
          check(/Update your status$/.test(entry.text), "Placeholder not restored: " + entry.text)
          view.closeAccountMenu()
          record.reload()
          test.stage = 5
        } else if (test.stage === 5) {
          var requests = JSON.parse(record.text() || '{"requests":[]}').requests
          var sets = requests.filter(function(r) { return r.type === "set_status" })
          var clears = requests.filter(function(r) { return r.type === "clear_status" })
          if ((sets.length !== 3 || clears.length !== 1) && test.ticks < 140) { record.reload(); return }
          check(sets.length === 3 && sets.every(function(r) {
            return Object.keys(r).sort().join(",") === "emoji,expiresInHours,id,text,type,version" && r.text === "In a meeting"
              && r.emoji === "🗓️" && r.expiresInHours === 4 && service.uuidValue(r.id)
          }), "Unexpected set requests: " + JSON.stringify(sets))
          check(clears.length === 1 && Object.keys(clears[0]).sort().join(",") === "id,type,version", "Unexpected clear: " + JSON.stringify(clears))
          console.log("PASS: account menu shows Update your status above Settings with the placeholder or the current status; the status view offers 12 chips, a checked free emoji field and 1h/4h/1d/1w; Set sends text, emoji and hours; relay and rate-limit refusals are explained; an accepted status shows on the account and beside names; Clear, Escape and Back work")
          Qt.quit()
        }
      } catch (error) { console.error(error.message || error); Qt.exit(1) } finally { test.busy = false }
    }
  }
}
