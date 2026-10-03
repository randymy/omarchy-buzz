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
    // Started explicitly below, then restarted on its own after the daemon exits.
    autoRestart: true
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
      if (test.ticks > 65) {
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
        if (!service.reconnecting || service.statusLabel !== "Helper unavailable · reconnecting…" || service.barLabel !== "Reconnecting") {
          console.error("Ended bridge was not shown as reconnecting: " + service.statusLabel + " / " + service.barLabel)
          Qt.exit(1)
        }
        // The daemon comes back (a helper restart or reinstall): the bridge
        // reconnects without Retry.
        daemon.running = true
        test.stage = 4
      } else if (test.stage === 4 && service.connection === "unconfigured" && !service.sessionFailed) {
        if (service.reconnecting) {
          console.error("Restarted bridge still shown as reconnecting")
          Qt.exit(1)
        }
        console.log("PASS: actual QML bridge, setup state, missing helper, retry, daemon exit and automatic bridge restart")
        Qt.quit()
      }
    }
  }
}
