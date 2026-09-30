// Synthetic status frames only; no relay or identity access.
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  id: test
  Buzz.Service { id: service; autoConnect: false }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 520
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  property var crowdedFrame: null
  property string openedRoot: ""
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  Timer {
    id: openLastThread
    interval: 100
    onTriggered: {
      try {
        var scroll = test.findNamed(view, "buzzHistoryScroll", [])[0]
        var toggles = test.findNamed(view, "buzzThreadToggle", [])
        if (!scroll || toggles.length !== 15 || scroll.contentItem.contentHeight <= scroll.contentItem.height)
          throw new Error("Crowded history did not render in a scrollable viewport: toggles=" + toggles.length
            + " content=" + (scroll ? scroll.contentItem.contentHeight : "missing")
            + " viewport=" + (scroll ? scroll.contentItem.height : "missing")
            + " row=" + (toggles.length ? toggles[0].parent.height : "missing")
            + " button=" + (toggles.length ? toggles[0].height : "missing"))
        var reactions = test.findNamed(view, "buzzMessageReactions", [])
        if (reactions.length !== 15 || !reactions[reactions.length - 1].visible)
          throw new Error("Snapshot reactions were not rendered on their message")
        scroll.contentItem.contentY = scroll.contentItem.contentHeight - scroll.contentItem.height
        toggles[toggles.length - 1].clicked()
        if (service.threadRootId !== test.openedRoot || service.threadState !== "loading")
          throw new Error("Thread toggle did not open the selected root")
        test.crowdedFrame.status.thread = {state:"snapshot", roomId:test.crowdedFrame.status.history.roomId,
          rootId:test.openedRoot, rows:[{id:"f".repeat(64),author:"a".repeat(64),time:101,
            text:"Visible synthetic reply",edited:false,truncated:false,unavailable:false}],
          hasMore:false,category:"thread_completeness_unknown"}
        test.crowdedFrame.type="status"
        if (!service.acceptFrame(JSON.stringify(test.crowdedFrame))) throw new Error("Reply snapshot rejected")
        checkThreadViewport.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer {
    id: checkThreadViewport
    interval: 100
    onTriggered: {
      try {
        var scroll = test.findNamed(view, "buzzHistoryScroll", [])[0]
        var details = test.findNamed(view, "buzzThreadDetails", []).filter(function(item) { return item.visible })[0]
        if (!details || details.height <= 0 || service.threadRows.length !== 1)
          throw new Error("Open thread has no visible reply block")
        var y = details.mapToItem(scroll, 0, 0).y
        if (y < -20 || y >= scroll.height)
          throw new Error("Open thread remained outside the visible scroll viewport: " + y
            + " scroll=" + scroll.height + " contentY=" + scroll.contentItem.contentY
            + " contentHeight=" + scroll.contentItem.contentHeight
            + " available=" + scroll.availableHeight + " viewport=" + scroll.contentItem.height
            + " rowY=" + details.parent.y + " detailsY=" + details.y)
        console.log("PASS: thread root scope, stale results, invalid reply rejection, and expanded replies in viewport")
        Qt.quit()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer {
    interval: 100
    running: true
    onTriggered: {
      try {
        var room = "11111111-1111-4111-8111-111111111111"
        var rootId = "1".repeat(64)
        var replyId = "2".repeat(64)
        function row(id) { return {id:id,author:"a".repeat(64),time:100,text:"Synthetic reply",edited:false,truncated:false,unavailable:false} }
        function frame() {
          return {version:1,type:service.instanceId === "" ? "hello" : "status",instanceId:"thread-fixture",generation:1,
            capabilities:["connection_status","room_catalog","room_history","thread_replies"],
            status:{generation:1,connection:"authenticated",category:null,identity:"b".repeat(64),relay:"wss://fixture.example/",
              catalog:{state:"partial",category:"room_catalog_partial",rooms:[{id:room,name:"Fixture",description:""}]},
              history:{state:"snapshot",roomId:room,rows:[row(rootId)],hasMore:false,category:"history_completeness_unknown"},
              thread:{state:"unavailable",roomId:null,rootId:null,rows:[],hasMore:null,category:null}}}
        }
        function accept(value) { if (!service.acceptFrame(JSON.stringify(value))) throw new Error("Valid thread frame rejected") }
        function snapshot(value, id) {
          value.status.thread={state:"snapshot",roomId:room,rootId:id,rows:[row(replyId)],hasMore:false,category:"thread_completeness_unknown"}
          return value
        }
        service.beginSession()
        accept(frame())
        if (!service.canOpenThread(rootId) || service.canOpenThread("f".repeat(64))) throw new Error("Root selection not guarded")
        service.openThread(rootId)
        service.pendingThreadRequestId="ui-1"
        if (!service.acceptFrame(JSON.stringify({version:1,type:"error",instanceId:"thread-fixture",id:"ui-1",category:"request_busy"}))
            || service.threadState !== "loading" || service.threadRetryBudget !== 1)
          throw new Error("Busy thread lookup did not retain scoped retry")
        accept(snapshot(frame(),rootId))
        if (service.threadRows.length !== 1 || service.threadState !== "snapshot") throw new Error("Reply not displayed")
        service.openThread(rootId)
        accept(snapshot(frame(),"3".repeat(64)))
        if (service.threadRows.length || service.threadState !== "loading") throw new Error("Late foreign thread accepted")
        accept(snapshot(frame(),rootId))
        var failed=frame(); failed.status.thread={state:"unavailable",roomId:room,rootId:rootId,rows:[],hasMore:null,category:"thread_timeout"}
        accept(failed)
        if (service.threadState !== "unavailable" || service.threadRows.length || service.threadRootId !== rootId) throw new Error("Read error did not preserve retry scope")
        var removed=frame(); removed.status.history.rows=[]
        accept(removed)
        if (service.threadRootId || service.threadRows.length) throw new Error("Missing root retained replies")
        accept(frame()); service.openThread(rootId); accept(snapshot(frame(),rootId))
        var older=frame(); older.capabilities.pop(); delete older.status.thread
        accept(older)
        if (service.threadSupported || service.threadRows.length) throw new Error("Older helper retained replies")
        accept(frame()); service.openThread(rootId); accept(snapshot(frame(),rootId))
        var changed=frame(); changed.generation=2; changed.status.generation=2
        accept(snapshot(changed,rootId))
        if (service.threadRootId || service.threadRows.length) throw new Error("Generation change retained replies")
        service.openThread(rootId); accept(snapshot(changed,rootId))
        var disconnected=frame(); disconnected.generation=2; disconnected.status.generation=2; disconnected.status.connection="connecting"
        accept(disconnected)
        if (service.threadRootId || service.threadRows.length) throw new Error("Disconnect retained replies")
        service.beginSession(); accept(frame()); service.openThread(rootId)
        var bad=snapshot(frame(),rootId); bad.status.thread.rows[0].id=rootId
        if (service.acceptFrame(JSON.stringify(bad)) || !service.sessionFailed || service.threadRows.length) throw new Error("Root accepted as its own reply")
        service.beginSession(); accept(frame()); service.openThread(rootId)
        bad=snapshot(frame(),rootId); bad.status.thread.rows[0].author="invalid"
        if (service.acceptFrame(JSON.stringify(bad)) || !service.sessionFailed) throw new Error("Malformed reply accepted")
        service.beginSession()
        var reactionFrame=frame()
        reactionFrame.status.history.rows[0].reactions={seen:1,working:2}
        accept(reactionFrame)
        if (service.historyRows[0].reactions.seen !== 1 || service.historyRows[0].reactions.working !== 2)
          throw new Error("Validated snapshot reactions were lost")
        var badCounts=frame()
        badCounts.status.history.rows[0].reactions={seen:201,working:0}
        if (service.acceptFrame(JSON.stringify(badCounts)) || !service.sessionFailed)
          throw new Error("Out-of-range reaction count accepted")
        service.beginSession()
        var crowded=frame()
        crowded.status.history.rows=[]
        for (var i=2;i<=15;i++) crowded.status.history.rows.push(row(i.toString(16).repeat(64)))
        crowded.status.history.rows.push(row(rootId))
        crowded.status.history.rows[crowded.status.history.rows.length - 1].reactions={seen:1,working:2}
        accept(crowded)
        test.crowdedFrame=crowded
        test.openedRoot=rootId
        openLastThread.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
}
