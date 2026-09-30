// Synthetic stdio helper only: relay entry, a refusal, identity creation, the
// relay refusing a non-member, an invite redeemed from the panel (terms,
// acceptance, claim), then an open room joined and left. No relay, key or
// secret store is involved.
import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  readonly property string key: "5".repeat(64)
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  FloatingWindow {
    visible: true
    implicitWidth: 900
    implicitHeight: 600
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(name) { return findNamed(view, name, []).filter(function(entry) { return entry.visible }) }
  function click(name) {
    var found = shown(name)
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    found[0].clicked()
  }
  function enterInvite(text) {
    var field = shown("buzzInviteInput")
    if (field.length !== 1) throw new Error("Invite field not shown")
    field[0].text = text
    click("buzzInviteRedeem")
  }
  function enterRelay(url) {
    var field = shown("buzzSetupRelayUrl")
    if (field.length !== 1) throw new Error("Relay field not shown")
    field[0].text = url
    click("buzzSetupRelay")
  }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 140) throw new Error("Onboarding timed out at stage " + test.stage + " " + service.connection + " " + service.setupState)
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.connection === "unconfigured" && service.setupAssistAvailable) {
          if (shown("buzzCreateIdentity").length || shown("buzzExistingIdentityNote").length)
            throw new Error("Identity choices offered before a relay was set")
          // Refused locally: no request reaches the helper.
          if (service.setupRelay("http://fixture.example") || service.setupCategory !== "setup_invalid_relay"
              || shown("buzzSetupStatus").length !== 1) throw new Error("Invalid relay shape not refused in the panel")
          enterRelay("wss://refused.example")
          test.stage = 1
        } else if (test.stage === 1 && service.setupState === "failed") {
          if (service.setupCategory !== "setup_invalid_relay" || service.relay !== "" || service.sessionFailed
              || service.setupCategoryLabel.indexOf("not accepted") === -1)
            throw new Error("Helper refusal not shown as a sentence")
          enterRelay("wss://fixture.example")
          test.stage = 2
        } else if (test.stage === 2 && service.setupState === "idle" && service.relay === "wss://fixture.example/") {
          if (shown("buzzCreateIdentity").length !== 1 || shown("buzzExistingIdentityNote").length !== 1
              || shown("buzzExistingIdentityNote")[0].text.indexOf("omarchy-buzz setup identity enroll") === -1
              || shown("buzzNewIdentityNote")[0].text.indexOf("invitation") === -1)
            throw new Error("Identity choices missing after relay was saved")
          click("buzzCreateIdentity")
          if (service.createIdentity()) throw new Error("Second creation sent while the first was pending")
          test.stage = 3
        } else if (test.stage === 3 && service.setupState === "failed") {
          if (service.setupCategory !== "relay_unavailable" || service.identity !== ""
              || service.setupCategoryLabel.indexOf("reach the relay") === -1)
            throw new Error("Relay failure not reported")
          click("buzzCreateIdentity")
          test.stage = 4
        } else if (test.stage === 4 && service.connection === "disconnected" && service.joinAvailable) {
          // Refused as a non-member: the setup view offers an invite, not open rooms.
          if (service.identity !== test.key || service.createdIdentity !== test.key)
            throw new Error("Created identity not reported")
          if (shown("buzzJoinSetup").length !== 1 || shown("buzzOpenRooms").length || shown("buzzJoinPolicy").length)
            throw new Error("Invite entry not offered in the setup view")
          enterInvite("https://other.example/invite/x")
          test.stage = 5
        } else if (test.stage === 5 && service.inviteState === "failed") {
          if (service.inviteCategory !== "invite_relay_mismatch" || service.inviteLabel.indexOf("different relay") === -1
              || shown("buzzInviteStatus").length !== 1)
            throw new Error("Relay mismatch not shown as a sentence")
          enterInvite(" https://fixture.example/invite/v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8 ")
          test.stage = 6
        } else if (test.stage === 6 && service.joinSetup.state === "policy" && service.inviteState === "idle") {
          if (shown("buzzJoinPolicy").length !== 1 || shown("buzzJoinPolicyText")[0].text !== "Be kind to one another."
              || findNamed(view, "buzzJoinPolicy", [])[0].children.every(function(c) { return !c.text || c.text.indexOf("minimum age") === -1 }))
            throw new Error("Terms or age statement not shown before joining")
          click("buzzInviteAccept")
          test.stage = 7
        } else if (test.stage === 7 && service.connection === "authenticated" && service.openRoomsState === "snapshot") {
          if (service.joinSetup.state !== "joined" || service.inviteLabel.indexOf("Joined fixture.example") === -1)
            throw new Error("Claim result not reported")
          if (shown("buzzCreatedIdentity").length !== 1 || shown("buzzSetupRelay").length || shown("buzzCreateIdentity").length)
            throw new Error("Authenticated view shows setup controls or hides the new identity notice")
          var label = shown("buzzPublicKey")
          if (label.length !== 1 || label[0].text !== "Public key " + test.key.slice(0, 12) + "…")
            throw new Error("Public key not shown in short form")
          click("buzzCopyPublicKey")
          if (!service.publicKeyCopied || shown("buzzCopyPublicKey")[0].text !== "Copied") throw new Error("Copy not confirmed")
          if (service.setupRelay("wss://fixture.example") || service.createIdentity())
            throw new Error("Setup sent while authenticated")
          // No rooms yet: the sidebar lists open rooms, saying they need no approval.
          if (shown("buzzJoinFooter").length !== 1 || shown("buzzOpenRooms").length !== 1 || shown("buzzOpenRoom").length !== 1
              || findNamed(view, "buzzOpenRooms", [])[0].children.every(function(c) { return !c.text || c.text.indexOf("no approval") === -1 }))
            throw new Error("Open rooms not offered with their no-approval note")
          if (service.joinRoom("bbbbbbbb-0000-4000-8000-00000000000b")) throw new Error("Joined a room that is not listed as open")
          click("buzzOpenRoomJoin")
          test.stage = 8
        } else if (test.stage === 8 && service.selectedRoomId === "aaaaaaaa-0000-4000-8000-00000000000a") {
          if (service.openRooms.length !== 0 || shown("buzzJoinFooter").length) throw new Error("Joined room still offered")
          click("buzzLeaveRoom")
          if (!view.leaveArmed || shown("buzzLeaveRoom")[0].text !== "Confirm leave") throw new Error("Leave did not ask for confirmation")
          record.reload()
          if (JSON.parse(record.text()).requests.some(function(r) { return r.type === "leave_room" }))
            throw new Error("Leave sent on the first click")
          click("buzzLeaveRoom")
          test.stage = 9
        } else if (test.stage === 9 && service.roomAction.state === "rejected") {
          if (service.roomActionLabel.indexOf("only owner") === -1 || shown("buzzLeaveStatus").length !== 1)
            throw new Error("Leave refusal not explained")
          click("buzzLeaveRoom")
          click("buzzLeaveRoom")
          test.stage = 10
        } else if (test.stage === 10 && service.roomAction.state === "acknowledged" && service.rooms.length === 0) {
          if (service.openRooms.length !== 1 || shown("buzzJoinFooter").length !== 1) throw new Error("Left room not offered again")
          record.reload()
          test.stage = 11
        } else if (test.stage === 11) {
          var types = JSON.parse(record.text()).requests.map(function(r) { return r.type })
          var expected = "subscribe,set_relay,set_relay,create_identity,create_identity,claim_invite,claim_invite,accept_invite,open_rooms,join_room,leave_room,leave_room"
          if (types.join(",") !== expected && test.ticks < 130) { record.reload(); return }
          if (types.join(",") !== expected) throw new Error("Unexpected requests: " + types.join(","))
          console.log("PASS: onboarding relay entry, helper refusal, relay failure, identity creation, public key and copy, invite mismatch, terms accepted, claim, open room joined, leave confirmed, refused and left")
          Qt.quit()
        }
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
