// File attachments against a synthetic stdio helper: no relay, key, network or
// real files. Cards, the verified preview, a completed and a failed download,
// Open, refused and accepted uploads (typed, and through a stub file chooser),
// removal, and a send with an attachment.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  property int uploadsBefore: -1
  // Clicks are spaced beyond the double-click interval: a second press at the
  // same spot would be a double click, which buttons do not take as a click.
  property int lastClick: -10
  readonly property string rowA: "a".repeat(64)
  readonly property string pdfHash: "1".repeat(64)
  readonly property string pngHash: "2".repeat(64)
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 760
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  // Stands in for the desktop file chooser: offscreen, no real dialog is opened.
  QtObject {
    id: chooserStub
    property int opens: 0
    signal accepted(string path)
    signal rejected()
    function open() { opens++ }
  }
  property var fieldHistory: []
  function recordField() { test.fieldHistory.push(one("buzzComposerAttachPath").text) }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  TestCase { id: input; when: false; name: "AttachmentsInput" }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function visibleIn(item) {
    for (var p = item; p; p = p.parent) if (!p.visible) return false
    return true
  }
  function shown(name, within) { return findNamed(within || view, name, []).filter(visibleIn) }
  function one(name, within) {
    var found = shown(name, within)
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  function card(name) {
    var cards = shown("buzzAttachment").filter(function(c) { return c.modelData.name === name })
    if (cards.length !== 1) throw new Error("Expected one card for " + name + ", found " + cards.length)
    return cards[0]
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  function clickItem(item) {
    for (var flick = item.parent; flick; flick = flick.parent) {
      if (typeof flick.contentY !== "number" || typeof flick.contentHeight !== "number" || !flick.contentItem) continue
      var y = item.mapToItem(flick.contentItem, 0, 0).y
      if (y < flick.contentY || y + item.height > flick.contentY + flick.height)
        flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height, y + item.height - flick.height))
      break
    }
    input.mouseClick(item, item.width / 2, item.height / 2)
    // Leave at once: a hover tooltip would cover the next control in the row.
    input.mouseMove(view, 1, 1)
    test.lastClick = test.ticks
  }
  function click(name, within) { clickItem(one(name, within)) }
  // The record is reloaded every tick; a check waits for it to catch up.
  function requests(kind) {
    return JSON.parse(record.text() || '{"requests":[]}').requests.filter(function(r) { return r.type === kind })
  }
  function type(field, text) {
    field.forceActiveFocus()
    field.text = text
  }
  // Enter in the path field attaches, as the Attach button does.
  function attachTyped(text) {
    type(one("buzzComposerAttachPath"), text)
    input.keyClick(Qt.Key_Return)
    test.lastClick = test.ticks
  }
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        record.reload()
        if (test.ticks > 140) throw new Error("Attachments timed out at stage " + test.stage + " " + service.connection + " " + service.historyState + " pending=" + service.pendingAttachments.length + " upload=" + JSON.stringify(service.upload) + " local=" + service.uploadLocal + "/" + service.uploadLocalCategory + " uploads=" + JSON.stringify(requests("upload_attachment")))
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.ticks - test.lastClick < 5) return
        if (test.stage === 0 && service.historyState === "snapshot" && service.messages.length === 2) {
          check(service.attachmentsSupported, "Capability not taken")
          check(shown("buzzAttachment").length === 2, "Expected two attachment cards, found " + shown("buzzAttachment").length)
          var pdf = card("report.pdf")
          check(one("buzzAttachmentSize", pdf).text === "12.3 KB" && one("buzzAttachmentSize", card("shot.png")).text === "2.0 KB",
            "Sizes wrong")
          check(shown("buzzAttachmentOpen").length === 0 && shown("buzzAttachmentStatus").length === 0, "Open or status shown before a download")
          check(one("buzzAttachmentUnavailable").text === "attachment unavailable", "Malformed attachment not reported")
          check(shown("buzzMessageBody").some(function(b) { return b.text === "Here is the report" }), "Message text not shown once")
          test.stage = 1
        } else if (test.stage === 1 && service.thumbnailFor(test.pngHash) !== "") {
          var thumbs = requests("thumbnail_attachment")
          if (thumbs.length === 0) return
          check(thumbs.length === 1 && thumbs[0].eventId === test.rowA && thumbs[0].hash === test.pngHash,
            "Preview not requested exactly once for the image: " + JSON.stringify(thumbs))
          check(shown("buzzAttachmentThumb", card("report.pdf")).length === 0, "A file got a preview")
          test.stage = 2
        } else if (test.stage === 2) {
          var thumb = shown("buzzAttachmentThumb", card("shot.png"))
          if (thumb.length !== 1) return
          check(thumb[0].status === Image.Ready && thumb[0].height > 0 && thumb[0].height <= Style.space(180),
            "Preview not shown bounded: " + thumb[0].status + " " + thumb[0].height)
          check(thumb[0].source.toString().indexOf("file:///") === 0, "Preview is not the helper's local file")
          check(!service.openDownload("/etc/passwd") && !service.downloadAttachment("bad", test.pdfHash), "Unchecked request sent")
          click("buzzAttachmentDownload", card("report.pdf"))
          test.stage = 3
        } else if (test.stage === 3 && service.download.state === "done") {
          var pdfCard = card("report.pdf")
          check(one("buzzAttachmentStatus", pdfCard).text === "Saved to /home/fixture/Downloads/report.pdf",
            "Saved note wrong: " + one("buzzAttachmentStatus", pdfCard).text)
          var open = one("buzzAttachmentOpen", pdfCard)
          check(open.tooltipText === "/home/fixture/Downloads/report.pdf", "Open tooltip is not the saved path")
          check(shown("buzzAttachmentOpen", card("shot.png")).length === 0, "Open shown for a file not downloaded")
          clickItem(open)
          // The preview is clicked to download the image; the helper refuses what does not verify.
          clickItem(one("buzzAttachmentThumb", card("shot.png")))
          test.stage = 4
        } else if (test.stage === 4 && service.download.state === "failed") {
          check(service.download.hash === test.pngHash && service.download.category === "attachment_mismatch", "Wrong failure")
          check(one("buzzAttachmentStatus", card("shot.png")).text === "The file did not match what the message describes, so it was not kept.",
            "Mismatch note wrong")
          check(shown("buzzAttachmentOpen").length === 0 && shown("buzzAttachmentStatus", card("report.pdf")).length === 0,
            "Another download's result still shown")
          click("buzzComposerAttachToggle")
          test.stage = 41
        } else if (test.stage === 41) {
          // Browse… opens the chooser (a stub here); the dialog itself is never made.
          var chooser = one("buzzBrowse", one("buzzComposerAttachRow"))
          chooser.stub = chooserStub
          // By keyboard: the paperclip's hover tooltip can still cover this row offscreen.
          chooser.forceActiveFocus()
          input.keyClick(Qt.Key_Return)
          check(chooserStub.opens === 1 && chooser.dialog === null, "Browse did not open the stub chooser, or made a dialog: " + chooserStub.opens)
          check(chooser.folder.indexOf("file:///") === 0 && chooser.nameFilters.join("|") === "All files (*)|Images (*.png *.jpg *.jpeg *.gif *.webp)",
            "Chooser folder or filters wrong: " + chooser.folder + " " + chooser.nameFilters)
          type(one("buzzComposerAttachPath"), "typed/by hand")
          var localBefore = service.uploadLocal
          // Cancel, a relative path and a path with .. leave the field as it was and send nothing.
          chooserStub.rejected()
          chooserStub.accepted("relative/photo.png")
          check(one("buzzComposerBrowseProblem").text !== "", "Refused choice not explained")
          chooserStub.accepted("file:///home/fixture/../etc/passwd")
          chooserStub.accepted("file:///home/fixture/%2E%2E/x.png")
          chooserStub.accepted("https://example.com/x.png")
          check(one("buzzComposerAttachPath").text === "typed/by hand" && service.uploadLocal === localBefore && localBefore !== "sending",
            "A cancelled or refused choice changed the field or attached: " + one("buzzComposerAttachPath").text + " " + service.uploadLocal)
          check(chooser.localPath("file:///home/fixture/a%20b.png") === "/home/fixture/a b.png"
            && chooser.localPath("file://localhost/home/x") === "/home/x" && chooser.localPath("file:///bad%zz") === "",
            "URL decoding wrong")
          test.stage = 411
        } else if (test.stage === 411) {
          // The revealed row is laid out before it is used.
          attachTyped("notes.txt")
          test.stage = 42
        } else if (test.stage === 42) {
          check(service.uploadLocal === "failed", "Relative path not refused: " + service.uploadLocal + " " + one("buzzComposerAttachPath").text + " " + one("buzzComposerAttach").enabled)
          check(one("buzzComposerUploadStatus").text === "Choose an existing file of yours by its full path (at most four per message).",
            "Relative path not refused locally: " + one("buzzComposerUploadStatus").text)
          test.uploadsBefore = requests("upload_attachment").length
          attachTyped("/home/fixture/evil.svg")
          test.stage = 5
        } else if (test.stage === 5 && service.uploadLocal === "failed") {
          check(one("buzzComposerUploadStatus").text === "This type of file cannot be attached.", "Refusal not explained")
          check(!service.canSendFor(""), "Send enabled with nothing to send")
          type(one("buzzComposerAttachPath"), "/home/fixture/notes.pdf")
          click("buzzComposerAttach")
          test.stage = 6
        } else if (test.stage === 6 && service.pendingAttachments.length === 1 && service.upload.state === "done") {
          check(one("buzzComposerAttachPath").text === "", "Path kept after attaching")
          check(one("buzzPendingName").text === "notes.pdf · 3.2 KB", "Pending item wrong: " + one("buzzPendingName").text)
          check(one("buzzComposerAttachToggle").text === "📎 1", "Paperclip count wrong")
          check(shown("buzzComposerUploadStatus").length === 0, "Stale upload note shown")
          test.stage = 61
        } else if (test.stage === 61) {
          // A chosen file URL is decoded, fills the field and is attached at once.
          var field = one("buzzComposerAttachPath")
          field.textChanged.connect(test.recordField)
          chooserStub.accepted("file:///home/fixture/shot%202.png")
          field.textChanged.disconnect(test.recordField)
          check(test.fieldHistory.join("|") === "/home/fixture/shot 2.png|" && field.text === "" && service.uploadLocal === "sending"
            && shown("buzzComposerBrowseProblem").length === 0, "Chosen file not filled and attached: " + JSON.stringify(test.fieldHistory))
          test.lastClick = test.ticks
          test.stage = 7
        } else if (test.stage === 7 && service.pendingAttachments.length === 2 && service.upload.state === "done") {
          test.stage = 71
        } else if (test.stage === 71) {
          var removes = shown("buzzPendingRemove")
          check(removes.length === 2, "Expected two removable items")
          // By keyboard: offscreen, the pointer's last hover can leave a tooltip over this row.
          removes[1].forceActiveFocus()
          input.keyClick(Qt.Key_Return)
          test.lastClick = test.ticks
          test.stage = 8
        } else if (test.stage === 8 && service.pendingAttachments.length === 1) {
          check(service.pendingAttachments[0].name === "notes.pdf", "Wrong attachment removed")
          check(service.draftText === "" && service.canSendFor(""), "An attachment alone cannot be sent")
          test.stage = 81
        } else if (test.stage === 81) {
          click("buzzComposerSend")
          test.stage = 9
        } else if (test.stage === 9 && service.deliveryState === "acknowledged" && service.pendingAttachments.length === 0) {
          check(shown("buzzPendingAttachment").length === 0, "Sent attachment still listed")
          test.stage = 10
        } else if (test.stage === 10) {
          if (requests("send_message").length !== 1 && test.ticks < 130) return
          check(test.uploadsBefore === 0, "A relative path reached the helper")
          var sends = requests("send_message")
          var uploads = requests("upload_attachment")
          var downloads = requests("download_attachment")
          var opens = requests("open_download")
          var removed = requests("remove_pending_attachment")
          check(sends.length === 1 && sends[0].text === "" && Object.keys(sends[0]).sort().join(",")
            === "generation,id,instanceId,mentions,roomId,text,type,version", "Send wrong: " + JSON.stringify(sends))
          check(uploads.map(function(r) { return r.path }).join("|") === "/home/fixture/evil.svg|/home/fixture/notes.pdf|/home/fixture/shot 2.png"
            && uploads.every(function(r) { return Object.keys(r).sort().join(",") === "id,path,roomId,type,version" }),
            "Uploads wrong: " + JSON.stringify(uploads))
          check(downloads.map(function(r) { return r.hash }).join(",") === test.pdfHash + "," + test.pngHash
            && downloads.every(function(r) { return r.eventId === test.rowA }), "Downloads wrong: " + JSON.stringify(downloads))
          check(opens.length === 1 && opens[0].path === "/home/fixture/Downloads/report.pdf", "Open wrong: " + JSON.stringify(opens))
          check(removed.length === 1 && removed[0].hash === "4".repeat(64), "Removal wrong: " + JSON.stringify(removed))
          console.log("PASS: attachment cards show name, size and a verified bounded preview; downloads report progress, Saved/Open or a refusal; uploads refuse relative paths and SVG; the file chooser fills the path and attaches, decodes file URLs and refuses cancelled, relative and .. choices; pending files are listed with removal, and a message with only an attachment sends and clears them")
          Qt.quit()
        }
      } catch (error) { console.error(error.message || error); Qt.exit(1) }
    }
  }
}
