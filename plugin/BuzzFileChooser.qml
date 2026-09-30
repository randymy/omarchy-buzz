import QtQuick
import QtQuick.Dialogs
import Quickshell
import qs.Ui as Ui
import qs.Commons

// "Browse…" opens the desktop's file chooser (through xdg-desktop-portal on
// Wayland) and hands on the chosen file as a plain absolute path. The dialog
// is created only on the first click, so nothing is opened or created until a
// person asks for it. Tests set `stub` to an object with open(), accepted(path)
// and rejected(): the stub stands in for the dialog, which is then never made.
Ui.Button {
  id: root
  objectName: "buzzBrowse"
  text: "Browse…"
  tooltipText: "Choose a file"
  fontSize: Style.font.caption
  focusable: true

  property string title: "Choose a file"
  property var nameFilters: ["All files (*)"]
  // Where the chooser opens; it follows the last file chosen here, for this session.
  property string folder: {
    var home = Quickshell.env("HOME") || ""
    return home.startsWith("/") ? "file://" + encodeURI(home) : "file:///"
  }
  property var stub: null
  // Made lazily; null until the first real open.
  property var dialog: null
  // Why the last choice was refused, or "".
  property string problem: ""
  signal chosen(string path)
  signal refused(string reason)
  signal canceled()

  // A plain absolute path from a file:// URL (or a plain path), or "" when refused.
  function localPath(value) {
    var text = value === null || value === undefined ? "" : String(value)
    if (text.startsWith("file://")) {
      text = text.slice(7)
      if (text.startsWith("localhost/")) text = text.slice(9)
      try { text = decodeURIComponent(text) } catch (error) { return "" }
    } else if (/^[A-Za-z][A-Za-z0-9+.-]*:/.test(text)) {
      return ""
    }
    if (!text.startsWith("/") || text.length > 4096 || /[\u0000-\u001f\u007f]/.test(text)) return ""
    if (text.split("/").some(function(part) { return part === ".." })) return ""
    return text
  }
  function take(value) {
    var path = localPath(value)
    if (path === "") {
      problem = "Only a file's full path can be used; this choice was refused."
      refused(problem)
      return false
    }
    problem = ""
    folder = "file://" + encodeURI(path.slice(0, path.lastIndexOf("/")) || "/")
    chosen(path)
    return true
  }
  function open() {
    if (stub) { stub.open(); return }
    if (!dialog) dialog = dialogComponent.createObject(root)
    if (!dialog) return
    dialog.title = root.title
    dialog.nameFilters = root.nameFilters
    dialog.currentFolder = root.folder
    dialog.open()
  }
  onClicked: open()

  Connections {
    target: root.stub
    ignoreUnknownSignals: true
    function onAccepted(path) { root.take(path) }
    function onRejected() { root.canceled() }
  }
  Component {
    id: dialogComponent
    FileDialog {
      fileMode: FileDialog.OpenFile
      onAccepted: root.take(selectedFile.toString())
      onRejected: root.canceled()
    }
  }
}
