// Synthetic stdio helper only; no relay or identity access.
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  property string anchorId: ""
  property real anchorY: 0
  property int arrived: -1
  // Rows have arrived and the panel has had time to settle its layout.
  function settled(ready) {
    if (!ready) { test.arrived = -1; return false }
    if (test.arrived === -1) test.arrived = test.ticks
    return test.ticks - test.arrived >= 8
  }
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  FloatingWindow {
    visible: true
    implicitWidth: 900
    implicitHeight: 520
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function scroll() { return test.findNamed(view, "buzzHistoryScroll", [])[0] }
  function loadOlder() { return test.findNamed(view, "buzzLoadOlder", [])[0] }
  function messages() { return test.findNamed(test.scroll(), "buzzThreadToggle", []) }
  function delegateFor(id) {
    var found = test.messages().filter(function(item) { return item.messageId === id })[0]
    while (found && !(found.row && found.row.id === id)) found = found.parent
    return found
  }
  // The row at the top of the viewport and where it sits on screen.
  function markTop() {
    var flick = test.scroll().contentItem
    var rows = service.historyRows
    for (var i = 0; i < rows.length; i++) {
      var item = test.delegateFor(rows[i].id)
      if (item && item.y + item.height > flick.contentY + 1) {
        test.anchorId = rows[i].id
        test.anchorY = item.mapToItem(view, 0, 0).y
        return
      }
    }
    throw new Error("No visible row to anchor")
  }
  function checkAnchor(when) {
    var item = test.delegateFor(test.anchorId)
    if (!item) throw new Error("Anchored row disappeared " + when)
    var y = item.mapToItem(view, 0, 0).y
    if (Math.abs(y - test.anchorY) > 1) throw new Error("Viewport jumped " + when + ": " + test.anchorY + " -> " + y)
    if (test.scroll().follow) throw new Error("Loading older rows made the view follow the newest row " + when)
  }
  function requests() {
    record.reload()
    return JSON.parse(record.text()).requests
  }
  function historyFrame(rows, cursor, hasMore) {
    return {state:"snapshot", roomId:"11111111-1111-4111-8111-111111111111", rows:rows, hasMore:hasMore,
      category:"history_completeness_unknown", nextCursor:cursor, olderState:"idle"}
  }
  function checkValidation() {
    var rows = []
    for (var n = 0; n < 101; n++) rows.push({id:(n + 1).toString(16).padStart(64, "0"), author:"a".repeat(64), time:n,
      text:"Row " + n, edited:false, truncated:false, unavailable:false})
    var cursor = {createdAt: 5, id: "c".repeat(64)}
    if (!service.validatedHistory(test.historyFrame(rows.slice(0, 100), cursor, true)))
      throw new Error("100 held rows were rejected")
    if (service.validatedHistory(test.historyFrame(rows, cursor, true)))
      throw new Error("101 rows were accepted")
    var bad = [
      {createdAt: 5, id: "C".repeat(64)}, {createdAt: -1, id: "c".repeat(64)}, {createdAt: 1.5, id: "c".repeat(64)},
      {createdAt: 5}, {createdAt: 5, id: "c".repeat(64), extra: true}, [5, "c".repeat(64)], "cursor"
    ]
    bad.forEach(function(value) {
      if (service.validatedHistory(test.historyFrame(rows.slice(0, 2), value, true)))
        throw new Error("Malformed cursor accepted: " + JSON.stringify(value))
    })
    if (service.validatedHistory(test.historyFrame(rows.slice(0, 2), cursor, false)))
      throw new Error("A cursor without more history was accepted")
    var state = test.historyFrame(rows.slice(0, 2), null, true)
    state.olderState = "done"
    if (service.validatedHistory(state)) throw new Error("Unknown older state accepted")
    var legacy = test.historyFrame(rows.slice(0, 2), null, false)
    delete legacy.nextCursor
    delete legacy.olderState
    var accepted = service.validatedHistory(legacy)
    if (!accepted || accepted.nextCursor !== null || accepted.olderState !== "idle")
      throw new Error("A helper without older pages was not read as having no cursor")
  }
  Timer {
    interval: 50
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 240) { console.error("Older history check timed out at stage " + test.stage); Qt.exit(1); return }
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.historyRows.length === 20 && test.messages().length === 20) {
          if (!service.canLoadOlder || !test.loadOlder() || !test.loadOlder().visible)
            throw new Error("Load older messages not offered while a cursor exists")
          if (service.historyLabel.indexOf("20 messages shown") === -1 || service.historyLabel.indexOf("older history available") === -1)
            throw new Error("History label did not count the shown messages: " + service.historyLabel)
          var flick = test.scroll().contentItem
          if (flick.contentHeight <= flick.height) throw new Error("Head page did not fill the viewport")
          // The reader scrolls up to the top of what is shown.
          flick.contentY = 0
          test.stage = 1
        } else if (test.stage === 1) {
          if (test.scroll().follow) throw new Error("Reader at the top is still following the newest row")
          test.markTop()
          test.loadOlder().clicked()
          if (!service.olderLoading || test.loadOlder().text !== "Loading older messages…")
            throw new Error("Older request did not show as loading")
          if (service.loadOlder()) throw new Error("A second older request was sent while one is pending")
          test.stage = 2
        } else if (test.stage === 2 && test.settled(service.historyRows.length === 40 && !service.olderLoading)) {
          var sent = test.requests()
          // The record is written by the fixture process; wait until it is visible.
          if (sent.length === 0) return
          if (sent.length !== 1 || JSON.stringify(Object.keys(sent[0]).sort()) !== JSON.stringify(["id", "roomId", "type", "version"])
              || sent[0].version !== 1 || sent[0].type !== "fetch_older" || !/^ui-[0-9]+$/.test(sent[0].id)
              || sent[0].roomId !== "11111111-1111-4111-8111-111111111111")
            throw new Error("Unexpected fetch_older request: " + JSON.stringify(sent))
          if (test.messages().length !== 40 || service.historyRows[0].time !== 1700000060)
            throw new Error("Older rows were not rendered above the held rows")
          test.checkAnchor("after the first older page")
          if (!test.loadOlder().visible || test.loadOlder().text !== "Load older messages")
            throw new Error("Load older messages not offered for the next page")
          test.markTop()
          test.loadOlder().clicked()
          test.arrived = -1
          test.stage = 3
        } else if (test.stage === 3 && test.settled(service.historyRows.length === 100 && !service.olderLoading)) {
          var count = test.requests().length
          if (count < 2) return
          if (count !== 2) throw new Error("Second older request not sent once")
          if (test.messages().length !== 100 || service.historyRows[0].time !== 1700000000)
            throw new Error("100 held rows were not all rendered oldest first")
          if (service.historyNextCursor !== null || service.canLoadOlder || test.loadOlder().visible)
            throw new Error("Load older messages still offered without a cursor")
          if (service.historyLabel.indexOf("100 messages shown") === -1
              || service.historyLabel.indexOf("older messages exist but are not held") === -1)
            throw new Error("Cap was not reported: " + service.historyLabel)
          test.checkAnchor("after the control disappeared")
          if (service.loadOlder()) throw new Error("Older request sent without a cursor")
          test.checkValidation()
          console.log("PASS: older room history loads on request with an exact fetch_older request, renders above held rows without moving the reader, accepts 100 rows and rejects 101, and hides Load older messages without a cursor")
          Qt.quit()
        }
      } catch (error) { console.error(error.message || error); Qt.exit(1) }
    }
  }
}
