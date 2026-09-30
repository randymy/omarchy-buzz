// Synthetic status frames only; no relay or identity access.
// Replays the helper's periodic joined-room check as recorded from 0.0.8.
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
  readonly property string room: "11111111-1111-4111-8111-111111111111"
  readonly property string otherRoom: "22222222-2222-4222-8222-222222222222"
  readonly property string rootId: "1".repeat(64)
  readonly property string author: "a".repeat(64)
  property var retainedToggle: null
  property real retainedScroll: 0
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function row(id, text) { return {id:id,author:author,time:100,text:text,edited:false,truncated:false,unavailable:false} }
  function rooms() { return [{id:room,name:"Fixture",description:""},{id:otherRoom,name:"Other",description:""}] }
  function history() {
    var rows = []
    for (var i = 2; i <= 15; i++) rows.push(row(i.toString(16).repeat(64), "Synthetic message"))
    rows.push(row(rootId, "Synthetic root"))
    return {state:"snapshot",roomId:room,rows:rows,hasMore:false,category:"history_completeness_unknown"}
  }
  function thread() {
    var reply = row("f".repeat(64), "Synthetic reply")
    reply.depth = 1
    reply.parent = rootId
    return {state:"snapshot",roomId:room,rootId:rootId,rows:[reply],
      hasMore:false,category:"thread_completeness_unknown"}
  }
  function recipients() { return {state:"snapshot",roomId:room,entries:[{key:author,name:"Fixture Person"}],partial:false,category:null} }
  function noHistory() { return {state:"unavailable",roomId:null,rows:[],hasMore:null,category:null} }
  function noThread(category) { return {state:"unavailable",roomId:null,rootId:null,rows:[],hasMore:null,category:category || null} }
  function noRecipients() { return {state:"unavailable",roomId:null,entries:[],partial:false,category:null} }
  function frame(views) {
    return {version:1,type:service.instanceId === "" ? "hello" : "status",instanceId:"catalog-fixture",generation:1,
      capabilities:["connection_status","room_catalog","room_history","thread_replies","room_recipients"],
      status:{generation:1,connection:"authenticated",category:null,identity:"b".repeat(64),relay:"wss://fixture.example/",
        catalog:views.catalog || {state:"partial",category:"room_catalog_partial",rooms:rooms()},
        history:views.history || history(), thread:views.thread || noThread(), recipients:views.recipients || recipients()}}
  }
  function accept(views) { if (!service.acceptFrame(JSON.stringify(frame(views)))) throw new Error("Valid frame rejected") }
  function settle() {
    service.beginSession()
    accept({})
    service.openThread(rootId)
    accept({thread:thread()})
    if (service.threadState !== "snapshot" || service.recipientsState !== "snapshot") throw new Error("Baseline not displayed")
  }
  function blank() { accept({catalog:{state:"loading",category:null,rooms:[]},history:noHistory(),recipients:noRecipients()}) }
  Timer {
    id: checkRendered
    interval: 100
    onTriggered: {
      try {
        var scroll = test.findNamed(view, "buzzHistoryScroll", [])[0]
        if (test.findNamed(view, "buzzThreadToggle", []).indexOf(test.retainedToggle) === -1)
          throw new Error("Joined-room check rebuilt the conversation")
        if (Math.abs(scroll.contentItem.contentY - test.retainedScroll) > 1)
          throw new Error("Joined-room check moved the reader's scroll position")
        if (!test.findNamed(view, "buzzThreadPanel", [])[0].visible
            || test.findNamed(view, "buzzThreadReply", []).filter(function(item) { return item.visible && item.height > 0 }).length !== 1)
          throw new Error("Joined-room check hid the open thread")
        console.log("PASS: periodic joined-room check keeps rooms, conversation, names, scroll and open thread; failures still clear")
        Qt.quit()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer {
    id: replayRefresh
    interval: 100
    onTriggered: {
      try {
        var scroll = test.findNamed(view, "buzzHistoryScroll", [])[0]
        // A reader who scrolled up to older messages must stay there.
        scroll.contentItem.contentY = Math.floor((scroll.contentItem.contentHeight - scroll.contentItem.height) / 2)
        test.retainedScroll = scroll.contentItem.contentY
        if (test.retainedScroll <= 0) throw new Error("Baseline conversation is not scrollable")
        test.retainedToggle = test.findNamed(view, "buzzThreadToggle", []).filter(function(item) {
          return item.messageId === test.rootId
        })[0]
        if (!test.retainedToggle) throw new Error("Baseline conversation not rendered")
        var kept = {rooms:service.catalogRooms, history:service.historyRows, replies:service.threadRows, people:service.recipientEntries}
        function unchanged(step) {
          if (service.catalogRooms !== kept.rooms || service.catalogState !== "partial" || service.selectedRoomId !== test.room
              || service.historyRows !== kept.history || service.historyState !== "snapshot"
              || service.threadRows !== kept.replies || service.threadState !== "snapshot" || service.threadRootId !== test.rootId
              || service.recipientEntries !== kept.people || service.recipientsState !== "snapshot"
              || service.messageAuthorName(test.author) !== "Fixture Person")
            throw new Error("Displayed state changed at: " + step)
        }
        test.blank()
        unchanged("rooms being checked")
        if (service.resyncStage !== "catalog") throw new Error("Joined-room check not recognized")
        test.accept({history:test.noHistory(),recipients:test.noRecipients()})
        unchanged("rooms confirmed")
        if (service.resyncStage !== "history") throw new Error("History was not requested after the room check")
        test.accept({history:{state:"loading",roomId:test.room,rows:[],hasMore:null,category:null},recipients:test.noRecipients()})
        unchanged("history loading")
        test.accept({recipients:test.noRecipients()})
        unchanged("history restored")
        if (service.resyncStage !== "recipients") throw new Error("Recipients were not requested after history")
        test.accept({recipients:{state:"loading",roomId:test.room,entries:[],partial:false,category:null}})
        unchanged("recipients loading")
        test.accept({})
        unchanged("recipients restored")
        if (service.resyncStage !== "thread") throw new Error("Open thread was not requested after recipients")
        test.accept({thread:{state:"loading",roomId:test.room,rootId:test.rootId,rows:[],hasMore:null,category:null}})
        unchanged("thread loading")
        test.accept({thread:test.thread()})
        unchanged("thread restored")
        if (service.resyncStage !== "") throw new Error("Joined-room check did not finish")
        checkRendered.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer {
    interval: 100
    running: true
    onTriggered: {
      try {
        // Removal from the room list still clears the conversation.
        test.settle()
        test.blank()
        test.accept({catalog:{state:"partial",category:"room_catalog_partial",rooms:[test.rooms()[1]]},
          history:test.noHistory(),recipients:test.noRecipients()})
        if (service.selectedRoomId !== test.otherRoom || service.historyRows.length || service.threadRootId
            || service.recipientEntries.length || service.resyncStage !== "")
          throw new Error("Removed room stayed on screen")
        // A failed room check still clears everything it could not confirm.
        test.settle()
        test.blank()
        test.accept({catalog:{state:"unavailable",category:"room_catalog_timeout",rooms:[]},
          history:test.noHistory(),recipients:test.noRecipients()})
        if (service.catalogRooms.length || service.historyRows.length || service.threadRootId || service.resyncStage !== "")
          throw new Error("Failed room check kept unconfirmed rooms")
        // A history error after the check is shown, not hidden behind the old snapshot.
        test.settle()
        test.blank()
        test.accept({history:test.noHistory(),recipients:test.noRecipients()})
        test.accept({history:{state:"unavailable",roomId:test.room,rows:[],hasMore:null,category:"history_access_denied"},
          recipients:test.noRecipients()})
        if (service.historyRows.length || service.threadRootId || service.historyCategory !== "history_access_denied" || service.resyncStage !== "")
          throw new Error("History error stayed hidden")
        // A refused thread read closes the thread.
        test.settle()
        test.blank()
        test.accept({history:test.noHistory(),recipients:test.noRecipients()})
        test.accept({recipients:test.noRecipients()})
        test.accept({})
        test.accept({thread:test.noThread("thread_access_denied")})
        if (service.threadRootId || service.threadRows.length || service.historyState !== "snapshot" || service.resyncStage !== "")
          throw new Error("Refused thread stayed open")
        // Disconnects and new helper generations are never quiet.
        test.settle()
        test.blank()
        var offline = test.frame({catalog:{state:"unavailable",category:null,rooms:[]},history:test.noHistory(),recipients:test.noRecipients()})
        offline.status.connection = "connecting"
        if (!service.acceptFrame(JSON.stringify(offline)) || service.historyRows.length || service.catalogRooms.length || service.resyncStage !== "")
          throw new Error("Disconnect kept the conversation")
        // Refreshing a roster that is on screen keeps its names.
        test.settle()
        service.refreshRecipients()
        if (service.recipientsState !== "snapshot" || service.messageAuthorName(test.author) !== "Fixture Person")
          throw new Error("Roster refresh hid names")
        test.settle()
        replayRefresh.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
}
