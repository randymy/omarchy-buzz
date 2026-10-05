import QtQuick
import QtQuick.Controls as Controls
import qs.Commons
import "Links.js" as Links
import "PlainText.js" as PlainText

// The only item in the plugin that renders message text as rich text, and only
// for text that contains links. Message text is plain text and must never be
// read as markup, so it is never handed to the renderer as it is:
//   - `Links.render()` escapes every character outside a link, and wraps each
//     link in an `<a href="buzz-link:N">` whose N indexes a list of URLs found
//     by `Links.detect()` (never the URL itself as an href);
//   - no other tag can appear, so no <img>, <font>, <style> or foreign <a>;
//   - text without links stays Text.PlainText.
// tests/test_no_rich_text.py fails if any other QML file asks for RichText or
// StyledText. Opening is not done here: this item only reports which link was
// chosen, and the panel asks the helper, which checks the URL again.
TextEdit {
  id: root
  // The message text, exactly as sent.
  property string source: ""
  // Off for sample data and for a helper that cannot open links: plain text only.
  property bool linkify: true
  property color linkColor: Color.accent
  readonly property var rendered: linkify ? Links.render(source, linkColor.toString()) : ({html: "", urls: []})
  readonly property var urls: rendered.urls
  readonly property bool hasLinks: urls.length > 0
  // The text as shown, without markup.
  readonly property string shownText: getText(0, length)
  // The link under the pointer (an index into `urls`), or -1.
  property int hoveredIndex: -1
  // What the tooltip says: the host the link goes to, then the whole URL.
  readonly property string tipSource: hoveredIndex >= 0 && hoveredIndex < urls.length ? Links.describe(urls[hoveredIndex]) : ""
  // The last link put on the clipboard (for the panel to confirm).
  property string lastCopied: ""
  // A link was clicked; `floating` is true for shift-click.
  signal linkChosen(int index, string url, bool floating)

  function linkIndexAt(x, y) {
    return urls.length === 0 ? -1 : Links.indexOf(linkAt(x, y), urls.length)
  }
  function choose(index, floating) {
    if (index < 0 || index >= urls.length) return false
    linkChosen(index, urls[index], floating)
    return true
  }
  // The same clipboard path as the Copy button: a TextEdit's own copy().
  function copyLink(index) {
    if (index < 0 || index >= urls.length) return false
    scratch.text = urls[index]
    scratch.selectAll()
    scratch.copy()
    scratch.deselect()
    lastCopied = urls[index]
    return true
  }
  function copyAll() {
    selectAll()
    copy()
    deselect()
  }
  // The text is replaced, not rebound: the format never changes while old text
  // is still set, so plain text is never briefly read as markup.
  function refresh() {
    text = ""
    textFormat = hasLinks ? TextEdit.RichText : TextEdit.PlainText
    text = hasLinks ? rendered.html : source
  }
  onRenderedChanged: refresh()
  onSourceChanged: refresh()
  Component.onCompleted: refresh()
  onLinkHovered: (link) => root.hoveredIndex = Links.indexOf(link, root.urls.length)

  TextEdit {
    id: scratch
    visible: false
    width: 0
    height: 0
    readOnly: true
    textFormat: TextEdit.PlainText
  }
  // Left click opens in Omarchy's browser, shift-click in a floating window.
  // (TextEdit's own linkActivated is not used: it cannot tell the modifiers.)
  TapHandler {
    acceptedButtons: Qt.LeftButton
    acceptedModifiers: Qt.NoModifier
    onTapped: (point) => root.choose(root.linkIndexAt(point.position.x, point.position.y), false)
  }
  TapHandler {
    acceptedButtons: Qt.LeftButton
    acceptedModifiers: Qt.ShiftModifier
    onTapped: (point) => root.choose(root.linkIndexAt(point.position.x, point.position.y), true)
  }
  HoverHandler {
    cursorShape: root.hoveredIndex >= 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
  }
  Controls.ToolTip.visible: root.tipSource !== ""
  Controls.ToolTip.delay: 300
  Controls.ToolTip.text: PlainText.tip(root.tipSource)
}
