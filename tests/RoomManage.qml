// Load more, New room and Room settings against a synthetic stdio helper: no
// relay, key or network (tests/room_manage_fixture.py scripts the relay's answers).
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  readonly property string general: "aaaaaaaa-0000-4000-8000-000000000001"
  readonly property string ops: "aaaaaaaa-0000-4000-8000-000000000002"
  readonly property string created: "bbbbbbbb-0000-4000-8000-000000000009"
  readonly property string pat: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  FloatingWindow {
    visible: true
    implicitWidth: 1100
    implicitHeight: 800
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  TestCase { id: input; when: false; name: "RoomManageInput" }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(name) {
    return findNamed(view, name, []).filter(function(item) {
      for (var p = item; p; p = p.parent) if (!p.visible) return false
      return true
    })
  }
  function one(name) {
    var found = shown(name)
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  function reveal(item) {
    for (var flick = item.parent; flick; flick = flick.parent) {
      if (typeof flick.contentY !== "number" || typeof flick.contentHeight !== "number" || !flick.contentItem) continue
      var y = item.mapToItem(flick.contentItem, 0, 0).y
      if (y < flick.contentY || y + item.height > flick.contentY + flick.height)
        flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height, y + item.height - flick.height))
      break
    }
  }
  function clickItem(item) {
    reveal(item)
    input.mouseClick(item, item.width / 2, item.height / 2)
  }
  function click(name) { clickItem(one(name)) }
  function type(name, text) {
    var field = one(name)
    reveal(field)
    field.forceActiveFocus()
    field.text = text
  }
  function requests(kind) {
    return JSON.parse(record.text() || '{"requests":[]}').requests.filter(function(r) { return r.type === kind })
  }
  function memberKeys() { return shown("buzzRoomMember").map(function(m) { return m.memberKey }) }
  function advance(next) { test.stage = next; test.ticks = 0 }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        record.reload()
        if (test.ticks > 80) throw new Error("RoomManage timed out at stage " + test.stage + " " + service.connection + " " + service.catalogMore
          + " " + service.roomActionLabel + " " + service.selectedRoomId)
        if (test.stage === -1) {
          // Every capability the helper announces is one the panel accepts.
          var helper = JSON.parse(Quickshell.env("BUZZ_HELPER_CAPABILITIES"))
          check(helper.indexOf("room_manage") !== -1 && helper.length === service.knownCapabilities.length, "Helper capability list not as expected: " + helper.length)
          check(service.validCapabilities(helper), "The panel refuses the helper's full capability list")
          check(!service.validCapabilities(helper.concat(["unknown_capability"])) && !service.validCapabilities(helper.concat([helper[1]])),
            "Unknown or repeated capabilities accepted")
          service.retry()
          advance(0)
        } else if (test.stage === 0 && service.connection === "authenticated" && service.roomManageSupported && service.catalogMore === "available") {
          check(service.catalogRooms.length === 3 && service.selectedRoomId === test.general, "First page not listed")
          check(service.catalogLabel === "Partial list · 3 shown · more available", "Label wrong: " + service.catalogLabel)
          one("buzzNewRoom")
          check(shown("buzzLoadMoreStatus").length === 0, "Status shown before loading more")
          click("buzzLoadMoreRooms")
          advance(1)
        } else if (test.stage === 1 && service.catalogMore === "failed") {
          // A failed page keeps every room and offers a retry.
          check(service.catalogRooms.length === 3, "A failed Load more dropped rooms")
          check(one("buzzLoadMoreStatus").text === "Loading more rooms timed out. Try again.", "Failure text wrong: " + one("buzzLoadMoreStatus").text)
          check(one("buzzLoadMoreRooms").text === "Try again · load more rooms", "Retry label wrong")
          click("buzzLoadMoreRooms")
          advance(2)
        } else if (test.stage === 2 && service.catalogMore === "none") {
          check(service.catalogRooms.length === 4 && service.catalogRooms.some(function(r) { return r.name === "later" }), "Next page not merged")
          check(shown("buzzLoadMoreRooms").length === 0 && shown("buzzLoadMoreStatus").length === 0, "Load more still shown at the end")
          click("buzzNewRoom")
          advance(3)
        } else if (test.stage === 3) {
          one("buzzNewRoomView")
          check(one("buzzHeaderPlace").text === "New room" && !one("buzzNewRoomCreate").enabled, "New room view wrong or Create enabled without a name")
          check(one("buzzNewRoomOpen").selected && !one("buzzNewRoomPrivate").selected, "Open is not the default")
          type("buzzNewRoomName", "  Plans ")
          type("buzzNewRoomAbout", "Quarterly")
          click("buzzNewRoomPrivate")
          check(one("buzzNewRoomPrivate").selected && one("buzzNewRoomCreate").enabled, "Private choice or Create not taken")
          click("buzzNewRoomCreate")
          advance(4)
        } else if (test.stage === 4 && service.roomAction.state === "rejected") {
          // The relay's refusal is shown in its own words, as text.
          check(one("buzzNewRoomStatus").text === "The relay refused to create the room. Relay: blocked: you may not create rooms here",
            "Refusal not shown honestly: " + one("buzzNewRoomStatus").text)
          one("buzzNewRoomView")
          check(shown("buzzNewRoomStatus")[0].textFormat === Text.PlainText, "Refusal text is not plain")
          click("buzzNewRoomCreate")
          advance(5)
        } else if (test.stage === 5 && service.selectedRoomId === test.created) {
          // Created, listed by a later read, selected; the form closed.
          check(shown("buzzNewRoomView").length === 0 && !view.subViewOpen, "New room view did not close")
          check(service.selectedRoom.name === "Plans", "New room not listed as created")
          click("buzzRoomSettings")
          advance(6)
        } else if (test.stage === 6 && service.roomDetailShown) {
          one("buzzRoomSettingsView")
          check(one("buzzHeaderPlace").text === "Room settings", "Settings header wrong")
          check(one("buzzRoomTopic").text === "No topic set." && /^Private room/.test(one("buzzRoomVisibility").text), "Topic or visibility wrong")
          check(memberKeys().length === 1 && shown("buzzRoomRemoveMember").length === 0, "The owner's own row offers Remove")
          one("buzzRoomEdit")
          check(one("buzzRoomEditName").text === "Plans" && one("buzzRoomEditAbout").text === "Quarterly", "Edit fields not filled")
          type("buzzRoomEditName", "Plans 2")
          click("buzzRoomSaveDetails")
          advance(7)
        } else if (test.stage === 7 && service.roomAction.state === "rejected") {
          check(one("buzzRoomManageStatus").text === "The relay refused to change the room details. Relay: restricted: actor not authorized for name/about changes",
            "Permission refusal not shown: " + one("buzzRoomManageStatus").text)
          click("buzzRoomSaveDetails")
          advance(8)
        } else if (test.stage === 8 && service.roomAction.state === "acknowledged" && service.selectedRoom.name === "Plans 2") {
          check(one("buzzRoomManageStatus").text === "Room details saved.", "Saved text wrong: " + one("buzzRoomManageStatus").text)
          type("buzzRoomEditTopic", "Ship it")
          click("buzzRoomSaveTopic")
          advance(9)
        } else if (test.stage === 9 && service.roomDetail.topic === "Ship it") {
          check(one("buzzRoomTopic").text === "Ship it", "Topic not shown after saving")
          // Adding by key (case and spaces forgiven) and from a verified DM partner.
          check(!one("buzzRoomAddMember").enabled, "Add enabled without a key")
          type("buzzRoomAddKey", "  " + "B".repeat(64) + " ")
          check(one("buzzRoomAddMember").enabled, "Add disabled for a valid key")
          type("buzzRoomAddKey", "abc")
          check(!one("buzzRoomAddMember").enabled, "Add enabled for a short key")
          type("buzzRoomAddKey", "d".repeat(64))
          click("buzzRoomAddMember")
          advance(10)
        } else if (test.stage === 10 && memberKeys().length === 2) {
          check(memberKeys().indexOf("d".repeat(64)) !== -1, "Added key not in the roster")
          check(shown("buzzRoomAddCandidate").length === 1 && one("buzzRoomAddCandidate").children.length > 0, "DM partner not offered")
          click("buzzRoomAddCandidateButton")
          advance(11)
        } else if (test.stage === 11 && memberKeys().length === 3) {
          check(memberKeys().indexOf(test.pat) !== -1 && shown("buzzRoomAddCandidate").length === 0, "Candidate not added or still offered")
          var removes = shown("buzzRoomRemoveMember")
          check(removes.length === 2, "Expected Remove for the two other members, found " + removes.length)
          clickItem(removes[0])
          check(removes[0].text === "Confirm remove" && requests("remove_room_member").length === 0, "Removal was not armed first")
          clickItem(removes[0])
          advance(12)
        } else if (test.stage === 12 && memberKeys().length === 2) {
          check(one("buzzRoomManageStatus").text === "Member removed.", "Removal text wrong")
          // A room where this identity is a plain member: topic and members, no edits.
          service.selectRoom(test.ops)
          advance(13)
        } else if (test.stage === 13 && service.roomDetailShown && service.roomDetail.roomId === test.ops) {
          check(service.roomRole === "member" && shown("buzzRoomEdit").length === 0 && shown("buzzRoomAdd").length === 0
            && shown("buzzRoomRemoveMember").length === 0, "A plain member is offered edits")
          check(/^Only this room's owners and admins/.test(one("buzzRoomEditNote").text), "Permission note missing")
          check(memberKeys().length === 2 && one("buzzRoomTopic").text === "No topic set.", "Members or topic not shown to a member")
          check(!service.updateRoomDetails("x", "") && !service.setRoomTopic("x") && !service.addRoomMember("e".repeat(64))
            && !service.removeRoomMember("a".repeat(64)), "The panel sent a change a member cannot make")
          advance(14)
        } else if (test.stage === 14) {
          // The requests that were sent (the record file trails by a tick or two).
          var topics = requests("set_room_topic")
          var adds = requests("add_room_member")
          var removed = requests("remove_room_member")
          var creates = requests("create_room")
          var more = requests("load_more_rooms")
          if (!(topics.length === 1 && adds.length === 2 && removed.length === 1 && creates.length === 2 && more.length === 2
              && requests("update_room").length === 2 && requests("refresh_rooms").length >= 1)) return
          check(more.every(function(r) { return Object.keys(r).sort().join(",") === "id,type,version" }), "Load more requests wrong")
          check(creates.every(function(r) { return r.name === "Plans" && r.about === "Quarterly" && r.visibility === "private" }),
            "Create requests wrong: " + JSON.stringify(creates))
          check(topics[0].topic === "Ship it" && topics[0].roomId === test.created, "Topic request wrong")
          check(adds[0].key === "d".repeat(64) && adds[1].key === test.pat && adds.every(function(r) { return r.roomId === test.created }),
            "Add requests wrong: " + JSON.stringify(adds))
          check(removed[0].roomId === test.created && removed[0].key !== service.identity, "Removal request wrong")
          console.log("PASS: Load more keeps the loaded rooms, retries after a timeout and merges the next page; New room sends a trimmed name, description and visibility, shows the relay's refusal as said, then selects the room once it is listed; Room settings shows topic and members to everyone and lets owners and admins edit details and the topic, add members by key or from DMs and remove others after confirmation, with every relay refusal shown in its own words; the panel accepts the helper's full capability list")
          Qt.quit()
        }
      } catch (error) { console.error(error.message || error); Qt.exit(1) }
    }
  }
}
