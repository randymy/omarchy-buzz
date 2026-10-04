// Synthetic profiles only; never connect to a real helper or relay.
import QtQuick
import Quickshell
import Quickshell.Io
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
        // A directory read the helper repeats in later frames is not new evidence:
        // it must not undo a newer name learned from a member list.
        var directory = {state: "snapshot", requestId: "people-1", entries: [{key: fdax, name: "FDAX old"}], category: ""}
        service.applyPeople(directory)
        if (service.knownNames[fdax] !== "FDAX old") throw new Error("Served directory name not remembered")
        service.rememberNames([{key: fdax, name: "FDAX renamed"}])
        service.applyPeople(directory)
        if (service.knownNames[fdax] !== "FDAX renamed") throw new Error("Repeated directory read undid a newer name")
        var many = []
        for (var n = 0; n < service.knownNamesLimit + 5; n++) many.push({key: ("000" + n).slice(-4).repeat(16), name: "P" + n})
        service.rememberNames(many)
        if (service.knownNameOrder.length !== service.knownNamesLimit || service.knownNames[fdax] !== undefined
            || service.knownNames[many[many.length - 1].key] !== "P" + (many.length - 1)) throw new Error("Name cache not bounded oldest-first")
        service.forgetNames("wss://other.example|" + "b".repeat(64))
        if (service.messageAuthorName(many[many.length - 1].key) !== many[many.length - 1].key.slice(0, 12) + "…")
          throw new Error("Names kept across communities")
        // The persisted cache: used only for the community it was stored for, and
        // only when it validates (hex keys, plain bounded names, at most 1000).
        var scopeA = "wss://fixture.example/|" + "a".repeat(64)
        var scopeB = "wss://fixture.example/|" + "b".repeat(64)
        var cached = {version: 1, scope: scopeA, names: [{key: "1".repeat(64), name: "Cached One"}, {key: "2".repeat(64), name: "Cached Two"}]}
        function clearPersisted() { service.persistedNames = {scope: "", entries: []}; service.forgetNames("") }
        clearPersisted()
        service.loadNames(JSON.stringify(cached))
        service.forgetNames(scopeB)
        if (service.messageAuthorName("1".repeat(64)) !== "111111111111…") throw new Error("Persisted names used for another community")
        service.forgetNames(scopeA)
        if (service.messageAuthorName("1".repeat(64)) !== "Cached One" || service.personName("2".repeat(64)) !== "Cached Two")
          throw new Error("Persisted names not used for the same community")
        // A name learned since the file was read wins over the stored one.
        service.forgetNames(scopeB)
        service.forgetNames(scopeA)
        service.rememberNames([{key: "1".repeat(64), name: "Fresh One"}])
        if (service.knownNames["1".repeat(64)] !== "Fresh One" || service.knownNames["2".repeat(64)] !== "Cached Two")
          throw new Error("Stored name replaced a newer one")
        // Corrupt or hostile files are ignored whole.
        var bad = ["not json", "", "[]", "null"]
        bad.push(JSON.stringify({version: 2, scope: scopeA, names: cached.names}))
        bad.push(JSON.stringify({version: 1, scope: "no-pipe", names: cached.names}))
        bad.push(JSON.stringify({version: 1, scope: scopeA, names: "x"}))
        bad.push(JSON.stringify({version: 1, scope: scopeA, names: [{key: "short", name: "Bad key"}]}))
        bad.push(JSON.stringify({version: 1, scope: scopeA, names: [{key: "1".repeat(64), name: "Fine"}, {key: "2".repeat(64), name: "Re‮versed"}]}))
        bad.push(JSON.stringify({version: 1, scope: scopeA, names: [{key: "1".repeat(64), name: "Tab\there"}]}))
        bad.push(JSON.stringify({version: 1, scope: scopeA, names: [{key: "1".repeat(64), name: "x".repeat(65)}]}))
        bad.push(JSON.stringify({version: 1, scope: scopeA, names: [{key: "1".repeat(64), name: "  "}]}))
        bad.push(JSON.stringify({version: 1, scope: scopeA, names: [{key: "1".repeat(64), name: "Dup"}, {key: "1".repeat(64), name: "Dup"}]}))
        var tooMany = []
        for (var m = 0; m < service.knownNamesLimit + 1; m++) tooMany.push({key: ("000" + m).slice(-4).repeat(16), name: "P" + m})
        bad.push(JSON.stringify({version: 1, scope: scopeA, names: tooMany}))
        for (var b = 0; b < bad.length; b++) {
          clearPersisted()
          service.forgetNames(scopeA)
          service.loadNames(bad[b])
          if (service.persistedNames.scope !== "" || Object.keys(service.knownNames).length !== 0) throw new Error("Corrupt names file used: " + bad[b].slice(0, 60))
        }
        // Leave this service without a community: only the services below store names.
        clearPersisted()
        // The helper's author profiles are checked like served names.
        if (service.validatedProfiles([{key: "x", name: "Bad"}]) !== null || service.validatedProfiles([{key: "1".repeat(64), name: "Re‮versed"}]) !== null
            || service.validatedProfiles("no") !== null || service.validatedProfiles([{key: "1".repeat(64), name: "Ok"}]).length !== 1)
          throw new Error("Author profiles not validated")
        console.log("PASS: message usernames, profile updates, agent labels, room-scoped fallback, a member re-read for unknown authors, served names kept per community, and the persisted cache")
        retryCheck.running = true
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }

  // Transient member-list failures keep retrying (fast delays here) until the
  // list arrives; an access denial never retries; helper profiles feed the names.
  Buzz.Service { id: retries; autoConnect: false; recipientsRetryDelays: [30, 30, 30, 60] }
  FileView { id: storedNames; path: Quickshell.env("XDG_STATE_HOME") + "/omarchy-buzz/names.json"; watchChanges: false; printErrors: false }
  Timer {
    id: retryCheck
    interval: 20
    repeat: true
    property int stage: 0
    property int waited: 0
    readonly property string roomA: "11111111-1111-4111-8111-111111111111"
    readonly property string person: "c".repeat(64)
    readonly property string outsider: "d".repeat(64)
    function accept(target, recipients, options) {
      options = options || {}
      var status = {generation: 1, connection: "authenticated", category: null, identity: "a".repeat(64), relay: "wss://fixture.example/",
        catalog: {state: "partial", category: "room_catalog_partial", rooms: [{id: roomA, name: "First", description: "", kind: "stream", participants: [], hidden: false}]},
        recipients: recipients}
      if (options.profiles) status.profiles = options.profiles
      var caps = ["connection_status", "room_catalog", "room_recipients"].concat(options.profiles ? ["author_profiles"] : [])
      return target.acceptFrame(JSON.stringify({version: 1, type: options.type || "status", instanceId: "names-fixture", generation: 1, capabilities: caps, status: status}))
    }
    function failed(category) { return {state: "unavailable", roomId: roomA, entries: [], agents: [], partial: true, category: category} }
    function start(target) {
      target.beginSession()
      if (!accept(target, {state: "unavailable", roomId: null, entries: [], agents: [], partial: true, category: null}, {type: "hello"})) throw new Error("Hello rejected")
      if (target.recipientsState !== "loading" || target.selectedRoomId !== roomA) throw new Error("Member list read not started: " + target.recipientsState)
    }
    onTriggered: {
      try {
        waited += 20
        if (waited > 4000) throw new Error("Stage " + stage + " timed out")
        if (stage === 0) {
          start(retries)
          // The first failure is a busy error frame for the read in flight.
          retries.pendingRecipientsRequestId = "ui-9"
          if (!retries.acceptFrame(JSON.stringify({version: 1, type: "error", category: "request_busy", id: "ui-9", instanceId: "names-fixture"}))) throw new Error("Busy frame rejected")
          if (retries.recipientsRetryAttempt !== 1) throw new Error("Busy read not retried")
          stage = 1; waited = 0
        } else if (stage === 1 || stage === 2 || stage === 3) {
          // Each retry fires on its own, and the helper keeps reporting the failure.
          if (retries.recipientsState !== "loading") return
          if (retries.recipientsRetryAttempt !== stage) throw new Error("Unexpected attempt " + retries.recipientsRetryAttempt + " at stage " + stage)
          if (!accept(retries, failed(stage === 2 ? "recipients_timeout" : "recipients_unavailable"))) throw new Error("Failure status rejected")
          if (retries.recipientsRetryAttempt !== stage + 1) throw new Error("Failure " + stage + " not retried")
          stage++; waited = 0
        } else if (stage === 4) {
          if (retries.recipientsState !== "loading") return
          // The fourth failure continues at the steady cadence; then the list arrives.
          if (!accept(retries, {state: "snapshot", roomId: roomA, entries: [{key: person, name: "Late Name"}], agents: [], partial: true, category: null},
              {profiles: [{key: outsider, name: "Outsider"}]})) throw new Error("Snapshot rejected")
          if (retries.recipientsRetryAttempt !== 0 || retries.messageAuthorName(person) !== "Late Name") throw new Error("Success did not end the retries")
          if (retries.messageAuthorName(outsider) !== "Outsider") throw new Error("Helper profile name not used")
          stage = 5; waited = 0
        } else if (stage === 5) {
          // Nothing asks again once the list is shown.
          if (waited < 300) return
          if (retries.recipientsState !== "snapshot") throw new Error("Retry after success")
          // A repeated profile list does not undo a newer name from the member list.
          retries.rememberNames([{key: outsider, name: "Renamed"}])
          if (!accept(retries, {state: "snapshot", roomId: roomA, entries: [{key: person, name: "Late Name"}], agents: [], partial: true, category: null},
              {profiles: [{key: outsider, name: "Outsider"}]})) throw new Error("Repeat rejected")
          if (retries.messageAuthorName(outsider) !== "Renamed") throw new Error("Repeated profiles undid a newer name")
          // A malformed profile list fails the session like any malformed status.
          if (accept(retries, {state: "snapshot", roomId: roomA, entries: [], agents: [], partial: true, category: null}, {profiles: [{key: "nope", name: "Bad"}]}))
            throw new Error("Malformed profiles accepted")
          // An access denial revokes: it is shown once and never read again.
          start(retries)
          if (!accept(retries, failed("recipients_access_denied"))) throw new Error("Denied status rejected")
          if (retries.recipientsRetryAttempt !== 0 || retries.recipientsState !== "unavailable") throw new Error("Access denial scheduled a retry")
          stage = 6; waited = 0
        } else if (stage === 6) {
          if (waited < 300) return
          if (retries.recipientsState !== "unavailable" || retries.recipientsRetryAttempt !== 0) throw new Error("Denied room was read again")
          stage = 7; waited = 0
        } else if (stage === 7) {
          // The names were written (once, debounced) for this community only.
          storedNames.reload()
          var text = storedNames.text()
          if (text === "") return
          var parsed = JSON.parse(text)
          if (parsed.version !== 1 || parsed.scope !== "wss://fixture.example/|" + "a".repeat(64) || parsed.names.length < 2) throw new Error("Unexpected stored names: " + text)
          if (!parsed.names.some(function(n) { return n.key === person && n.name === "Late Name" })) throw new Error("Member name not stored")
          running = false
          console.log("PASS: transient member-list failures retry with backoff until success, access denial does not retry, helper profile names are used, and names are stored")
          Qt.quit()
        }
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
}
