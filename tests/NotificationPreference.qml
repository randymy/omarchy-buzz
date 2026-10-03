// Offscreen persistence check; each phase runs in a fresh Quickshell process.
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
      if (!service.notificationSettingsLoaded || !service.notificationSettingsDirReady) {
        if (++attempts > 100) { console.error("Preference did not load"); Qt.exit(1) }
        return
      }
      stop()
      var phase = Quickshell.env("BUZZ_PREFERENCE_PHASE")
      // Fresh state is the default; each phase then checks what the last one saved.
      var expected = ({enable: ["direct", true], disable: ["all", false], invalid: ["none", true], legacy: ["all", true], legacyoff: ["none", true]})[phase]
      if (service.notificationMode !== expected[0] || service.notificationText !== expected[1]) {
        console.error("Unexpected notification preference in phase " + phase + ": "
          + service.notificationMode + ", " + service.notificationText)
        Qt.exit(1)
        return
      }
      if (phase === "enable") {
        service.notificationMode = "all"
        service.notificationText = false
        settled.start()
      } else if (phase === "disable") {
        service.notificationMode = "none"
        service.notificationText = true
        settled.start()
      } else Qt.quit()
    }
  }
  Timer { id: settled; interval: 400; onTriggered: Qt.quit() }
}
