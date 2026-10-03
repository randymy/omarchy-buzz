// Offscreen check of rich desktop notifications: synthetic helper notices drive the
// service, a fake `omarchy` records the argv (scripts/preview compares it exactly).
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  Buzz.Service { id: service; autoConnect: false }
  property int step: 0
  property string roomA: "11111111-1111-4111-8111-111111111111"
  property string roomB: "22222222-2222-4222-8222-222222222222"
  property string self: "a".repeat(64)
  property string other: "b".repeat(64)
  property string thread: "c".repeat(64)
  property var seqs: ({})

  function fail(message) { console.error("Buzz offscreen notification check failed: " + message); Qt.exit(1) }
  function check(ok, message) { if (!ok) { fail(message); throw new Error(message) } }
  // A room keeps reporting its latest notice (as the helper does) until a higher seq.
  function entry(roomId, notice) {
    if (notice) seqs[roomId] = {roomId: roomId, epoch: 1, observed: notice.seq, notice: {seq: notice.seq, kind: notice.kind,
      count: notice.count || 1, roomName: notice.roomName === undefined ? "general" : notice.roomName,
      sender: notice.sender === undefined ? "Alex" : notice.sender, snippet: notice.snippet === undefined ? "hello" : notice.snippet,
      eventId: "d".repeat(64), threadRoot: notice.threadRoot || null}}
    return seqs[roomId] || {roomId: roomId, epoch: 1, observed: 0, notice: null}
  }
  // Both rooms stay baselined at seq 0 until a notice names a higher seq.
  function accept(type, a, b) {
    var status = {generation: 1, connection: "authenticated", category: null, identity: self, relay: "wss://fixture.example/",
      catalog: {state: "partial", category: "room_catalog_partial", rooms: [
        {id: roomA, name: "general", description: "", kind: "stream", participants: [], hidden: false},
        {id: roomB, name: "Alex", description: "", kind: "dm", participants: [self, other], hidden: false}]},
      history: {state: "snapshot", roomId: roomA, rows: [], hasMore: false, category: "history_completeness_unknown"},
      activity: [entry(roomA, a), entry(roomB, b)]}
    check(service.acceptFrame(JSON.stringify({version: 1, type: type, instanceId: "notify-fixture", generation: 1,
      capabilities: ["connection_status", "room_catalog", "room_history", "room_activity"], status: status})), "valid frame rejected")
  }

  Timer {
    interval: 400
    repeat: true
    running: true
    onTriggered: {
      try {
        step++
        if (step === 1) {
          service.beginSession()
          accept("hello", null, null)
          check(service.notificationMode === "direct" && service.notificationText, "defaults are not mentions & DMs with text")
          // 1. a mention: titled by sender and room, plain snippet, escaped for the styled-text server
          accept("status", {seq: 1, kind: "mention", snippet: "hi <img src=http://x/y.png> & you"}, null)
        } else if (step === 2) {
          // 2. a thread reply: the click target carries the thread; text that looks like options stays an argument
          accept("status", {seq: 2, kind: "thread", sender: "-t", snippet: "--exec", threadRoot: thread}, null)
        } else if (step === 3) {
          // 3. general activity does not notify under "direct", nor does a repeated seq
          accept("status", {seq: 3, kind: "room"}, null)
          accept("status", {seq: 3, kind: "room"}, null)
          service.notificationMode = "mentions"
          accept("status", {seq: 4, kind: "dm"}, null)
          // 4. "all" with text off: no message content, only who wrote
          service.notificationMode = "all"
          service.notificationText = false
          accept("status", {seq: 5, kind: "room", snippet: "private words"}, null)
        } else if (step === 4) {
          // 5. a DM, coalesced burst, text on again
          service.notificationText = true
          service.notificationMode = "direct"
          accept("status", null, {seq: 1, kind: "dm", count: 2, roomName: "Alex", snippet: "yo"})
        } else if (step === 5) {
          // 6. the open, focused room is quiet; an unfocused panel is not
          service.panelOpen = true
          check(service.panelFocused && service.historyState === "snapshot" && service.selectedRoomId === roomA, "fixture room is not open")
          accept("status", {seq: 6, kind: "mention", sender: "Quiet"}, null)
          service.panelFocused = false
          accept("status", {seq: 7, kind: "mention", sender: "Sam", snippet: "ping"}, null)
          service.panelFocused = true
          // a DM in another room is not suppressed by the open room; unknown names fall back
          accept("status", null, {seq: 2, kind: "dm", sender: "", snippet: "", roomName: ""})
          service.panelOpen = false
        } else if (step === 6) {
          // 7. click targets: a joined room opens, junk and unknown rooms are ignored
          service.openNotificationTarget(roomB, "")
          check(service.selectedRoomId === roomB && service.targetRoomId === "", "click did not open the DM room")
          service.openNotificationTarget("not-a-room", "")
          service.openNotificationTarget(roomA, "xyz")
          service.openNotificationTarget("33333333-3333-4333-8333-333333333333", "")
          check(service.selectedRoomId === roomB && service.targetRoomId === "", "invalid click target changed the view")
          service.openNotificationTarget(roomA, thread)
          check(service.selectedRoomId === roomA && service.targetThreadId === thread, "thread click did not select its room")
        } else if (step === 7) {
          accept("status", null, null)
          check(service.targetRoomId === "" && service.selectedRoomId === roomA, "click target was not settled once history arrived")
          // Settings persist the mode choice but never message data.
          check(service.notificationsEnabled, "notifications unexpectedly off")
        } else if (step === 8) {
          console.log("Buzz offscreen notification check passed")
          Qt.quit()
        }
      } catch (error) { fail(error) }
    }
  }
}
