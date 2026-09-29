// Synthetic status frames only; no relay or identity access.
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  Buzz.Service { id: service; autoConnect: false }
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
        console.log("PASS: thread root scope, stale results, helper compatibility and invalid reply rejection")
        Qt.quit()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
}
