// Message edit, delete and reactions against a synthetic stdio helper: no
// relay, key or network. Own messages offer Edit (inline, Enter saves, Esc
// cancels) and Delete (confirmed); other members' messages offer only React.
// A refused edit keeps the editor and its text, an accepted one shows after the
// refresh; reaction chips toggle and show whether this identity reacted; a
// lost outcome is shown as unknown and changes nothing.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int step: -1
  property int ticks: 0
  property bool started: false
  readonly property string mine: "a".repeat(64)
  readonly property string theirs: "b".repeat(64)
  readonly property string shortened: "c".repeat(64)
  readonly property string gone: "d".repeat(64)
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
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  TestCase { id: input; when: false; name: "MessageActionsInput" }
  function visibleItem(item) {
    for (var p = item; p; p = p.parent) if (!p.visible) return false
    return true
  }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function findMessages(item, found) {
    if (item.actionsOpen !== undefined && item.row && item.row.id) found.push(item)
    for (var i = 0; i < item.children.length; i++) findMessages(item.children[i], found)
    return found
  }
  function message(id) {
    var found = findMessages(view, []).filter(function(item) { return item.row.id === id })
    if (found.length !== 1) throw new Error("Expected one message " + id.slice(0, 1) + ", found " + found.length)
    return found[0]
  }
  function inside(id, name) { return findNamed(message(id), name, []).filter(visibleItem) }
  function one(id, name) {
    var found = inside(id, name)
    if (found.length !== 1) throw new Error("Expected one visible " + name + " on " + id.slice(0, 1) + ", found " + found.length)
    return found[0]
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  // Delegates inside the scrolling timeline are driven through their own
  // signal, as the other timeline tests do; pointer delivery there is not what is tested.
  function click(item) {
    if (typeof item.activate === "function") item.activate()
    else item.clicked()
  }
  function chip(id, emoji) {
    var found = inside(id, "buzzReactionChip").filter(function(item) { return item.emoji === emoji })
    return found.length === 1 ? found[0] : null
  }
  function openActions(id) { if (inside(id, "buzzMessageActions").length === 0) click(one(id, "buzzMessageMore")) }
  function bodyOf(id) { return one(id, "buzzMessageBody").text }
  function requests() {
    record.reload()
    return JSON.parse(record.text() || '{"requests":[]}').requests.filter(function(r) { return r.type !== "fetch_recent" && r.type !== "subscribe" })
  }
  function chipRow(id) {
    return inside(id, "buzzReactionChip").map(function(c) { return c.emoji + c.count + (c.mine ? "*" : "") }).join(" ")
  }
  function validationChecks() {
    var full = ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients", "history_auto_refresh", "room_activity",
      "agent_profiles", "thread_replies", "thread_send", "thread_summaries", "dm_open", "older_history", "live_updates", "setup_assist",
      "community_join", "invite_mint", "attachments", "user_status", "presence", "communities", "message_actions"]
    check(service.validCapabilities(full), "The helper's full capability list is refused")
    check(!service.validCapabilities(full.concat(["unknown_capability"])) && !service.validCapabilities(full.concat(["message_actions"])), "Unknown or repeated capability accepted")
    var rows = function(chips) {
      return {state: "snapshot", roomId: "00000000-0000-4000-8000-0000000000c1", hasMore: false, category: null,
        rows: [{id: "a".repeat(64), author: test.theirs, time: 1790000000, text: "x", edited: false, truncated: false, unavailable: false,
          reactions: {seen: 0, working: 0, chips: chips}}]}
    }
    var good = [{emoji: "🎉", count: 2, mine: true}, {emoji: "👍", count: 1, mine: false}]
    var accepted = service.validatedHistory(rows(good), 100, null)
    check(accepted && accepted.rows[0].reactions.chips.length === 2 && accepted.rows[0].reactions.chips[0].mine === true, "Valid chips refused")
    check(service.validatedHistory(rows(undefined), 100, null).rows[0].reactions.chips.length === 0, "Absent chips are not an empty list")
    var many = []
    for (var i = 0; i < 17; i++) many.push({emoji: String.fromCodePoint(0x1f600 + i), count: 1, mine: false})
    var bad = [
      [{emoji: "👀", count: 1, mine: false}], [{emoji: "💬", count: 1, mine: false}], [{emoji: ":tada:", count: 1, mine: false}],
      [{emoji: "x", count: 1, mine: false}], [{emoji: "🎉", count: 0, mine: false}], [{emoji: "🎉", count: 1001, mine: false}],
      [{emoji: "🎉", count: 1.5, mine: false}], [{emoji: "🎉", count: 1, mine: "yes"}], [{emoji: "🎉", count: 1}],
      [{emoji: "🎉", count: 1, mine: false, id: "a".repeat(64)}], [good[0], good[0]], many, "🎉"
    ]
    bad.forEach(function(chips, index) { check(service.validatedHistory(rows(chips), 100, null) === null, "Malformed chips " + index + " accepted") })
    check(service.validReactionEmoji("👍") && service.validReactionEmoji("❤️") && !service.validReactionEmoji("👀") && !service.validReactionEmoji(":tada:")
      && !service.validReactionEmoji("") && !service.validReactionEmoji("ab"), "Reaction emoji check wrong")
    // Sample data offers nothing and sends nothing.
    check(!sample.canAct && !sample.editMessage(test.mine, "x") && !sample.deleteMessage(test.mine) && !sample.toggleReaction(test.mine, "👍", false), "Sample mode can act")
  }
  readonly property var steps: [
    // 0: connected, rows shown.
    {until: function() { return service.connection === "authenticated" && service.messageActionsSupported && service.historyState === "snapshot" && findMessages(view, []).length >= 4 }},
    // 1: who may do what.
    {run: function() {
      test.validationChecks()
      check(service.canAct && service.identity === test.mine.replace(/a/g, "5"), "Service cannot act or lacks identity")
      check(one(test.mine, "buzzMessageMore") && one(test.theirs, "buzzMessageMore"), "Action affordance missing")
      check(chipRow(test.theirs) === "🎉2 👍1*", "Chips wrong: " + chipRow(test.theirs))
      check(chip(test.theirs, "👍").mine && !chip(test.theirs, "🎉").mine, "Mine flag wrong")
      check(inside(test.theirs, "buzzMessageReactions").length === 1, "Agent counts row lost")
      click(one(test.theirs, "buzzMessageMore"))
      check(inside(test.theirs, "buzzActionReact").length === 1 && inside(test.theirs, "buzzActionEdit").length === 0
        && inside(test.theirs, "buzzActionDelete").length === 0, "Another member's message offers edit or delete")
      click(one(test.theirs, "buzzMessageMore"))
      check(inside(test.theirs, "buzzMessageActions").length === 0, "Actions did not close")
      click(one(test.shortened, "buzzMessageMore"))
      check(!one(test.shortened, "buzzActionEdit").enabled && one(test.shortened, "buzzActionDelete").enabled, "A shortened message can be edited")
      click(one(test.shortened, "buzzMessageMore"))
      // Edit: Escape cancels without sending anything.
      click(one(test.mine, "buzzMessageMore"))
      click(one(test.mine, "buzzActionEdit"))
      var field = one(test.mine, "buzzEditField")
      check(field.text === "hello team" && inside(test.mine, "buzzMessageBody").length === 0, "Editor did not replace the text")
      field.text = "changed my mind"
      input.keyClick(Qt.Key_Escape)
      check(inside(test.mine, "buzzMessageEditor").length === 0 && bodyOf(test.mine) === "hello team", "Escape did not cancel the edit")
    }, until: function() { return true }},
    // 2: edit, refused by the relay.
    {run: function() {
      if (inside(test.mine, "buzzActionEdit").length === 0) click(one(test.mine, "buzzMessageMore"))
      click(one(test.mine, "buzzActionEdit"))
      one(test.mine, "buzzEditField").text = "hello team (fixed)"
      input.keyClick(Qt.Key_Return)
      // The fixture answers at once, so the pending window may already be over.
      if (service.actionState === "sending") check(!service.canAct && !service.editMessage(test.mine, "again"), "A second action was allowed while one was pending")
    }, until: function() { return service.actionState === "rejected" }},
    {run: function() {
      check(inside(test.mine, "buzzMessageEditor").length === 1 && one(test.mine, "buzzEditField").text === "hello team (fixed)", "A refused edit lost the editor or its text")
      check(/refused/.test(one(test.mine, "buzzActionNote").text), "Refusal not explained: " + inside(test.mine, "buzzActionNote").length)
      check(inside(test.mine, "buzzMessageBody").length === 0, "Original shown during a refused edit")
      // A send and a message action share the helper's one publication slot:
      // neither composer may submit while an action is pending.
      var saved = service.drafts
      var next = Object.assign({}, saved); next[service.composerKey] = "queued text"; service.drafts = next
      check(service.canSend && service.canSendFor(""), "A ready draft could not be sent")
      service.actionState = "sending"
      check(!service.canSend && !service.canSendFor(""), "Composer could submit while a message action was pending")
      service.actionState = "rejected"
      service.drafts = saved
      click(one(test.mine, "buzzEditSave"))
    }, until: function() { return service.actionState === "acknowledged" && inside(test.mine, "buzzMessageBody").length === 1 && /hello team \(fixed\) \(edited\)/.test(bodyOf(test.mine)) }},
    {run: function() {
      check(inside(test.mine, "buzzMessageEditor").length === 0 && /accepted/.test(one(test.mine, "buzzActionNote").text), "Editor stayed or acceptance not shown")
      service.clearAction()
    }, until: function() { return true }},
    // 5: delete with confirmation.
    {run: function() {
      click(one(test.gone, "buzzMessageMore"))
      click(one(test.gone, "buzzActionDelete"))
      check(inside(test.gone, "buzzDeleteConfirm").length === 1, "Delete did not ask first")
      click(one(test.gone, "buzzDeleteCancel"))
      check(inside(test.gone, "buzzDeleteConfirm").length === 0 && requests().filter(function(r) { return r.type === "delete_message" }).length === 0, "Keep still deleted")
      click(one(test.gone, "buzzActionDelete"))
      click(one(test.gone, "buzzDeleteConfirmYes"))
    }, until: function() { return findMessages(view, []).filter(function(m) { return m.row.id === test.gone }).length === 0 }},
    // 6: reactions toggle through the chips.
    {run: function() { click(chip(test.theirs, "🎉")) }, until: function() { return chipRow(test.theirs) === "🎉3* 👍1*" }},
    {run: function() { click(chip(test.theirs, "👍")) }, until: function() { return chipRow(test.theirs) === "🎉3*" }},
    // 8: the picker: quick emoji and free entry.
    {run: function() {
      service.clearAction()
      click(one(test.theirs, "buzzMessageMore"))
      click(one(test.theirs, "buzzActionReact"))
      check(inside(test.theirs, "buzzReactionQuick").length === 8, "Expected 8 quick reactions")
      var field = one(test.theirs, "buzzReactionField")
      field.text = "ab"
      check(inside(test.theirs, "buzzReactionFieldNote").length === 1, "Invalid free emoji not flagged")
      field.text = "👀"
      check(inside(test.theirs, "buzzReactionFieldNote").length === 1, "Agent emoji accepted as a reaction")
      field.accepted()
      check(service.actionState === "idle", "An invalid reaction was sent")
      field.text = "🔥"
      check(inside(test.theirs, "buzzReactionFieldNote").length === 0, "Valid free emoji flagged")
      field.accepted()
    }, until: function() { return chipRow(test.theirs) === "🎉3* 🔥1*" }},
    // 9: a lost outcome is unknown and changes nothing.
    {run: function() {
      service.clearAction()
      test.openActions(test.theirs)
      if (inside(test.theirs, "buzzReactionQuick").length === 0) click(one(test.theirs, "buzzActionReact"))
      var quick = inside(test.theirs, "buzzReactionQuick").filter(function(b) { return b.emoji === "😮" })[0]
      click(quick)
    }, until: function() { return service.actionState === "unknown" }},
    {run: function() {
      check(/unknown/.test(one(test.theirs, "buzzActionNote").text) && chipRow(test.theirs) === "🎉3* 🔥1*", "Unknown outcome shown as done or hidden")
    }, until: function() { return test.requests().length >= 7 }},
    {run: function() {
      var sent = test.requests()
      var shape = sent.map(function(r) { return r.type + (r.emoji ? ":" + r.emoji : "") })
      var expected = ["edit_message", "edit_message", "delete_message", "add_reaction:🎉", "remove_reaction:👍", "add_reaction:🔥", "add_reaction:😮"]
      check(JSON.stringify(shape) === JSON.stringify(expected), "Unexpected requests: " + JSON.stringify(shape))
      check(sent.every(function(r) { return service.uuidValue(r.id) && r.generation === 3 && r.instanceId === "message-actions-fixture" }), "Request scope wrong")
      console.log("PASS: own messages offer Edit (inline, Enter saves, Escape cancels) and confirmed Delete, other members' messages only React; a refused edit keeps the editor and text, an accepted one shows after the refresh; chips count reactors, mark mine and toggle; the picker offers 8 emoji and checked free entry (never 👀/💬/shortcodes); a lost outcome is shown as unknown; one action at a time; the helper's full capability list is accepted")
      Qt.quit()
    }, until: function() { return true }}
  ]
  property bool ran: false
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 140) throw new Error("Message actions timed out at step " + test.step + " " + service.connection + " " + service.actionState + " " + service.actionCategory)
        if (!test.started) { service.retry(); test.started = true; test.step = 0; return }
        var current = test.steps[test.step]
        if (!test.ran) {
          if (current.until && !current.run && !current.until()) return
          if (current.run) current.run()
          test.ran = true
          return
        }
        if (current.until()) { test.step++; test.ran = false }
      } catch (error) { console.error(error.message || error); Qt.exit(1) }
    }
  }
}
