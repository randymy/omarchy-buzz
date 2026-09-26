import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  property int lostAt: 0
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
      test.ticks++
      if (test.ticks > 100) { console.error("Synthetic composer bridge timed out: " + test.stage + " " + service.connection); Qt.exit(1); return }
      if (test.stage === -1) { service.retry(); test.stage = 0; return }
      if (test.stage === 0 && service.connection === "authenticated" && service.historyState === "snapshot" && service.recipientsState === "snapshot") {
        service.toggleRecipient("c".repeat(64))
        service.updateDraft("Synthetic accepted draft")
        if (!service.submitDraft() || service.submitDraft()) throw new Error("Send or double-click fence failed")
        test.stage = 1
      } else if (test.stage === 1 && service.deliveryState === "acknowledged" && service.historyState === "snapshot") {
        if (service.draftText !== "") throw new Error("Acknowledgement retained unchanged draft")
        record.reload()
        var first = JSON.parse(record.text())
        if (first.sends.length !== 1 || first.fetches !== 2 || first.recipientFetches !== 1) return
        service.updateDraft("Synthetic lost draft")
        if (!service.submitDraft() || service.submitDraft()) throw new Error("Second send fence failed")
        test.stage = 2
      } else if (test.stage === 2 && service.deliveryState === "unknown") {
        if (service.drafts["11111111-1111-4111-8111-111111111111"] !== "Synthetic lost draft" || service.submitDraft()) throw new Error("Lost receipt lost draft or retried")
        test.lostAt = test.ticks
        test.stage = 3
      } else if (test.stage === 3 && test.ticks - test.lostAt > 8) {
        record.reload()
        test.stage = 4
      } else if (test.stage === 4) {
        var finalRecord = JSON.parse(record.text())
        if (finalRecord.sends.length !== 2 || finalRecord.fetches !== 2 || finalRecord.recipientFetches !== 1
            || finalRecord.sends[0].id === finalRecord.sends[1].id)
          throw new Error("Duplicate send, unexpected refresh, or reused draft UUID")
        console.log("PASS: composer Process requests, scope fences, acknowledgement, one refresh, double-click and lost receipt")
        Qt.quit()
      }
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
