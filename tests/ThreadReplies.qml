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
  property var retainedToggle: null
  property real retainedScroll: 0
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(name) { return test.findNamed(view, name, []).filter(function(item) { return item.visible && item.height > 0 }) }
  function atNewest(scroll) {
    return Math.abs(scroll.contentItem.contentY - (scroll.contentItem.contentHeight - scroll.contentItem.height)) <= 1
  }
  function incoming(id, text) {
    test.crowdedFrame.status.history.rows.push({id:id.repeat(32),author:"a".repeat(64),time:102,
      text:text,edited:false,truncated:false,unavailable:false})
    if (!service.acceptFrame(JSON.stringify(test.crowdedFrame))) throw new Error("New message snapshot rejected")
  }
  Timer {
    id: checkClosed
    interval: 100
    onTriggered: {
      try {
        if (test.shown("buzzThreadPanel").length || test.shown("buzzThreadReply").length)
          throw new Error("Closed thread stayed on screen")
        var summarized = JSON.parse(JSON.stringify(test.crowdedFrame))
        summarized.capabilities.push("thread_summaries")
        var rows = summarized.status.history.rows
        rows[0].thread = {replies:2, lastReplyAt:150, participants:["c".repeat(64)]}
        rows[1].thread = {replies:1, lastReplyAt:null, participants:[]}
        if (!service.acceptFrame(JSON.stringify(summarized)) || !service.threadSummariesSupported)
          throw new Error("Thread summary snapshot rejected")
        test.summaryIds = [rows[0].id, rows[1].id]
        checkSummaries.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  property var summaryIds: []
  Timer {
    id: checkSummaries
    interval: 100
    onTriggered: {
      try {
        var toggles = test.findNamed(view, "buzzThreadToggle", [])
        function toggle(id) { return toggles.filter(function(item) { return item.messageId === id })[0] }
        var two = toggle(test.summaryIds[0])
        var one = toggle(test.summaryIds[1])
        if (!two || two.text !== "2 replies ›" || !two.visible)
          throw new Error("Summary count not rendered on its row: " + (two ? two.text : "missing"))
        if (two.tooltipText !== "Last reply " + service.formatTimestamp(150))
          throw new Error("Last reply time not offered: " + two.tooltipText)
        if (!one || one.text !== "1 reply ›" || one.tooltipText !== "Open replies beside the room")
          throw new Error("Singular summary or unknown recency mislabelled: " + (one ? one.text : "missing"))
        var quiet = toggles.filter(function(item) { return test.summaryIds.indexOf(item.messageId) === -1 })
        if (!quiet.length || !quiet.every(function(item) { return item.text === "Reply ›" }))
          throw new Error("Rows without a summary did not offer a plain reply")
        console.log("PASS: thread opens beside the room with its replies; snapshots keep delegates and the open thread; the view follows the newest message only for a reader at the end; relay reply counts label their rows")
        Qt.quit()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer {
    id: checkReaderPosition
    interval: 100
    onTriggered: {
      try {
        var scroll = test.findNamed(view, "buzzHistoryScroll", [])[0]
        if (Math.abs(scroll.contentItem.contentY - test.retainedScroll) > 1)
          throw new Error("Incoming message moved a reader who had scrolled up")
        if (test.findNamed(view, "buzzThreadToggle", []).indexOf(test.retainedToggle) === -1)
          throw new Error("Incoming message rebuilt existing delegates")
        if (service.threadRootId !== test.openedRoot || service.threadState !== "snapshot" || test.shown("buzzThreadReply").length !== 1)
          throw new Error("Incoming message interrupted the open thread")
        test.findNamed(view, "buzzCloseThread", [])[0].clicked()
        if (service.threadRootId !== "") throw new Error("Close did not close the thread")
        checkClosed.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer {
    id: checkFollow
    interval: 100
    onTriggered: {
      try {
        var scroll = test.findNamed(view, "buzzHistoryScroll", [])[0]
        if (!test.atNewest(scroll)) throw new Error("Reader at the end did not follow the newest message")
        if (test.findNamed(view, "buzzThreadToggle", []).indexOf(test.retainedToggle) === -1)
          throw new Error("Incoming message rebuilt existing delegates")
        scroll.contentItem.contentY = 0
        test.retainedScroll = 0
        test.incoming("9a", "Second incoming message")
        checkReaderPosition.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
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
            + " viewport=" + (scroll ? scroll.contentItem.height : "missing"))
        if (!test.atNewest(scroll)) throw new Error("Conversation did not open at its newest message")
        var reactions = test.findNamed(scroll, "buzzMessageReactions", [])
        if (reactions.length !== 15 || !reactions[reactions.length - 1].visible)
          throw new Error("Snapshot reactions were not rendered on their message")
        if (service.threadSummariesSupported || !toggles.every(function(item) { return item.text === "Thread ›" }))
          throw new Error("Helper without thread summaries did not keep the neutral thread label")
        if (test.shown("buzzThreadPanel").length) throw new Error("Thread panel shown without an open thread")
        toggles[toggles.length - 1].clicked()
        if (service.threadRootId !== test.openedRoot || service.threadState !== "loading")
          throw new Error("Thread toggle did not open the selected root")
        test.crowdedFrame.status.thread = {state:"snapshot", roomId:test.crowdedFrame.status.history.roomId,
          rootId:test.openedRoot, rows:[{id:"f".repeat(64),author:"a".repeat(64),time:101,
            text:"Visible synthetic reply",edited:false,truncated:false,unavailable:false}],
          hasMore:false,category:"thread_completeness_unknown"}
        test.crowdedFrame.type="status"
        if (!service.acceptFrame(JSON.stringify(test.crowdedFrame))) throw new Error("Reply snapshot rejected")
        checkThreadPanel.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer {
    id: checkThreadPanel
    interval: 100
    onTriggered: {
      try {
        var scroll = test.findNamed(view, "buzzHistoryScroll", [])[0]
        var panel = test.shown("buzzThreadPanel")[0]
        if (!panel || test.shown("buzzThreadRoot").length !== 1 || test.shown("buzzThreadReply").length !== 1
            || service.threadRows.length !== 1 || service.threadCountLabel !== "1 reply")
          throw new Error("Open thread did not show its root and reply")
        if (!scroll.visible || scroll.width <= 0 || panel.mapToItem(view, 0, 0).x <= scroll.mapToItem(view, 0, 0).x)
          throw new Error("Thread did not open to the right of a visible room")
        if (test.shown("buzzThreadToggle").length !== 15) throw new Error("Opening a thread changed the room's messages")
        test.retainedToggle = test.findNamed(view, "buzzThreadToggle", []).filter(function(item) {
          return item.messageId === test.openedRoot
        })[0]
        if (!test.retainedToggle || !test.retainedToggle.selected) throw new Error("Open thread is not marked on its message")
        if (!service.acceptFrame(JSON.stringify(test.crowdedFrame))) throw new Error("Repeated snapshot rejected")
        scroll.toNewest()
        test.incoming("0a", "New incoming message")
        checkFollow.start()
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
              catalog:{state:"partial",category:"room_catalog_partial",rooms:[{id:room,name:"Fixture",description:"",kind:"stream",participants:[],hidden:false}]},
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
        var retainedHistory = service.historyRows
        var retainedReplies = service.threadRows
        var retainedCatalog = service.catalogRooms
        accept(snapshot(frame(),rootId))
        if (service.historyRows !== retainedHistory || service.threadRows !== retainedReplies || service.catalogRooms !== retainedCatalog)
          throw new Error("Identical background snapshot replaced displayed models")
        service.refreshHistory()
        var historyLoading = snapshot(frame(),rootId)
        historyLoading.status.history = {state:"loading",roomId:room,rows:[],hasMore:null,category:null}
        accept(historyLoading)
        if (service.historyRows !== retainedHistory || service.historyState !== "snapshot" || service.threadRootId !== rootId)
          throw new Error("History refresh interrupted visible conversation")
        accept(snapshot(frame(),rootId))
        service.refreshThread()
        if (service.threadState !== "snapshot" || service.threadRows !== retainedReplies)
          throw new Error("Background refresh cleared visible replies")
        var loading = frame()
        loading.status.thread = {state:"loading",roomId:room,rootId:rootId,rows:[],hasMore:null,category:null}
        accept(loading)
        if (service.threadState !== "snapshot" || service.threadRows !== retainedReplies)
          throw new Error("Refresh loading frame cleared visible replies")
        accept(snapshot(frame(),rootId))
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
        var summaryFrame=frame()
        summaryFrame.capabilities.push("thread_summaries")
        summaryFrame.status.history.rows[0].thread={replies:2,lastReplyAt:150,participants:["c".repeat(64),"d".repeat(64)],extra:true}
        accept(summaryFrame)
        var kept=service.historyRows[0].thread
        if (!service.threadSummariesSupported || kept.replies !== 2 || kept.lastReplyAt !== 150
            || kept.participants.length !== 2 || kept.extra !== undefined)
          throw new Error("Valid thread summary was not carried into the row")
        var unknown=frame(); unknown.capabilities.push("thread_summaries")
        unknown.status.history.rows[0].thread=null
        accept(unknown)
        if (service.historyRows[0].thread !== null) throw new Error("Absent thread summary was invented")
        var badSummaries=[{replies:-1,lastReplyAt:null,participants:[]},
          {replies:1000001,lastReplyAt:null,participants:[]},
          {replies:1.5,lastReplyAt:null,participants:[]},
          {replies:"2",lastReplyAt:null,participants:[]},
          {replies:2,lastReplyAt:-1,participants:[]},
          {replies:2,lastReplyAt:253402300800,participants:[]},
          {replies:2,participants:[]},
          {replies:2,lastReplyAt:null},
          {replies:2,lastReplyAt:null,participants:["not-a-key"]},
          {replies:2,lastReplyAt:null,participants:["c".repeat(64),"c".repeat(64)]},
          {replies:2,lastReplyAt:null,participants:"c".repeat(64)},
          {replies:2,lastReplyAt:null,participants:Array.from({length:11},function(_,i){return i.toString(16).repeat(64)})},
          [2], 2, "2 replies"]
        for (var b=0;b<badSummaries.length;b++) {
          service.beginSession()
          var badSummary=frame(); badSummary.capabilities.push("thread_summaries")
          badSummary.status.history.rows[0].thread=badSummaries[b]
          if (service.acceptFrame(JSON.stringify(badSummary)) || !service.sessionFailed || service.historyRows.length)
            throw new Error("Malformed thread summary accepted: " + JSON.stringify(badSummaries[b]))
        }
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
