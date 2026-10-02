// Communities against a synthetic stdio helper only (no relay, key or secret
// store): first setup through "Join an existing community" with an invite link
// (identity created, invite redeemed), the account menu's Communities block,
// the Join and Create views (Desktop's copy, button gating, failure messages,
// buzz.xyz step, terms), switching with per-community room memory, Settings
// rename and leave with the last-community guard, and sample data.
import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  property var opened: []
  readonly property string first: "wss://first.example/"
  readonly property string second: "wss://second.example/"
  readonly property string third: "wss://third.example/"
  readonly property string code: "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
  readonly property string placeholder: "https://community.example.com or paste an invite link"
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  Buzz.Service { id: sampleService; sampleMode: true }
  FloatingWindow {
    visible: true
    implicitWidth: 900
    implicitHeight: 640
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service; linkOpener: function(url) { test.opened = test.opened.concat([url]) } }
  }
  FloatingWindow {
    visible: true
    implicitWidth: 900
    implicitHeight: 640
    Buzz.PanelContent { id: sampleView; anchors.fill: parent; service: sampleService }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(name, within) { return findNamed(within || view, name, []).filter(function(entry) { return entry.visible }) }
  function one(name, within) {
    var found = shown(name, within)
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  function click(name, within) { one(name, within).clicked() }
  function type(fieldName, buttonName, text) {
    var field = one(fieldName)
    field.text = text
    var button = one(buttonName)
    check(button.enabled, buttonName + " disabled with text")
    button.clicked()
  }
  function requests() { record.reload(); return JSON.parse(record.text()).requests }
  FileView { id: agentsRecord; path: Quickshell.env("BUZZ_SEND_RECORD").replace(/send-record\.json$/, "agents-record.json"); blockLoading: true; printErrors: false }
  function agentRequests() { agentsRecord.reload(); try { return JSON.parse(agentsRecord.text()).requests } catch (_) { return [] } }
  function texts(name) { return shown(name).map(function(item) { return item.text }) }
  // Agents are enrolled in one or more communities: those with an instance in
  // the current one (first or not) are listed and editable, the others muted
  // and read-only with only their own rooms, and a direct message with an
  // agent exists only in the community where it was opened.
  function expectAgents(current, other, dms) {
    check(JSON.stringify(texts("buzzAgentRow")) === JSON.stringify(current.map(function(name) { return name + " · stopped" })),
      "Current agents wrong: " + texts("buzzAgentRow"))
    check(JSON.stringify(texts("buzzAgentOtherRow")) === JSON.stringify(other), "Other agents wrong: " + texts("buzzAgentOtherRow"))
    check(shown("buzzAgentsOtherHeading").length === (other.length ? 1 : 0), "Other heading wrong")
    check(JSON.stringify(texts("buzzDmRow")) === JSON.stringify(dms), "Direct messages wrong: " + texts("buzzDmRow"))
  }
  function openAgent(name) { shown("buzzAgentRow").concat(shown("buzzAgentOtherRow")).filter(function(row) { return row.text.indexOf(name + " · ") === 0 })[0].clicked() }
  function instanceLabels() { return texts("buzzAgentInstanceLabel") }
  function agentEntry(name) { return service.agents.agents.find(function(entry) { return entry.name === name }) }
  function editorRooms() { return shown("buzzAgentRoom").map(function(box) { return (box.checked ? "x " : "  ") + box.text }) }
  function rowFor(relay) {
    return shown("buzzCommunityRow").filter(function(row) { return row.relay === relay })[0]
  }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 280) throw new Error("Communities timed out at stage " + test.stage + " " + service.connection + " " + service.communityLocal)
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.connection === "unconfigured" && service.setupAssistAvailable && service.communitiesSupported) {
          // First setup: Join an existing community is the default, with Desktop's copy.
          check(service.setupProvider === "join" && one("buzzSetupJoinOption").selected && !one("buzzSetupCreateOption").selected, "Join is not the default")
          check(one("buzzSetupJoinDescription").text === "Use the community URL or invite link you received.", "Join description wrong")
          var field = one("buzzSetupRelayUrl")
          check(field.placeholderText === test.placeholder, "Placeholder wrong: " + field.placeholderText)
          check(one("buzzSetupRelay").text === "Join community" && !one("buzzSetupRelay").enabled, "Join enabled while empty")
          check(shown("buzzCreateIdentity").length === 0, "Identity offered before a community")
          // Create a new community: the buzz.xyz step, then the same join field.
          click("buzzSetupCreateOption")
          check(service.setupProvider === "create" && shown("buzzSetupRelayUrl").length === 0, "Create did not replace the join form")
          check(one("buzzSetupCreateDescription").text.indexOf("buzz.xyz") !== -1
            && one("buzzSetupCreateDescription").text.indexOf("cannot create one itself") !== -1, "Create explanation wrong")
          click("buzzSetupCreateOpen")
          check(test.opened.length === 1 && test.opened[0] === "https://buzz.xyz", "buzz.xyz not opened: " + test.opened)
          check(one("buzzSetupCreateInput").placeholderText === test.placeholder && !one("buzzSetupCreateJoin").enabled, "Create's join field wrong")
          click("buzzSetupJoinOption")
          type("buzzSetupRelayUrl", "buzzSetupRelay", "not a link")
          test.stage = 1
        } else if (test.stage === 1 && service.communityLocal === "failed") {
          check(service.communityCategory === "join_invalid" && one("buzzSetupJoinStatus").text === "Please enter a valid invite link or community URL",
            "Invalid input not explained: " + service.communityLabel)
          check(!service.sessionFailed, "A refusal ended the session")
          type("buzzSetupRelayUrl", "buzzSetupRelay", "  https://first.example/invite/" + test.code + "  ")
          check(service.communityBusy && one("buzzSetupJoinStatus").text === "Joining…", "Joining state not shown")
          test.stage = 2
        } else if (test.stage === 2 && service.relay === test.first && service.communityLocal === "idle") {
          // Saved without an identity: create one, then the invite is redeemed by itself.
          check(service.pendingInviteInput === "https://first.example/invite/" + test.code, "Invite not kept for after the identity")
          click("buzzCreateIdentity")
          test.stage = 3
        } else if (test.stage === 3 && service.connection === "authenticated" && service.catalogState === "ready") {
          check(service.joinSetup.state === "joined" && service.pendingInviteInput === "", "Invite not redeemed after the identity")
          check(service.selectedRoomId === "aaaaaaaa-0000-4000-8000-0000000000f1", "First room not selected")
          service.selectRoom("aaaaaaaa-0000-4000-8000-0000000000f2")
          service.agents.retry()
          // Account menu: the current community, no others yet, Join and Create.
          view.openAccountMenu()
          check(one("buzzAccountCommunityName").text === "✓ first" && one("buzzAccountCommunityHost").text === "first.example", "Current community wrong")
          check(shown("buzzCommunitySwitch").length === 0, "Switch offered with one community")
          var join = one("buzzAccountJoinCommunity"), create = one("buzzAccountCreateCommunity"), status = shown("buzzAccountStatus")
          check(join.text === "Join an existing community…" && create.text === "Create a new community…", "Menu entries wrong")
          // Order within the menu: Join, then Create, and the block above Update your status.
          var siblings = join.parent.children
          var at = function(item) { for (var i = 0; i < siblings.length; i++) if (siblings[i] === item) return i; return -1 }
          check(at(join) !== -1 && at(join) < at(create), "Create is above Join")
          var block = one("buzzAccountCommunities"), menuItems = block.parent.children, statusRow = findNamed(view, "buzzAccountStatus", [])[0]
          var blockAt = -1, statusAt = -1
          for (var m = 0; m < menuItems.length; m++) { if (menuItems[m] === block) blockAt = m; if (menuItems[m] === statusRow) statusAt = m }
          check(blockAt !== -1 && statusAt !== -1 && blockAt < statusAt, "Communities not above Update your status")
          join.clicked()
          check(!view.accountMenuOpen && view.subView === "join-community" && one("buzzHeaderPlace").text === "Join an existing community"
            && one("buzzHeaderBack").text === "← Back to rooms", "Join view header wrong")
          check(one("buzzCommunityJoinDescription").text === "Use the community URL or invite link you received.", "Join view description wrong")
          check(one("buzzCommunityInput").placeholderText === test.placeholder && !one("buzzCommunityJoin").enabled, "Join view gating wrong")
          check(shown("buzzHistoryScroll").length === 0, "Rooms still shown under the join view")
          type("buzzCommunityInput", "buzzCommunityJoin", "https://refused.example")
          test.stage = 4
        } else if (test.stage === 4 && service.communityLocal === "failed") {
          check(service.communityCategory === "join_rejected" && one("buzzCommunityJoinStatus").text.indexOf("Not a member yet.") === 0,
            "Refusal not in Desktop's words: " + one("buzzCommunityJoinStatus").text)
          check(view.subView === "join-community", "A refusal left the view")
          type("buzzCommunityInput", "buzzCommunityJoin", "https://second.example")
          test.stage = 5
        } else if (test.stage === 5 && view.subView === "" && service.relay === test.second && service.catalogState === "ready"
            && service.agents.available && service.agents.activeRelay === test.second) {
          // The agent service followed the switch (asked again for status).
          check(test.agentRequests().length >= 2, "No subscribe after the switch: " + test.agentRequests())
          // Both bot is enrolled here first and in the first community too.
          test.expectAgents(["Night bot", "Both bot"], ["vClaude · in first"], [])
          test.openAgent("vClaude")
          var remote = findNamed(view, "buzzAgentEditor", [])[0]
          check(remote.readOnly && one("buzzAgentCommunity").text === "Community: first"
            && one("buzzAgentOtherCommunity").text === "Enrolled in first. Switch to that community to manage it, or add it to second below.",
            "vClaude not read-only here: " + one("buzzAgentOtherCommunity").text)
          check(shown("buzzAgentSave").length === 0 && shown("buzzAgentStart").length === 0 && shown("buzzAgentDelete").length === 0, "Actions offered for another community's agent")
          check(JSON.stringify(test.editorRooms()) === JSON.stringify(["x Room aaaaaaaa…"]), "vClaude's rooms shown with this community's: " + test.editorRooms())
          check(JSON.stringify(test.instanceLabels()) === JSON.stringify(["first · stopped"]), "vClaude's communities wrong: " + test.instanceLabels())
          // Add to this community, with this community's verified rooms.
          check(JSON.stringify(texts("buzzAgentAddRoom")) === JSON.stringify(["# night-shift"]) && !one("buzzAgentAddToCommunity").enabled
            && one("buzzAgentAddToCommunity").text === "Add to second", "Add to second wrong: " + texts("buzzAgentAddRoom"))
          click("buzzAgentAddRoom")
          click("buzzAgentAddToCommunity")
          test.stage = 50
        } else if (test.stage === 50 && service.agents.requestState === "done" && test.agentEntry("vClaude").instances.length === 2) {
          var added = test.agentRequests().filter(function(r) { return r.type === "enroll_agent_in" })
          // The record file may still be reloading; it is read again shortly.
          if (!added.length && test.ticks < 270) return
          check(added.length === 1 && JSON.stringify(Object.keys(added[0]).sort()) === JSON.stringify(["agentId", "id", "instanceId", "relay", "rooms", "type", "version"])
            && added[0].relay === test.second && JSON.stringify(added[0].rooms) === JSON.stringify(["bbbbbbbb-0000-4000-8000-0000000000b1"]),
            "enroll_agent_in shape wrong: " + JSON.stringify(added))
          // This fixture has no room_recipients, so no member refetch is sent here
          // (the --agents mode covers it); nothing else may be sent either.
          check(test.requests().filter(function(r) { return r.type === "fetch_recipients" }).length === 0, "fetch_recipients sent without the capability")
          // Now listed here, editable with this community's rooms; both communities listed.
          test.expectAgents(["vClaude", "Night bot", "Both bot"], [], [])
          var vclaude = findNamed(view, "buzzAgentEditor", [])[0]
          check(!vclaude.readOnly && one("buzzAgentCommunity").text === "Community: second" && shown("buzzAgentAddSection").length === 0,
            "vClaude not managed here after Add")
          check(JSON.stringify(test.editorRooms()) === JSON.stringify(["x # night-shift"]), "vClaude's rooms here wrong: " + test.editorRooms())
          check(JSON.stringify(test.instanceLabels()) === JSON.stringify(["first · stopped", "second (this community) · stopped"]),
            "vClaude's communities wrong: " + test.instanceLabels())
          check(JSON.stringify(texts("buzzAgentLeave")) === JSON.stringify(["Leave first", "Leave second"]), "Leave buttons wrong: " + texts("buzzAgentLeave"))
          // The first community's instance is started from here, by its own relay.
          check(JSON.stringify(shown("buzzAgentInstanceStart").map(function(b) { return b.relay })) === JSON.stringify([test.first]), "Per-community Start wrong")
          test.openAgent("Both bot")
          check(one("buzzAgentCommunity").text === "Community: second" && JSON.stringify(test.instanceLabels())
            === JSON.stringify(["second (this community) · stopped", "first · stopped"]), "Both bot's communities wrong: " + test.instanceLabels())
          test.shown("buzzAgentRow")[1].clicked()
          check(!findNamed(view, "buzzAgentEditor", [])[0].readOnly && one("buzzAgentCommunity").text === "Community: second", "Night bot not editable here")
          check(JSON.stringify(test.editorRooms()) === JSON.stringify(["x # night-shift"]), "Night bot's rooms wrong: " + test.editorRooms())
          view.closeAgentEditor()
          // Joined and switched: back to the rooms of the new community.
          check(service.selectedRoomId === "bbbbbbbb-0000-4000-8000-0000000000b1", "Second community's room not selected")
          service.selectRoom("bbbbbbbb-0000-4000-8000-0000000000b1")
          check(service.communityEntries.length === 2 && service.activeCommunity.relay === test.second, "List not updated")
          view.openAccountMenu()
          var others = shown("buzzCommunitySwitch")
          check(others.length === 1 && others[0].text === "first · first.example", "Switch row wrong")
          others[0].clicked()
          check(!view.accountMenuOpen, "Menu stayed open after switching")
          test.stage = 6
        } else if (test.stage === 6 && service.relay === test.first && service.catalogState === "ready"
            && service.agents.activeRelay === test.first) {
          // The room chosen earlier in this community comes back.
          check(service.selectedRoomId === "aaaaaaaa-0000-4000-8000-0000000000f2", "Room memory not per community: " + service.selectedRoomId)
          // Back in vClaude's community: it is listed and editable with this community's rooms, and its DM is here.
          // Both bot is listed here too, though this is not its first community.
          test.expectAgents(["vClaude", "Both bot"], ["Night bot · in second"], ["vClaude"])
          test.openAgent("vClaude")
          check(!findNamed(view, "buzzAgentEditor", [])[0].readOnly && one("buzzAgentCommunity").text === "Community: first", "vClaude not editable in its community")
          check(JSON.stringify(test.editorRooms()) === JSON.stringify(["x # general", "  # random"]), "vClaude's rooms not this community's: " + test.editorRooms())
          check(shown("buzzAgentSave").length === 1 && shown("buzzAgentDelete").length === 1, "vClaude's actions missing in its community")
          test.openAgent("Both bot")
          var both = findNamed(view, "buzzAgentEditor", [])[0]
          check(!both.readOnly && one("buzzAgentCommunity").text === "Community: first"
            && JSON.stringify(test.editorRooms()) === JSON.stringify(["  # general", "x # random"]), "Both bot's rooms here wrong: " + test.editorRooms())
          // Its workspace here is its own default, named like its unit; not edited from here.
          check(shown("buzzAgentWorkspace").length === 0 && one("buzzAgentInstanceWorkspace").text.indexOf("omarchy-buzz-room-workspaces/77777777-7777-4777-8777-777777777777-") !== -1,
            "Second community's workspace offered for editing")
          // Leave the second community (its first): confirmed by a second click.
          var leaveSecond = shown("buzzAgentLeave").filter(function(b) { return b.relay === test.second })[0]
          leaveSecond.clicked()
          check(leaveSecond.text === "Confirm leave second" && !test.agentRequests().some(function(r) { return r.type === "leave_agent_community" }),
            "Leave sent on the first click")
          leaveSecond.clicked()
          test.stage = 60
        } else if (test.stage === 60 && service.agents.requestState === "done" && test.agentEntry("Both bot").instances.length === 1) {
          var left = test.agentRequests().filter(function(r) { return r.type === "leave_agent_community" })
          if (!left.length && test.ticks < 270) return
          check(left.length === 1 && JSON.stringify(Object.keys(left[0]).sort()) === JSON.stringify(["agentId", "id", "instanceId", "relay", "type", "version"])
            && left[0].relay === test.second, "leave_agent_community shape wrong: " + JSON.stringify(left))
          test.expectAgents(["vClaude", "Both bot"], ["Night bot · in second"], ["vClaude"])
          check(JSON.stringify(test.instanceLabels()) === JSON.stringify(["first (this community) · stopped"]) && shown("buzzAgentLeave").length === 0
            && shown("buzzAgentLastCommunity").length === 1, "Both bot's last community could be left: " + test.instanceLabels())
          check(!service.agents.leaveCommunity("77777777-7777-4777-8777-777777777777", test.first), "Service sent a leave for the last community")
          view.closeAgentEditor()
          view.openAccountMenu()
          click("buzzAccountCreateCommunity")
          check(view.subView === "create-community" && one("buzzHeaderPlace").text === "Create a new community", "Create view header wrong")
          click("buzzCommunityCreateOpen")
          check(test.opened.length === 2 && test.opened[1] === "https://buzz.xyz", "buzz.xyz not opened from the create view")
          check(findNamed(view, "buzzCommunityCreateInput", []).filter(function(f) { return f.visible }).length === 1, "Create view has no join field")
          type("buzzCommunityCreateInput", "buzzCommunityCreateJoin", "buzz://join?relay=wss://third.example&code=" + test.code)
          test.stage = 7
        } else if (test.stage === 7 && service.relay === test.third && service.policyShown && view.communityAwaiting) {
          // Terms first: shown in the view, accepted with Desktop's button.
          check(view.subView === "create-community" && one("buzzCommunityTermsText").text === "Third team terms.", "Terms not shown")
          check(one("buzzCommunityAccept").text === "Accept and join", "Accept button wording wrong")
          click("buzzCommunityAccept")
          test.stage = 8
        } else if (test.stage === 8 && view.subView === "" && service.connection === "authenticated" && service.relay === test.third) {
          // Settings: rename inline, then leave with a confirmation.
          view.openSettings()
          check(shown("buzzCommunityRow").length === 3, "Settings does not list the communities")
          var row = test.rowFor(test.second)
          click("buzzCommunityRename", row)
          check(one("buzzCommunityRenameField", row).text === "second", "Rename field not prefilled")
          one("buzzCommunityRenameField", row).text = "   "
          check(!one("buzzCommunityRenameSave", row).enabled, "Empty name could be saved")
          one("buzzCommunityRenameField", row).text = "Night shift"
          click("buzzCommunityRenameSave", row)
          test.stage = 9
        } else if (test.stage === 9 && service.communityLocal === "idle" && service.communityEntries[1].name === "Night shift") {
          check(view.renamingRelay === "", "Rename field still open")
          check(one("buzzCommunityRowHost", test.rowFor(test.second)).text === "second.example · calls itself Second Team HQ", "Hint not shown as a hint")
          var leaveRow = test.rowFor(test.third)
          click("buzzCommunityLeave", leaveRow)
          check(one("buzzCommunityLeave", leaveRow).text === "Confirm leave" && shown("buzzCommunityLeaveNote", leaveRow).length === 1, "Leave did not ask to confirm")
          check(!requests().some(function(r) { return r.type === "leave_community" }), "Leave sent on the first click")
          click("buzzCommunityLeave", leaveRow)
          test.stage = 10
        } else if (test.stage === 10 && service.relay === test.second && service.communityEntries.length === 2 && service.communityLocal === "idle") {
          // The active (last listed) community left: the one before it became active.
          var row2 = test.rowFor(test.second)
          click("buzzCommunityLeave", row2)
          click("buzzCommunityLeave", row2)
          test.stage = 11
        } else if (test.stage === 11 && service.communityEntries.length === 1 && service.communityLocal === "idle") {
          check(one("buzzCommunitySettingsStatus").text.indexOf("You were no longer a member") !== -1, "Already-absent notice missing")
          var last = test.rowFor(test.first)
          check(!one("buzzCommunityLeave", last).enabled && shown("buzzCommunityLastNote").length === 1, "Last community could be left")
          check(!service.leaveCommunity(test.first), "Service sent a leave for the last community")
          // Sample data: two fixture communities; switching is presentation only.
          sampleView.openAccountMenu()
          var sampleSwitch = shown("buzzCommunitySwitch", sampleView)
          check(one("buzzAccountCommunityName", sampleView).text === "✓ Sample" && sampleSwitch.length === 1
            && sampleSwitch[0].text === "Second team · second.example", "Sample communities wrong")
          sampleSwitch[0].clicked()
          check(sampleService.activeCommunity.relay === "wss://second.example/", "Sample switch did not apply")
          sampleView.openSettings()
          check(shown("buzzCommunityRow", sampleView).length === 2 && !one("buzzCommunityLeave", shown("buzzCommunityRow", sampleView)[0]).enabled,
            "Sample settings wrong")
          test.stage = 12
        } else if (test.stage === 12) {
          var sent = requests()
          var types = sent.map(function(r) { return r.type }).filter(function(t) { return ["subscribe", "get_snapshot", "open_rooms", "retry_connection"].indexOf(t) === -1 })
          var expected = "join_community,join_community,create_identity,claim_invite,accept_invite,join_community,join_community,switch_community,join_community,accept_invite,rename_community,leave_community,leave_community"
          // The record file may still be reloading; it is checked again shortly.
          if (types.join(",") !== expected && test.ticks < 270) return
          if (types.join(",") !== expected) throw new Error("Unexpected requests: " + types.join(","))
          var rename = sent.filter(function(r) { return r.type === "rename_community" })[0]
          check(JSON.stringify(Object.keys(rename).sort()) === JSON.stringify(["id", "name", "relay", "type", "version"]) && rename.relay === test.second,
            "Rename shape wrong")
          console.log("PASS: first setup joins from an invite link and redeems it after the identity; menu Communities block; Join and Create views with Desktop's copy, gating, failures and the buzz.xyz step; terms accepted; switching keeps each community's room; agents listed in every community they are enrolled in (an agent in two communities under either), others read-only with only their rooms and their communities, Add to this community with its verified rooms, Leave one community with confirmation and never the last, an agent's DM only in its community; rename; leave with confirmation, next community active, already-absent notice, last community kept; sample communities")
          Qt.quit()
        }
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
