import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons
import "PlainText.js" as PlainText

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
  // The author's verified status from the room's roster (`user_status`), if any.
  readonly property var authorStatus: ready && lead ? service.authorStatus(row.author) : null
  // Thread replies nest compactly: at most three steps of indentation.
  readonly property int depth: Number.isInteger(row.depth) ? row.depth : 1
  readonly property real indent: Math.min(Math.max(depth - 1, 0), 3) * Style.space(14)
  signal threadRequested()
  // The author's avatar was clicked: the panel shows a profile card.
  signal avatarRequested(string key, string name, string art, real brightness)
  spacing: Style.space(2)
  leftPadding: indent

  Item {
    visible: root.row.dayBreak === true
    width: root.width - root.indent
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
    width: root.width - root.indent
    spacing: Style.space(8)
    // Lead rows show the author's avatar; grouped rows keep its width so text aligns.
    Item {
      Layout.alignment: Qt.AlignTop
      Layout.topMargin: Style.space(2)
      implicitWidth: avatar.implicitWidth
      implicitHeight: avatar.visible ? avatar.implicitHeight : 0
      BuzzAvatar {
        id: avatar
        visible: root.lead && root.ready
        key: root.ready ? root.row.author : ""
        name: !root.ready ? "" : root.sample ? root.row.author : root.service.messageAuthorName(root.row.author)
        // Art kept on this machine: its enrolled agents' and this user's own; everyone else has an identicon.
        art: root.ready && !root.sample && root.service.agents ? root.service.agents.avatarArtForKey(root.row.author) : ""
        brightness: root.ready && !root.sample && root.service.agents ? root.service.agents.avatarBrightnessForKey(root.row.author) : 0
        clickable: visible
        onActivated: root.avatarRequested(key, name, art, brightness)
      }
    }
    Column {
      Layout.fillWidth: true
      Layout.alignment: Qt.AlignTop
      spacing: Style.space(2)
      Text {
        objectName: "buzzThreadReplyCaption"
        visible: root.ready && root.lead && root.depth > 1 && typeof root.row.parentAuthor === "string"
        width: parent.width
        text: visible ? "↳ replying to " + (root.row.parentAuthor ? root.service.messageAuthorName(root.row.parentAuthor) : "an earlier reply") : ""
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: Color.foreground
        opacity: 0.55
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
      Row {
        visible: root.lead
        width: parent.width
        spacing: Style.space(8)
        // The author's verified presence (`presence`), when known.
        PresenceDot {
          id: presenceTag
          objectName: "buzzAuthorPresence"
          anchors.verticalCenter: author.verticalCenter
          service: root.service
          presence: root.ready && root.lead ? root.service.presenceOf(root.row.author) : ""
        }
        Text {
          id: author
          text: !root.ready ? "" : root.sample ? root.row.author + " · " + root.row.role : root.service.messageAuthorName(root.row.author)
          textFormat: Text.PlainText
          elide: Text.ElideRight
          width: Math.min(implicitWidth, parent.width - stamp.width - agentTag.width - statusTag.width - presenceTag.width - parent.spacing * 4)
          color: Color.accent
          font.family: Style.font.family
          font.pixelSize: Style.font.body
          font.bold: true
          Controls.ToolTip.visible: authorHover.containsMouse && !root.sample
          Controls.ToolTip.text: PlainText.tip(root.sample || !root.ready ? "" : root.row.author)
          MouseArea {
            id: authorHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }
        }
        Text {
          id: statusTag
          objectName: "buzzAuthorStatus"
          anchors.baseline: author.baseline
          visible: !!root.authorStatus
          // No explicit width: an emoji's implicit width must not depend on it.
          text: visible ? root.service.statusEmojiText(root.authorStatus) : ""
          textFormat: Text.PlainText
          color: Color.foreground
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
          Controls.ToolTip.visible: statusHover.containsMouse && root.authorStatus && root.authorStatus.text !== ""
          Controls.ToolTip.text: PlainText.tip(root.authorStatus ? root.authorStatus.text : "")
          MouseArea {
            id: statusHover
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
          Controls.ToolTip.text: PlainText.tip("Self-described agent · what it is doing is not known here")
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
          Controls.ToolTip.text: PlainText.tip(root.sample || !root.ready ? "" : root.service.formatTimestamp(root.row.time))
          MouseArea {
            id: stampHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }
        }
      }
      // Read-only so the text can be selected and copied like any other text.
      TextEdit {
        id: body
        objectName: "buzzMessageBody"
        width: parent.width
        // An attachment-only message has no text line of its own.
        visible: !(root.ready && !root.sample && root.row.text === "" && !root.row.unavailable && !root.row.edited
          && !root.row.truncated && Array.isArray(root.row.attachments) && root.row.attachments.length > 0)
        height: visible ? implicitHeight : 0
        text: !root.ready ? "" : root.sample ? root.row.text : (root.row.unavailable ? "Content unavailable" : root.row.text)
          + (root.row.edited ? " (edited)" : "") + (root.row.truncated ? " [truncated]" : "")
        textFormat: TextEdit.PlainText
        wrapMode: TextEdit.Wrap
        readOnly: true
        selectByMouse: true
        selectionColor: Util.alpha(Color.accent, 0.35)
        selectedTextColor: Color.foreground
        color: Color.foreground
        opacity: root.row.unavailable ? 0.6 : 1
        font.family: Style.font.family
        font.pixelSize: Style.font.body
        function copyAll() {
          selectAll()
          copy()
          deselect()
        }
      }
      // Attachments: verified by the helper before anything is saved or shown.
      Repeater {
        model: root.ready && !root.sample && !root.row.unavailable && Array.isArray(root.row.attachments) ? root.row.attachments : []
        delegate: Rectangle {
          id: card
          required property var modelData
          readonly property string target: root.row.id + ":" + modelData.hash
          readonly property string thumb: root.service.thumbnailUrl(modelData.hash)
          readonly property string saved: root.service.downloadedPath(root.row.id, modelData.hash)
          readonly property string note: root.service.downloadLabelFor(root.row.id, modelData.hash)
          objectName: "buzzAttachment"
          width: Math.min(parent ? parent.width : 0, Style.space(420))
          height: cardColumn.implicitHeight + Style.space(12)
          radius: Style.cornerRadius
          color: Util.alpha(Color.foreground, 0.04)
          border.color: Util.alpha(Color.foreground, 0.16)
          Component.onCompleted: root.service.wantThumbnail(root.row.id, modelData)
          Column {
            id: cardColumn
            x: Style.space(6)
            y: Style.space(6)
            width: card.width - Style.space(12)
            spacing: Style.space(4)
            // Shown only once the helper reports a verified image file.
            Image {
              objectName: "buzzAttachmentThumb"
              visible: card.thumb !== "" && status === Image.Ready
              width: Math.min(cardColumn.width, implicitWidth)
              height: visible ? Math.min(Style.space(180), implicitHeight * (width / Math.max(1, implicitWidth))) : 0
              source: card.thumb
              sourceSize.height: Style.space(360)
              fillMode: Image.PreserveAspectFit
              asynchronous: true
              cache: false
              smooth: true
              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.service.downloadAttachment(root.row.id, card.modelData.hash)
              }
            }
            RowLayout {
              width: cardColumn.width
              spacing: Style.space(6)
              Text {
                text: card.modelData.kind === "image" ? "🖼" : card.modelData.kind === "video" ? "🎞" : "📄"
                textFormat: Text.PlainText
                color: Color.foreground
                font.family: Style.font.family
                font.pixelSize: Style.font.body
              }
              Text {
                objectName: "buzzAttachmentName"
                Layout.fillWidth: true
                text: card.modelData.name
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
                color: Color.foreground
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                Controls.ToolTip.visible: nameHover.containsMouse
                Controls.ToolTip.text: PlainText.tip(card.modelData.name + " · " + card.modelData.mime)
                MouseArea {
                  id: nameHover
                  anchors.fill: parent
                  hoverEnabled: true
                  acceptedButtons: Qt.NoButton
                }
              }
              Text {
                objectName: "buzzAttachmentSize"
                text: root.service.formatSize(card.modelData.size)
                textFormat: Text.PlainText
                color: Color.foreground
                opacity: 0.6
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
              }
              Ui.Button {
                objectName: "buzzAttachmentDownload"
                text: "Download"
                tooltipText: "Save to Downloads after checking it matches the message"
                fontSize: Style.font.caption
                horizontalPadding: Style.space(6)
                verticalPadding: Style.space(2)
                focusable: true
                // Busy is refused by the service, not by disabling the control under the pointer.
                enabled: root.service.attachmentsAvailable
                opacity: root.service.downloadBusy ? 0.5 : 1
                onClicked: root.service.downloadAttachment(root.row.id, card.modelData.hash)
              }
              Ui.Button {
                objectName: "buzzAttachmentOpen"
                visible: card.saved !== ""
                text: "Open"
                tooltipText: card.saved
                fontSize: Style.font.caption
                horizontalPadding: Style.space(6)
                verticalPadding: Style.space(2)
                focusable: true
                onClicked: root.service.openDownload(card.saved)
              }
            }
            Text {
              objectName: "buzzAttachmentStatus"
              visible: card.note !== ""
              width: cardColumn.width
              text: card.note
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              color: Color.foreground
              opacity: 0.7
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
          }
        }
      }
      Text {
        objectName: "buzzAttachmentUnavailable"
        visible: root.ready && !root.sample && root.row.attachmentsUnavailable === true
        text: "attachment unavailable"
        textFormat: Text.PlainText
        color: Color.foreground
        opacity: 0.5
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
        font.italic: true
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
          Controls.ToolTip.text: PlainText.tip("Queued reaction (snapshot)")
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
          Controls.ToolTip.text: PlainText.tip("Working reaction (snapshot) · not a reply count")
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
      objectName: "buzzCopyMessage"
      Layout.alignment: Qt.AlignTop
      visible: root.ready && !root.row.unavailable && root.row.text !== ""
      text: "Copy"
      tooltipText: "Copy this message's text"
      fontSize: Style.font.caption
      horizontalPadding: Style.space(6)
      verticalPadding: Style.space(2)
      foreground: Util.alpha(Color.foreground, 0.6)
      focusable: true
      onClicked: body.copyAll()
    }
    Ui.Button {
      objectName: root.threadLink ? "buzzThreadToggle" : ""
      property string messageId: root.row.id || ""
      Layout.alignment: Qt.AlignTop
      visible: root.threadLink && root.ready && root.service.canOpenThread(root.row.id)
      // Counts come only from a relay summary. Without the helper capability
      // nothing is known about replies, so the neutral label stays.
      readonly property var summary: root.row && root.row.thread ? root.row.thread : null
      readonly property int replies: summary ? summary.replies : 0
      text: replies > 0 ? replies + (replies === 1 ? " reply ›" : " replies ›")
        : root.ready && root.service.threadSummariesSupported ? "Reply ›" : "Thread ›"
      tooltipText: root.threadSelected ? "Close this thread"
        : replies > 0 && summary.lastReplyAt !== null ? "Last reply " + root.service.formatTimestamp(summary.lastReplyAt)
        : "Open replies beside the room"
      fontSize: Style.font.caption
      horizontalPadding: Style.space(6)
      verticalPadding: Style.space(2)
      foreground: root.threadSelected || replies > 0 ? Color.foreground : Util.alpha(Color.foreground, 0.6)
      selected: root.threadSelected
      focusable: true
      onClicked: root.threadRequested()
    }
  }
}
