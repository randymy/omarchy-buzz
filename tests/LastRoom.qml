// Offscreen persistence check with synthetic frames; each phase runs in a fresh Quickshell process.
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  Buzz.Service { id: service; autoConnect: false }
  Timer {
    id: check
    interval: 20
    repeat: true
    running: true
    property int attempts: 0
    onTriggered: {
      if (!service.notificationSettingsDirReady) {
        if (++attempts > 100) { console.error("State directory was not prepared"); Qt.exit(1) }
        return
      }
      stop()
      try {
        var phase = Quickshell.env("BUZZ_LAST_ROOM_PHASE")
        var first = "11111111-1111-4111-8111-111111111111"
        var second = "22222222-2222-4222-8222-222222222222"
        var rooms = [{id:first,name:"First",description:""},{id:second,name:"Second",description:""}]
        if (phase === "removed") rooms.pop()
        var frame = {version:1,type:"hello",instanceId:"last-room",generation:1,
          capabilities:["connection_status","room_catalog"],
          status:{generation:1,connection:"authenticated",category:null,identity:"b".repeat(64),
            relay:phase === "foreign" ? "wss://another.example/" : "wss://fixture.example/",
            catalog:{state:"ready",category:null,rooms:rooms}}}
        service.beginSession()
        if (!service.acceptFrame(JSON.stringify(frame))) throw new Error("Valid frame rejected")
        var expected = phase === "restore" ? second : first
        if (service.selectedRoomId !== expected) throw new Error("Unexpected room in phase " + phase + ": " + service.selectedRoomId)
        if (phase === "choose") {
          service.selectRoom(second)
          if (service.selectedRoomId !== second) throw new Error("Room choice rejected")
          settled.start()
        } else Qt.quit()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer { id: settled; interval: 400; onTriggered: Qt.quit() }
}
