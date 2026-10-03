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
        // An author the member list does not know makes the service read the list
        // again (one request), but not twice within the cooldown, and not for a
        // known author or without the capability.
        service.recipientsSupported = true
        service.recipientsRoomId = room
        service.recipientsState = "snapshot"
        service.recipientEntries = [{key:key,name:"Buzz Person"}]
        var sent = service.requestSequence
        service.noteUnknownAuthors([{author: key}])
        if (service.requestSequence !== sent) throw new Error("Known author caused a member read")
        service.noteUnknownAuthors([{author: "b".repeat(64)}])
        service.noteUnknownAuthors([{author: "c".repeat(64)}])
        if (service.unknownAuthorRoom !== room) throw new Error("Unknown author not noted")
        service.recipientsSupported = false
        service.unknownAuthorRoom = ""
        service.noteUnknownAuthors([{author: "d".repeat(64)}])
        if (service.unknownAuthorRoom !== "") throw new Error("Member read attempted without the capability")
        // Names the helper served stay while the community is the same: a member
        // list that reloads, fails, belongs to another room or is cut at its bound
        // must not turn a known name back into a key.
        var fdax = "f".repeat(64)
        service.rememberNames([{key: fdax, name: "FDAX"}, {key: "e".repeat(64), name: "  "}])
        service.clearRecipients()
        if (service.messageAuthorName(fdax) !== "FDAX") throw new Error("Known name lost when the member list cleared")
        if (service.personName(fdax) !== "FDAX") throw new Error("Known name missing from people labels")
        if (service.messageAuthorName("e".repeat(64)) !== "eeeeeeeeeeee…") throw new Error("Blank name remembered")
        service.recipientsRoomId = room
        service.recipientsState = "snapshot"
        service.recipientEntries = [{key: fdax, name: "FDAX renamed"}]
        if (service.messageAuthorName(fdax) !== "FDAX renamed") throw new Error("Current member list not preferred")
        service.recipientsRoomId = "22222222-2222-4222-8222-222222222222"
        if (service.messageAuthorName(fdax) !== "FDAX") throw new Error("Known name lost in another room")
        service.rememberNames([{key: fdax, name: "FDAX renamed"}])
        if (service.messageAuthorName(fdax) !== "FDAX renamed") throw new Error("Newer name not remembered")
        var many = []
        for (var n = 0; n < service.knownNamesLimit + 5; n++) many.push({key: ("000" + n).slice(-4).repeat(16), name: "P" + n})
        service.rememberNames(many)
        if (service.knownNameOrder.length !== service.knownNamesLimit || service.knownNames[fdax] !== undefined
            || service.knownNames[many[many.length - 1].key] !== "P" + (many.length - 1)) throw new Error("Name cache not bounded oldest-first")
        service.forgetNames("wss://other.example|" + "b".repeat(64))
        if (service.messageAuthorName(many[many.length - 1].key) !== many[many.length - 1].key.slice(0, 12) + "…")
          throw new Error("Names kept across communities")
        console.log("PASS: message usernames, profile updates, agent labels, room-scoped fallback, a member re-read for unknown authors, and served names kept per community")
        Qt.quit()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
}
