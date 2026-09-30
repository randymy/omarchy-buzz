// Synthetic stdio helper only: relay entry, a refusal, identity creation and
// the authenticated result. No relay, key or secret store is involved.
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
        if (test.ticks > 120) throw new Error("Onboarding timed out at stage " + test.stage + " " + service.connection + " " + service.setupState)
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
        } else if (test.stage === 4 && service.connection === "authenticated") {
          if (service.identity !== test.key || service.createdIdentity !== test.key || service.setupState !== "idle")
            throw new Error("Created identity not reported")
          if (shown("buzzCreatedIdentity").length !== 1 || shown("buzzSetupRelay").length || shown("buzzCreateIdentity").length)
            throw new Error("Authenticated view shows setup controls or hides the new identity notice")
          var label = shown("buzzPublicKey")
          if (label.length !== 1 || label[0].text !== "Public key " + test.key.slice(0, 12) + "…")
            throw new Error("Public key not shown in short form")
          click("buzzCopyPublicKey")
          if (!service.publicKeyCopied || shown("buzzCopyPublicKey")[0].text !== "Copied") throw new Error("Copy not confirmed")
          if (service.setupRelay("wss://fixture.example") || service.createIdentity())
            throw new Error("Setup sent while authenticated")
          record.reload()
          test.stage = 5
        } else if (test.stage === 5) {
          var types = JSON.parse(record.text()).requests.map(function(r) { return r.type })
          if (types.length < 5 && test.ticks < 110) { record.reload(); return }
          if (types.join(",") !== "subscribe,set_relay,set_relay,create_identity,create_identity")
            throw new Error("Unexpected requests: " + types.join(","))
          console.log("PASS: onboarding relay entry, helper refusal, relay failure, identity creation, public key and copy, no setup while authenticated")
          Qt.quit()
        }
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
