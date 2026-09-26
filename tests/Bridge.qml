import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: 0
  property int ticks: 0
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  Buzz.Service {
    id: missing
    helperExecutable: "/nonexistent/omarchy-buzz-test-helper"
  }
  Process {
    id: daemon
    command: [Quickshell.env("BUZZ_TEST_HELPER"), "daemon", "--keep-running"]
    running: true
  }
  Timer {
    interval: 200
    repeat: true
    running: true
    onTriggered: {
      test.ticks++
      if (test.ticks > 45) {
        console.error("Bridge test timed out at stage " + test.stage)
        Qt.exit(1)
      }
      if (test.stage === 0) {
        service.retry()
        test.stage = 1
      } else if (test.stage === 1 && service.connection === "unconfigured") {
        if (service.rooms.length || service.messages.length || service.sampleMode) {
          console.error("Production service exposed sample rooms")
          Qt.exit(1)
        }
        service.retry()
        test.stage = 2
      } else if (test.stage === 2 && test.ticks > 4) {
        daemon.running = false
        test.stage = 3
      } else if (test.stage === 3 && service.connection === "unavailable" && missing.connection === "unavailable") {
        if (!service.sessionFailed || service.relay !== "") {
          console.error("Helper exit retained usable session state")
          Qt.exit(1)
        }
        console.log("PASS: actual QML bridge, setup state, missing helper, retry and daemon exit")
        Qt.quit()
      }
    }
  }
}
