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
      var expected = phase === "disable"
      if (service.notificationsEnabled !== expected) {
        console.error("Unexpected notification preference in phase " + phase)
        Qt.exit(1)
        return
      }
      if (phase === "enable" || phase === "disable") {
        service.notificationsEnabled = !expected
        settled.start()
      } else Qt.quit()
    }
  }
  Timer { id: settled; interval: 400; onTriggered: Qt.quit() }
}
