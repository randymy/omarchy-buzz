import QtQuick
import QtQuick.Layouts
import qs.Ui as Ui
import qs.Commons
import Quickshell.Io
import "AnsiArt.js" as AnsiArt

// Loads avatar art from a local .ans or .txt file the user names by absolute
// path. The file is checked before it is read (a regular file of at most
// 64 KiB), read once with FileView, checked again, then sanitized: only the
// sanitized text is handed on, never the path. Every failure is closed: no art
// changes. Nothing is sent anywhere.
ColumnLayout {
  id: root
  property string fieldName: "buzzAvatarPath"
  property bool showClear: false
  property bool canClear: false
  // idle, reading, loaded or failed.
  property string status: "idle"
  property string problem: ""
  property int generation: 0
  property string pendingPath: ""
  readonly property int maxBytes: 65536
  readonly property alias field: pathField
  signal artLoaded(string art)
  signal clearRequested()
  spacing: Style.space(4)

  // "" when the path may be read; otherwise the reason it is refused.
  function pathProblem(path) {
    if (typeof path !== "string" || path === "") return "Enter the file's full path."
    if (!path.startsWith("/") || path.length > 4096 || /[\u0000-\u001f\u007f]/.test(path))
      return "Use an absolute path, starting with /."
    if (path.split("/").some(function(part) { return part === ".." })) return "Paths with .. are not accepted."
    if (!/\.(ans|txt)$/i.test(path)) return "Choose a .ans or .txt file."
    return ""
  }
  function fail(message) {
    status = "failed"
    problem = message
    pendingPath = ""
    return false
  }
  function load(path) {
    generation++
    var refusal = pathProblem(path)
    if (refusal !== "") return fail(refusal)
    pendingPath = path
    status = "reading"
    problem = ""
    statOutput.expected = generation
    check.command = ["stat", "-L", "--printf", "%F\n%s\n", "--", path]
    check.running = true
    return true
  }
  function finish(expected, text, bytes) {
    if (expected !== generation || status !== "reading") return
    if (bytes > maxBytes) { fail("The file is larger than 64 KiB."); return }
    var art = AnsiArt.sanitize(text)
    if (art === "") { fail("The file holds no art."); return }
    status = "loaded"
    problem = ""
    pendingPath = ""
    artLoaded(art)
  }

  Process {
    id: check
    stdout: StdioCollector {
      id: statOutput
      property int expected: 0
    }
    stderr: SplitParser { onRead: function(line) {} }
    onExited: function(exitCode) {
      if (statOutput.expected !== root.generation || root.status !== "reading") return
      var lines = statOutput.text.split("\n")
      if (exitCode !== 0) { root.fail("The file could not be read."); return }
      if (lines[0] !== "regular file" && lines[0] !== "regular empty file") { root.fail("That path is not a regular file."); return }
      if (!/^\d+$/.test(lines[1] || "") || parseInt(lines[1], 10) > root.maxBytes) { root.fail("The file is larger than 64 KiB."); return }
      file.expected = root.generation
      if (file.path === root.pendingPath) file.reload()
      else file.path = root.pendingPath
    }
  }
  FileView {
    id: file
    property int expected: 0
    watchChanges: false
    printErrors: false
    blockLoading: false
    // The size is checked again after reading: the file may have changed since.
    onLoaded: root.finish(expected, text(), data().byteLength)
    onLoadFailed: if (expected === root.generation && root.status === "reading") root.fail("The file could not be read.")
  }

  Ui.TextField {
    id: pathField
    objectName: root.fieldName
    Layout.fillWidth: true
    verticalPadding: Style.space(3)
    font.pixelSize: Style.font.caption
    maximumLength: 4096
    placeholderText: "/home/you/avatar.ans"
    onAccepted: root.load(text)
  }
  RowLayout {
    Layout.fillWidth: true
    spacing: Style.space(4)
    Ui.Button {
      objectName: root.fieldName + "Apply"
      text: root.status === "reading" ? "Reading…" : "Apply"
      tooltipText: ".ans or .txt, at most 64 KiB; the art is kept, not the path"
      fontSize: Style.font.caption
      focusable: true
      enabled: root.status !== "reading"
      onClicked: root.load(pathField.text)
    }
    Ui.Button {
      objectName: root.fieldName + "Clear"
      visible: root.showClear
      text: "Clear"
      tooltipText: "Remove this avatar art"
      fontSize: Style.font.caption
      focusable: true
      enabled: root.canClear
      opacity: enabled ? 1 : 0.5
      onClicked: { root.status = "idle"; root.problem = ""; root.clearRequested() }
    }
  }
  Text {
    objectName: root.fieldName + "Status"
    visible: text !== ""
    Layout.fillWidth: true
    text: root.problem !== "" ? root.problem : root.status === "loaded" ? "Loaded" : ""
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    color: Color.foreground
    opacity: 0.7
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }
}
