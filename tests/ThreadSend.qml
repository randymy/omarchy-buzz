// Synthetic frames and composer requests only; no relay or identity.
import QtQuick
import Quickshell
import "plugin" as Buzz
ShellRoot {
  id: test
  property alias testedService: service
  Buzz.PanelContent { id: view; width: 1000; height: 800; service: test.testedService }
  Buzz.Service { id: service; autoConnect: false }
  Timer {
    interval: 100; running: true
    onTriggered: {
      try {
        var room = "11111111-1111-4111-8111-111111111111"
        var otherRoom = "22222222-2222-4222-8222-222222222222"
        var rootId = "1".repeat(64)
        function row(id) { return {id:id,author:"a".repeat(64),time:100,text:"Synthetic",edited:false,truncated:false,unavailable:false} }
        function frame() {
          return {version:1,type:service.instanceId === "" ? "hello" : "status",instanceId:"thread-send",generation:1,
            capabilities:["connection_status","room_catalog","room_history","thread_replies","message_send","thread_send"],
            status:{generation:1,connection:"authenticated",category:null,identity:"b".repeat(64),relay:"wss://fixture.example/",
              catalog:{state:"ready",category:null,rooms:[{id:room,name:"Fixture",description:"",kind:"stream",participants:[],hidden:false},{id:otherRoom,name:"Other",description:"",kind:"stream",participants:[],hidden:false}]},
              history:{state:"snapshot",roomId:room,rows:[row(rootId)],hasMore:false,category:"history_completeness_unknown"},
              delivery:{state:"idle",requestId:null,roomId:null,eventId:null,category:null},
              thread:{state:"snapshot",roomId:room,rootId:rootId,rows:[],hasMore:false,category:"thread_completeness_unknown"}}}
        }
        function accept(f) { if (!service.acceptFrame(JSON.stringify(f))) throw new Error("Valid frame rejected") }
        function named(item, name) {
          if (item.objectName === name) return item
          for (var i=0; i<item.children.length; ++i) { var found=named(item.children[i], name); if (found) return found }
          return null
        }
        function composer(item) { return named(item, "buzzComposer") }
        function replyComposer(item) { return named(item, "buzzThreadComposer") }
        function check(ok, message) { if (!ok) throw new Error(message) }
        service.beginSession(); accept(frame())
        service.updateDraft("Room draft")
        service.openThread(rootId); accept(frame())
        check(service.composeReply(rootId), "Reply target rejected")
        check(service.draftText === "" && replyComposer(view).text === "" && composer(view).text === "Room draft", "Room text leaked into reply")
        service.updateDraft("Thread draft")
        check(replyComposer(view).text === "Thread draft" && composer(view).text === "Room draft", "Composers do not show their own drafts")
        check(service.composeRoom() && service.draftText === "Room draft" && composer(view).text === "Room draft", "Room draft not preserved")
        check(service.composeReply(rootId) && service.draftText === "Thread draft", "Thread draft not preserved")
        // Typing in either composer makes it the destination without moving text.
        composer(view).text = "Room draft typed"
        check(service.replyRootId === "" && service.drafts[room] === "Room draft typed"
          && service.drafts[room + ":" + rootId] === "Thread draft", "Typing in the room composer changed the thread draft")
        replyComposer(view).text = "Thread draft typed"
        check(service.replyRootId === rootId && service.drafts[room + ":" + rootId] === "Thread draft typed"
          && service.drafts[room] === "Room draft typed", "Typing in the thread composer changed the room draft")
        check(service.canSendFor("") && service.canSendFor(rootId), "A composer with a valid draft cannot send")
        composer(view).text = "Room draft"
        replyComposer(view).text = "Thread draft"
        check(service.composeReply(rootId) && service.draftText === "Thread draft", "Thread draft not restored")
        var request=service.prepareSubmission()
        check(request && request.rootId === rootId && request.roomId === room && request.text === "Thread draft", "Wrong reply request")
        check(!service.composeRoom() && !service.prepareSubmission(), "In-flight destination changed or duplicate sent")
        service.losePendingDelivery()
        check(!service.composeRoom() && !service.prepareSubmission(), "Unknown delivery silently retargeted")
        service.newDraft(true)
        var oldId=request.id
        request=service.prepareSubmission()
        check(request.id !== oldId && request.rootId === rootId, "Explicit new submission lost target")
        service.applyDelivery({state:"acknowledged",requestId:request.id,roomId:room,eventId:"2".repeat(64),category:null})
        check(service.draftText === "" && service.drafts[room] === "Room draft", "Reply receipt cleared wrong draft")
        accept(frame()); service.updateDraft("Keep reply")
        var missing=frame(); missing.status.history.rows=[]
        accept(missing)
        check(service.replyRootId === rootId && service.draftText === "Keep reply" && !service.canSend, "Lost root silently changed destination")
        check(service.composeRoom() && service.draftText === "Room draft", "Explicit return failed")
        accept(frame()); service.openThread(rootId); accept(frame()); service.composeReply(rootId)
        var old=frame(); old.capabilities.pop(); accept(old)
        check(!service.threadSendSupported && !service.canSend, "Older helper allowed reply")
        service.composeRoom()
        request=service.prepareSubmission()
        check(request && !Object.prototype.hasOwnProperty.call(request,"rootId"), "Top-level request gained reply target")
        service.selectRoom(otherRoom)
        service.updateDraft("Other room draft")
        service.applyDelivery({state:"acknowledged",requestId:request.id,roomId:room,eventId:"3".repeat(64),category:null})
        check(service.deliveryScopeMismatch && service.deliveryLabel.indexOf("#Fixture") !== -1,
          "Acknowledgment in another room lost its destination")
        check(service.draftText === "Other room draft", "Acknowledgment erased another room draft")
        service.selectRoom(room)
        service.updateDraft("Uncertain room draft")
        request=service.prepareSubmission()
        check(request && request.roomId === room, "Uncertain-send setup selected wrong room")
        service.losePendingDelivery()
        service.selectRoom(otherRoom)
        check(service.deliveryScopeMismatch && service.deliveryLabel.indexOf("#Fixture") !== -1, "Previous-room receipt lost its destination")
        service.newDraft(false)
        check(service.draftText === "Other room draft", "Discarding uncertain draft erased current room")
        service.selectRoom(room)
        check(service.draftText === "" && !service.canSend, "Uncertain room draft became a new send after room switch")
        service.selectRoom(otherRoom)
        check(service.draftText === "Other room draft", "Other room draft was not preserved")
        console.log("PASS: scoped thread drafts, exact reply target, ambiguous-send lock, room-switch discard, acknowledgments, missing-root and old-helper fences")
        Qt.quit()
      } catch(error) { console.error(error); Qt.exit(1) }
    }
  }
}
