import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui as Ui
import qs.Commons

// "Browse…" opens the desktop's file chooser and hands on the chosen file as a
// plain absolute path. The chooser is the portal's own dialog
// (xdg-desktop-portal-gtk on Omarchy), asked for by scripts/pick-file in a
// process of its own: a QtQuick file dialog made inside omarchy-shell brought
// the whole shell down. Nothing runs until a person clicks. Tests set `stub`
// to an object with open(), accepted(path) and rejected(): the stub stands in
// for the picker, which is then never started.
Ui.Button {
  id: root
  objectName: "buzzBrowse"
  text: picking ? "Choosing…" : "Browse…"
  tooltipText: "Choose a file"
  fontSize: Style.font.caption
  focusable: true
  enabled: !picking

  property string title: "Choose a file"
  // "Label (*.ext *.other)" entries, shown by the chooser as its filters.
  property var nameFilters: ["All files (*)"]
  // Where the chooser opens; it follows the last file chosen here, for this session.
  property string folder: Quickshell.env("HOME") || "/"
  property var stub: null
  // The shared service, told while a dialog is up (`filePickersOpen`).
  property var service: null
  // True while the picker process is up (a dialog is on screen).
  readonly property bool picking: picker.running
  // Why the last choice was refused, or "".
  property string problem: ""
  onPickingChanged: if (service) service.filePickersOpen = Math.max(0, service.filePickersOpen + (picking ? 1 : -1))
  Component.onDestruction: if (picking && service) service.filePickersOpen = Math.max(0, service.filePickersOpen - 1)
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
    folder = path.slice(0, path.lastIndexOf("/")) || "/"
    chosen(path)
    return true
  }
  function open() {
    if (stub) { stub.open(); return }
    if (picker.running) return
    var command = ["/usr/bin/python3", pickerScript, root.title, root.folder]
    for (var i = 0; i < root.nameFilters.length; i++) command.push(String(root.nameFilters[i]))
    picker.command = command
    picker.running = true
  }
  onClicked: open()

  // scripts/pick-file, beside the plugin directory, as a plain path.
  readonly property string pickerScript: {
    var url = Qt.resolvedUrl("../scripts/pick-file").toString()
    return url.startsWith("file://") ? decodeURIComponent(url.slice(7)) : url
  }

  Connections {
    target: root.stub
    ignoreUnknownSignals: true
    function onAccepted(path) { root.take(path) }
    function onRejected() { root.canceled() }
  }
  Process {
    id: picker
    stdout: StdioCollector { id: pickerOutput }
    stderr: StdioCollector { id: pickerErrors }
    onExited: function(exitCode) {
      var line = pickerOutput.text.split("\n")[0] || ""
      if (exitCode === 0) { root.take(line); return }
      if (exitCode === 1) { root.canceled(); return }
      root.problem = "The file chooser could not be opened."
      var detail = pickerErrors.text.trim()
      if (detail !== "") console.warn("Buzz: " + detail.split("\n")[0])
      root.refused(root.problem)
    }
  }
}
