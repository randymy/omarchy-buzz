// Account control, account menu and settings view against synthetic helper
// frames. No helper, relay, keys, browser or network: Send feedback is
// recorded, not opened. BUZZ_SETTINGS_CAPTURE_DIR optionally saves PNGs.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as Buzz
import "plugin/AnsiArt.js" as AnsiArt

ShellRoot {
  id: test
  property int stage: 0
  property int ticks: 0
  property int presentationRequests: 0
  property int closeRequests: 0
  property var opened: []
  readonly property string me: "b".repeat(64)
  readonly property string other: "a".repeat(64)
  readonly property string room: "11111111-1111-4111-8111-111111111111"
  readonly property string captureDir: Quickshell.env("BUZZ_SETTINGS_CAPTURE_DIR")
  Buzz.Service { id: service; autoConnect: false; manifest: ({id: "community.buzz", version: "0.0.15"}) }
  FloatingWindow {
    id: window
    visible: true
    implicitWidth: 1000
    implicitHeight: 620
    Buzz.PanelContent {
      id: view
      anchors.fill: parent
      service: service
      onPresentationRequested: test.presentationRequests++
      onCloseRequested: test.closeRequests++
    }
  }
  FileView { id: fixture; path: Quickshell.env("BUZZ_ANSI_FIXTURE"); blockLoading: true; printErrors: false }
  // Mouse and key events go through the real window, as a pointer and keyboard would.
  TestCase { id: input; when: false; name: "SettingsInput" }

  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(name) { return findNamed(view, name, []).filter(function(item) { return item.visible }) }
  function one(name) {
    var found = shown(name)
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  // Scroll a settings control into view first, as a reader would.
  function click(item) {
    for (var flick = item.parent; flick; flick = flick.parent) {
      if (typeof flick.contentY !== "number" || typeof flick.contentHeight !== "number" || !flick.contentItem) continue
      var y = item.mapToItem(flick.contentItem, 0, 0).y
      if (y < flick.contentY || y + item.height > flick.contentY + flick.height)
        flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height, y + item.height - flick.height))
      break
    }
    input.mouseClick(item, item.width / 2, item.height / 2)
  }
  function frame(kind, generation, connection, identity, relay, withRoster) {
    var capabilities = ["connection_status"]
    var status = {generation: generation, connection: connection, category: null, identity: identity, relay: relay}
    if (withRoster) {
      capabilities = capabilities.concat(["room_catalog", "room_history", "room_recipients"])
      status.catalog = {state: "partial", category: "room_catalog_partial", rooms: [{id: test.room, name: "Fixture", description: "",
        kind: "stream", participants: [], hidden: false}]}
      status.history = {state: "snapshot", roomId: test.room, rows: [], hasMore: false, category: "history_completeness_unknown"}
      status.recipients = {state: "snapshot", roomId: test.room, partial: false, category: null,
        entries: [{key: test.me, name: "Fixture Me"}, {key: test.other, name: "Someone Else"}]}
    }
    return JSON.stringify({version: 1, type: kind, instanceId: "settings-fixture", generation: generation,
      capabilities: capabilities, status: status})
  }
  function expectState(state, label, dot, name) {
    var account = one("buzzAccount")
    check(account.stateName === state && view.accountStateLabel === label && Qt.colorEqual(one("buzzAccountDot").color, dot)
      && one("buzzAccountName").text === name,
      "Account control wrong for " + state + ": " + account.stateName + " · " + view.accountStateLabel + " · "
        + one("buzzAccountDot").color + " · " + one("buzzAccountName").text)
  }
  function openMenu() {
    click(one("buzzAccount"))
    check(view.accountMenuOpen && one("buzzAccountMenu").visible, "Account menu did not open on click")
  }
  // Header: title, status and Close only; the moved controls live in Settings.
  function headerClean() {
    var moved = findNamed(view, "", []).filter(function(item) {
      return item.visible && typeof item.text === "string" && /^(Alerts: (on|off)|Window|Overlay)$/.test(item.text)
        && typeof item.clicked === "function" && ["buzzSettingsAlerts", "buzzSettingsWindow", "buzzSettingsOverlay"].indexOf(item.objectName) === -1
    })
    check(moved.length === 0, "Header still shows moved controls: " + moved.map(function(item) { return item.text }).join(", "))
    one("buzzClose")
  }

  Timer {
    interval: 100
    running: true
    repeat: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 150) throw new Error("Timed out at stage " + test.stage)
        if (test.stage === 0) {
          // No session yet: the control is still there so Settings stays reachable.
          expectState("offline", "Offline", Color.urgent, "Not connected")
          service.beginSession()
          expectState("connecting", "Connecting", "#d29922", "Not connected")
          check(service.acceptFrame(test.frame("hello", 1, "unconfigured", null, null, false)), "Unconfigured hello rejected")
          expectState("unset", "Not set up", Color.muted, "Not connected")
          check(service.acceptFrame(test.frame("status", 2, "disconnected", test.me, "wss://fixture.example/", false)), "Disconnected frame rejected")
          expectState("offline", "Offline", Color.urgent, "Not connected")
          check(service.acceptFrame(test.frame("status", 2, "unavailable", null, null, false)), "Unavailable frame rejected")
          expectState("offline", "Offline", Color.urgent, "Not connected")
          service.beginSession()
          check(service.acceptFrame(test.frame("hello", 1, "authenticated", test.me, "wss://fixture.example/", true)), "Authenticated hello rejected")
          check(service.acceptFrame(test.frame("status", 1, "authenticated", test.me, "wss://fixture.example/", true)), "Roster frame rejected")
          test.stage = 1
        } else if (test.stage === 1) {
          if (!service.notificationSettingsDirReady || !service.notificationSettingsLoaded) return
          expectState("online", "Online", "#3fb950", "Fixture Me")
          var account = one("buzzAccount")
          check(account.tooltipText === "Fixture Me · Online · " + test.me.slice(0, 8) + "…", "Tooltip lacks the key prefix: " + account.tooltipText)
          check(one("buzzAccountAvatar").key === test.me && !one("buzzAccountAvatar").usesArt, "Account avatar is not my identicon")
          // My ANSI art shows on the account control once set.
          var art = AnsiArt.sanitize(fixture.text())
          check(art !== "" && service.agents.setOwnAvatarArt(test.me, art, AnsiArt.DEFAULT_BRIGHTNESS), "Fixture art not kept")
          check(one("buzzAccountAvatar").usesColor, "Account avatar does not use my art")
          var roomsView = one("buzzHistoryScroll")
          test.headerClean()
          check(shown("buzzMyAvatarPath").length === 0 && shown("buzzSetMyAvatar").length === 0, "Avatar controls still in the sidebar")

          // Menu: opens on click, closes on Escape without closing the panel.
          test.openMenu()
          check(one("buzzAccountMenuName").text === "Fixture Me" && one("buzzAccountStatePill").text === "Online"
            && one("buzzAccountCommunityHost").text === "fixture.example", "Menu header or community row wrong")
          check(one("buzzAccountCommunity").tooltipText === "Switching communities is not available yet", "Community tooltip missing")
          var menu = one("buzzAccountMenu")
          check(menu.y + menu.height <= account.mapToItem(view, 0, 0).y, "Menu not above the account control")
          input.keyClick(Qt.Key_Escape)
          check(!view.accountMenuOpen && shown("buzzAccountMenu").length === 0 && test.closeRequests === 0, "Escape did not close only the menu")
          // A click on the menu keeps it; a click outside closes it.
          test.openMenu()
          input.mouseClick(one("buzzAccountMenu"), 4, 4)
          check(view.accountMenuOpen, "Click on the menu closed it")
          input.mouseClick(one("buzzAccountMenuLayer"), 4, 4)
          check(!view.accountMenuOpen && test.closeRequests === 0, "Outside click did not close the menu")
          // Keyboard: Enter on the focused control opens it; Escape closes it again.
          one("buzzAccount").forceActiveFocus()
          input.keyClick(Qt.Key_Return)
          check(view.accountMenuOpen, "Enter did not open the menu")
          input.keyClick(Qt.Key_Escape)
          check(!view.accountMenuOpen, "Escape did not close the keyboard-opened menu")

          // Send feedback: the fixed URL, only on click; the menu closes.
          check(test.opened.length === 0, "Something was opened before a click")
          view.feedbackOpener = function(url) { test.opened.push(url) }
          test.openMenu()
          test.click(one("buzzAccountFeedback"))
          check(test.opened.length === 1 && test.opened[0] === "https://github.com/randymy/omarchy-buzz/issues/new" && !view.accountMenuOpen,
            "Send feedback wrong: " + JSON.stringify(test.opened))

          // Settings replaces the room view; Back to rooms returns.
          test.openMenu()
          test.click(one("buzzAccountSettings"))
          check(view.settingsOpen && !view.accountMenuOpen && one("buzzSettingsView").visible && !roomsView.visible,
            "Settings did not replace the room view")
          test.stage = 2
        } else if (test.stage === 2) {
          // One tick later: the layout has placed the settings view.
          var roomsView = findNamed(view, "buzzHistoryScroll", [])[0]
          check(one("buzzSettingsView").mapToItem(view, 0, 0).x > one("buzzAccount").mapToItem(view, 0, 0).x + one("buzzAccount").width,
            "Settings view is not beside the sidebar")
          test.headerClean()
          one("buzzMyAvatarPath")
          one("buzzMyAvatarPathApply")
          check(one("buzzMyAvatarPreview").usesColor && one("buzzMyAvatarBrightness").text === "Brightness 1.5", "Avatar section wrong")

          // Alerts flips notificationsEnabled, both ways.
          var alerts = one("buzzSettingsAlerts")
          var before = service.notificationsEnabled
          test.click(alerts)
          check(service.notificationsEnabled === !before && alerts.text === (before ? "Alerts: off" : "Alerts: on"), "Alerts did not flip")
          test.click(alerts)
          check(service.notificationsEnabled === before, "Alerts did not flip back")

          // Window: hidden without a host switch; the other presentation is requested.
          check(shown("buzzSettingsPresentation").length === 0, "Presentation offered without a host switch")
          view.presentationSwitchEnabled = true
          check(one("buzzSettingsOverlay").selected && !one("buzzSettingsWindow").selected, "Overlay not shown as current")
          test.click(one("buzzSettingsOverlay"))
          check(test.presentationRequests === 0, "Choosing the current presentation switched")
          test.click(one("buzzSettingsWindow"))
          check(test.presentationRequests === 1, "Window did not request the presentation switch")
          view.windowMode = true
          test.click(one("buzzSettingsOverlay"))
          check(test.presentationRequests === 2 && view.settingsOpen, "Overlay did not request the switch or closed Settings")
          view.windowMode = false

          // Shortcut and About.
          check(one("buzzSettingsShortcut").text.indexOf("scripts/desktop-shortcut install") !== -1, "Shortcut note missing")
          check(one("buzzSettingsVersion").text === "Buzz for Omarchy 0.0.15" && one("buzzSettingsRelay").text === "Relay fixture.example",
            "About wrong: " + one("buzzSettingsVersion").text + " · " + one("buzzSettingsRelay").text)
          one("buzzSettingsKey")
          Quickshell.clipboardText = ""
          var cb = one("buzzCopyPublicKey")
          test.click(cb)
          var pos = cb.mapToItem(view, 0, 0)
          test.click(one("buzzCopyPublicKey"))
          check(Quickshell.clipboardText === test.me && /^[a-f0-9]{64}$/.test(Quickshell.clipboardText) && one("buzzCopyPublicKey").text === "Copied",
            "Copy public key did not copy the 64-hex key")

          test.click(one("buzzSettingsBack"))
          check(!view.settingsOpen && shown("buzzSettingsView").length === 0 && roomsView.visible, "Back to rooms did not return")
          // Ctrl+, opens Settings from the panel.
          view.forceActiveFocus()
          input.keyClick(Qt.Key_Comma, Qt.ControlModifier)
          check(view.settingsOpen, "Ctrl+, did not open Settings")
          test.click(one("buzzSettingsBack"))
          check(test.closeRequests === 0 && window.visible, "The panel closed during the checks")

          if (test.captureDir === "") {
            console.log("PASS: the account control shows my avatar and name with a green, amber, red or grey dot per connection state; its menu opens above it, closes on Escape, outside click or a choice, and opens Send feedback's fixed URL only on click; Settings replaces the room view and Back to rooms returns; Alerts flips notificationsEnabled; Overlay/Window request the switch; Copy public key copies the 64-hex key; the header keeps only title, status and Close")
            Qt.quit()
            return
          }
          test.openMenu()
          test.stage = 20
        } else if (test.stage === 20) {
          test.stage = 3
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.captureDir + "/account-menu.png")) { console.error("Could not save the menu"); Qt.exit(1); return }
            test.click(one("buzzAccountSettings"))
            test.stage = 4
          })
        } else if (test.stage === 4) {
          test.stage = 5
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.captureDir + "/settings-view.png")) { console.error("Could not save settings"); Qt.exit(1); return }
            console.log("PASS: capture saved")
            Qt.quit()
          })
        }
      } catch (error) {
        console.error(error.message || error)
        Qt.exit(1)
      }
    }
  }
}
