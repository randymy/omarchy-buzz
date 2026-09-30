// Settings → Invite people against a synthetic stdio helper: no relay, key or
// network. The first Create invite is refused (not an owner or admin), the
// second shows both links and the message for newcomers; each Copy is checked.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  readonly property string code: "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 700
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  TestCase { id: input; when: false; name: "InvitesInput" }
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
  function click(name) {
    var item = one(name)
    for (var flick = item.parent; flick; flick = flick.parent) {
      if (typeof flick.contentY !== "number" || typeof flick.contentHeight !== "number" || !flick.contentItem) continue
      var y = item.mapToItem(flick.contentItem, 0, 0).y
      if (y < flick.contentY || y + item.height > flick.contentY + flick.height)
        flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height, y + item.height - flick.height))
      break
    }
    input.mouseClick(item, item.width / 2, item.height / 2)
  }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 120) throw new Error("Invites timed out at stage " + test.stage + " " + service.connection + " " + service.mintState)
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.connection === "authenticated" && service.inviteMintSupported) {
          view.openSettings()
          test.stage = 1
        } else if (test.stage === 1) {
          one("buzzInviteSection")
          check(shown("buzzInviteResult").length === 0 && shown("buzzInviteOffline").length === 0, "Result or offline note shown before a mint")
          check(one("buzzInviteUses1").selected && one("buzzInviteHours168").selected, "Defaults are not one person and 7 days")
          click("buzzInviteUses5")
          click("buzzInviteHours720")
          check(view.inviteUses === 5 && view.inviteHours === 720, "Choices not taken")
          check(!service.mintInvite(7, 24) && !service.mintInvite(5, 48), "An unoffered limit was sent")
          click("buzzMintInvite")
          check(!service.canMintInvite && !service.mintInvite(5, 720), "A second mint was sent while the first was pending")
          test.stage = 2
        } else if (test.stage === 2 && service.mintState === "failed") {
          check(service.mintCategory === "invite_forbidden" && service.invites.category === "invite_forbidden", "Refusal category wrong")
          check(one("buzzInviteMintStatus").text === "Only the relay's owner or admins can create invites.",
            "Forbidden message wrong: " + one("buzzInviteMintStatus").text)
          check(shown("buzzInviteResult").length === 0, "A refused mint shows a result")
          click("buzzMintInvite")
          test.stage = 3
        } else if (test.stage === 3 && service.invites.state === "minted" && service.mintState === "idle") {
          check(shown("buzzInviteMintStatus").length === 0, "Status still shown after the mint")
          var app = "buzz://join?relay=wss%3A%2F%2Ffixture.example&code=" + test.code
          var web = "https://fixture.example/invite/" + test.code
          check(one("buzzInviteAppLink").text === app, "App link wrong: " + one("buzzInviteAppLink").text)
          check(one("buzzInviteWebLink").text === web, "Web link wrong: " + one("buzzInviteWebLink").text)
          check(one("buzzInviteDetails").text.indexOf("5 uses · expires ") === 0 && /joins as member$/.test(one("buzzInviteDetails").text),
            "Details wrong: " + one("buzzInviteDetails").text)
          var blurb = one("buzzInviteBlurb").text
          check(blurb === service.inviteBlurb && blurb.length < 600 && blurb.indexOf(test.code) !== -1
            && blurb.indexOf("fixture.example") !== -1 && blurb.indexOf("wss://fixture.example,") !== -1
            && blurb.indexOf("https://github.com/randymy/omarchy-buzz") !== -1 && blurb.indexOf("Buzz Desktop") !== -1
            && blurb.indexOf("coding agent") !== -1 && /\n\nhttps:\/\/fixture\.example\/invite\/v2\.[A-Za-z0-9_-]{43}$/.test(blurb),
            "Blurb wrong (" + blurb.length + "): " + blurb)
          Quickshell.clipboardText = ""
          click("buzzCopyInviteApp")
          check(Quickshell.clipboardText === app && one("buzzCopyInviteApp").text === "Copied", "App link not copied")
          click("buzzCopyInviteWeb")
          check(Quickshell.clipboardText === web && one("buzzCopyInviteWeb").text === "Copied" && one("buzzCopyInviteApp").text === "Copy",
            "Web link not copied")
          click("buzzCopyInviteBlurb")
          check(Quickshell.clipboardText === blurb && one("buzzCopyInviteBlurb").text === "Copied", "Message not copied")
          record.reload()
          test.stage = 4
        } else if (test.stage === 4) {
          var requests = JSON.parse(record.text() || '{"requests":[]}').requests.filter(function(r) { return r.type === "mint_invite" })
          if (requests.length !== 2 && test.ticks < 110) { record.reload(); return }
          check(requests.length === 2 && requests.every(function(r) {
            return Object.keys(r).sort().join(",") === "expiresInHours,id,maxUses,type,version" && r.maxUses === 5 && r.expiresInHours === 720
          }), "Unexpected mint requests: " + JSON.stringify(requests))
          console.log("PASS: Invite people offers 1/5/25 uses and 1/7/30 days, explains a refused mint, shows the buzz:// and https invite links and a message for newcomers with the code and relay host, and copies each")
          Qt.quit()
        }
      } catch (error) { console.error(error.message || error); Qt.exit(1) }
    }
  }
}
