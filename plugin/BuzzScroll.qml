import QtQuick
import QtQuick.Controls as Controls
import qs.Commons

// A conversation viewport that stays on the newest row until the reader scrolls up.
Controls.ScrollView {
  id: root
  property bool follow: true
  // A conversation shorter than the viewport rests on its lower edge.
  property bool restOnEnd: false
  function toNewest() {
    var flick = root.contentItem
    flick.contentY = Math.max(-flick.topMargin, flick.contentHeight - flick.height)
  }
  clip: true
  contentWidth: availableWidth
  Binding {
    target: root.contentItem
    property: "topMargin"
    value: root.restOnEnd ? Math.max(0, root.contentItem.height - root.contentHeight) : 0
  }
  Connections {
    target: root.contentItem
    function onContentYChanged() {
      var flick = root.contentItem
      root.follow = flick.contentY >= flick.contentHeight - flick.height - Style.space(24)
    }
    function onContentHeightChanged() { if (root.follow) root.toNewest() }
    function onHeightChanged() { if (root.follow) root.toNewest() }
    function onTopMarginChanged() { if (root.follow) root.toNewest() }
  }
}
