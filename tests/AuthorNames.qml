// Synthetic profiles only; never connect to a real helper or relay.
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
        var key = "a".repeat(64)
        service.selectedRoomId = room
        service.recipientsRoomId = room
        service.recipientsState = "snapshot"
        service.recipientEntries = [{key:key,name:"Buzz Person"}]
        if (service.messageAuthorLabel(key) !== "Buzz Person") throw new Error("Username not preferred")
        service.recipientEntries = [{key:key,name:"Updated name"}]
        if (service.messageAuthorName(key) !== "Updated name") throw new Error("Profile update ignored")
        service.agentProfiles = [{key:key,name:"Agent alias"}]
        if (service.messageAuthorLabel(key) !== "Updated name · Self-described agent") throw new Error("Agent classification lost")
        service.recipientsRoomId = "22222222-2222-4222-8222-222222222222"
        if (service.messageAuthorLabel(key) !== key.slice(0,12) + "…") throw new Error("Foreign room label reused")
        service.recipientsRoomId = room
        service.recipientEntries = [{key:key,name:" "}]
        if (service.messageAuthorName(key) !== key.slice(0,12) + "…") throw new Error("Blank name displayed")
        service.clearRecipients()
        if (service.messageAuthorLabel(key) !== key.slice(0,12) + "…") throw new Error("Cleared profile retained")
        console.log("PASS: message usernames, profile updates, agent labels and room-scoped fallback")
        Qt.quit()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
}
