import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons

// One message or reply. Presentation only: every value is an already validated projection.
Column {
  id: root
  property var service: null
  property var row: ({})
  // Room rows offer their thread; replies and the thread's own root do not.
  property bool threadLink: false
  property bool threadSelected: false
  property bool showDate: false
  readonly property bool sample: !!service && service.sampleMode
  readonly property bool lead: !row.grouped
  readonly property bool ready: !!service && !!row && typeof row.author === "string"
  signal threadRequested()
  spacing: Style.space(2)

  Item {
    visible: root.row.dayBreak === true
    width: parent.width
    height: visible ? Style.space(24) : 0
    Rectangle {
      anchors.verticalCenter: parent.verticalCenter
      width: parent.width
      height: 1
      color: Util.alpha(Color.foreground, 0.14)
    }
    Rectangle {
      anchors.centerIn: parent
      width: dayLabel.implicitWidth + Style.space(16)
      height: parent.height
      color: Color.popups.background
      Text {
        id: dayLabel
        anchors.centerIn: parent
        text: root.row.dayBreak === true && root.service ? root.service.formatDay(root.row.time) : ""
        textFormat: Text.PlainText
        color: Color.foreground
        opacity: 0.7
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
    }
  }
  RowLayout {
    width: parent.width
    spacing: Style.space(8)
    Column {
      Layout.fillWidth: true
      Layout.alignment: Qt.AlignTop
      spacing: Style.space(2)
      Row {
        visible: root.lead
        width: parent.width
        spacing: Style.space(8)
        Text {
          id: author
          text: !root.ready ? "" : root.sample ? root.row.author + " · " + root.row.role : root.service.messageAuthorName(root.row.author)
          textFormat: Text.PlainText
          elide: Text.ElideRight
          width: Math.min(implicitWidth, parent.width - stamp.width - agentTag.width - parent.spacing * 2)
          color: Color.accent
          font.family: Style.font.family
          font.pixelSize: Style.font.body
          font.bold: true
          Controls.ToolTip.visible: authorHover.containsMouse && !root.sample
          Controls.ToolTip.text: root.sample || !root.ready ? "" : root.row.author
          MouseArea {
            id: authorHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }
        }
        Text {
          id: agentTag
          anchors.baseline: author.baseline
          visible: root.ready && !root.sample && root.service.participantLabel(root.row.author) !== "Participant"
          width: visible ? implicitWidth : 0
          text: "agent"
          textFormat: Text.PlainText
          color: Color.accent
          opacity: 0.8
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
          Controls.ToolTip.visible: agentHover.containsMouse
          Controls.ToolTip.text: "Self-described agent · what it is doing is not known here"
          MouseArea {
            id: agentHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }
        }
        Text {
          id: stamp
          anchors.baseline: author.baseline
          text: !root.ready ? "" : root.sample ? root.row.time
            : (root.showDate ? root.service.formatDay(root.row.time) + " " : "") + root.service.formatTime(root.row.time)
          textFormat: Text.PlainText
          color: Color.foreground
          opacity: 0.6
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
          Controls.ToolTip.visible: stampHover.containsMouse && !root.sample
          Controls.ToolTip.text: root.sample || !root.ready ? "" : root.service.formatTimestamp(root.row.time)
          MouseArea {
            id: stampHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }
        }
      }
      Text {
        width: parent.width
        text: !root.ready ? "" : root.sample ? root.row.text : (root.row.unavailable ? "Content unavailable" : root.row.text)
          + (root.row.edited ? " (edited)" : "") + (root.row.truncated ? " [truncated]" : "")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Color.foreground
        opacity: root.row.unavailable ? 0.6 : 1
        font.family: Style.font.family
        font.pixelSize: Style.font.body
      }
      Row {
        objectName: "buzzMessageReactions"
        visible: !!root.row.reactions && (root.row.reactions.seen > 0 || root.row.reactions.working > 0)
        spacing: Style.space(10)
        Text {
          visible: !!root.row.reactions && root.row.reactions.seen > 0
          text: "👀 " + (root.row.reactions ? root.row.reactions.seen : 0)
          textFormat: Text.PlainText
          color: Color.foreground
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
          Controls.ToolTip.visible: seenHover.containsMouse
          Controls.ToolTip.text: "Queued reaction (snapshot)"
          MouseArea {
            id: seenHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }
        }
        Text {
          visible: !!root.row.reactions && root.row.reactions.working > 0
          text: "💬 " + (root.row.reactions ? root.row.reactions.working : 0)
          textFormat: Text.PlainText
          color: Color.foreground
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
          Controls.ToolTip.visible: workingHover.containsMouse
          Controls.ToolTip.text: "Working reaction (snapshot) · not a reply count"
          MouseArea {
            id: workingHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }
        }
      }
    }
    Ui.Button {
      objectName: root.threadLink ? "buzzThreadToggle" : ""
      property string messageId: root.row.id || ""
      Layout.alignment: Qt.AlignTop
      visible: root.threadLink && root.ready && root.service.canOpenThread(root.row.id)
      text: "Thread ›"
      tooltipText: root.threadSelected ? "Close this thread" : "Open replies beside the room"
      fontSize: Style.font.caption
      horizontalPadding: Style.space(6)
      verticalPadding: Style.space(2)
      foreground: root.threadSelected ? Color.foreground : Util.alpha(Color.foreground, 0.6)
      selected: root.threadSelected
      focusable: true
      onClicked: root.threadRequested()
    }
  }
}
