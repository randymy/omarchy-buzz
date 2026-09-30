// Synthetic stdio helper only; no relay or identity access.
import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  property string seen: ""
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  Timer {
    interval: 50
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 200) { console.error("Synthetic room check timed out: " + test.stage + " " + service.resyncStage + " requests=" + test.seen); Qt.exit(1); return }
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.resyncStage === "catalog") {
          if (service.historyState !== "snapshot" || service.recipientsState !== "snapshot" || !service.selectedRoom
              || service.messageAuthorName("c".repeat(64)) !== "Synthetic person")
            throw new Error("Room check blanked the conversation")
          service.toggleRecipient("c".repeat(64))
          service.updateDraft("Synthetic held draft")
          if (!service.submitDraft() || service.submitDraft() || service.deliveryState !== "sending")
            throw new Error("Submission during the room check was refused or duplicated")
          test.stage = 1
        } else if (test.stage === 1 && service.deliveryState === "acknowledged") {
          if (service.draftText !== "" || service.resyncStage !== "" || service.heldSubmission !== "")
            throw new Error("Held submission did not complete")
          record.reload()
          var seen = JSON.parse(record.text()).requests.join(",")
          test.seen = seen
          // The record is written by the fixture process; wait for its final request.
          if (seen !== "fetch_recent,fetch_recipients,room_check_started,room_check_finished,fetch_recent,fetch_recipients,send_message,fetch_recent")
            return
          console.log("PASS: a submission made during the joined-room check is written once, after history and roster are revalidated")
          Qt.quit()
        } else if (test.stage === 1 && ["failed", "unknown", "rejected"].indexOf(service.deliveryState) !== -1)
          throw new Error("Held submission ended as " + service.deliveryState)
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
