import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons

FocusScope {
  id: root
  property var service: null
  property bool presentationSwitchEnabled: false
  property bool windowMode: false
  property alias recipientPickerExpanded: roomComposer.pickerExpanded
  readonly property bool connected: !!service && (service.sampleMode || service.connection === "authenticated")
  readonly property bool threadOpen: !!service && service.threadRootId !== "" && service.threadRoot !== null
  // An open thread sits beside the room. Narrow windows give up the room list
  // first, then the room itself, so the thread always has a readable column.
  readonly property bool showRooms: !threadOpen || width >= Style.space(1100)
  readonly property bool showTimeline: !threadOpen || width >= Style.space(640)
  readonly property var activeComposer: threadOpen && service.replyRootId !== "" ? threadComposer : roomComposer
  readonly property var mentionMatches: activeComposer.mentionMatches
  readonly property bool mentionOpen: activeComposer.mentionOpen
  function chooseMention(index) { return activeComposer.chooseMention(index) }

  // Keep delegates alive across snapshots; only changed rows are updated.
  readonly property var incomingMessages: service ? service.messages : []
  readonly property var incomingReplies: service ? service.threadRows : []
  onIncomingMessagesChanged: syncRows(messageModel, incomingMessages, null)
  onIncomingRepliesChanged: syncRows(replyModel, incomingReplies, service ? service.threadRoot : null)
  ListModel { id: messageModel; dynamicRoles: true }
  ListModel { id: replyModel; dynamicRoles: true }
  function syncRows(model, rows, before) {
    rows = rows || []
    for (var i = 0; i < rows.length; i++) {
      var key = rows[i].id || ("sample-" + i)
      var found = -1
      for (var j = i; j < model.count; j++) {
        if (model.get(j).eventKey === key) { found = j; break }
      }
      var previous = i > 0 ? rows[i - 1] : before
      var timed = typeof rows[i].time === "number" && (!previous || typeof previous.time === "number")
      var dayBreak = timed && (!previous || service.dayKey(previous.time) !== service.dayKey(rows[i].time))
      // Consecutive messages from one author within five minutes share a header.
      var grouped = timed && !!previous && previous !== before && !dayBreak && previous.author === rows[i].author
        && rows[i].time >= previous.time && rows[i].time - previous.time < 300
      var serialized = JSON.stringify(Object.assign({}, rows[i], {dayBreak: dayBreak, grouped: grouped}))
      if (found === -1) model.insert(i, {eventKey: key, payload: serialized})
      else {
        if (found !== i) model.move(found, i, 1)
        if (model.get(i).payload !== serialized) model.setProperty(i, "payload", serialized)
      }
    }
    if (model.count > rows.length) model.remove(rows.length, model.count - rows.length)
  }
  function toggleThread(id) {
    if (!service) return
    if (service.threadRootId === id) closeThread()
    else {
      service.openThread(id)
      threadScroll.follow = true
    }
  }
  function closeThread() {
    if (!service) return
    var composing = service.replyRootId !== ""
    service.closeThread()
    // The reply draft stays with its thread; the room composer takes over.
    if (composing) service.composeRoom()
    roomComposer.field.forceActiveFocus()
  }
  signal closeRequested()
  signal presentationRequested()
  Keys.onEscapePressed: closeRequested()

  Rectangle {
    anchors.fill: parent
    color: Color.popups.background
    border.color: Color.popups.border
    border.width: Math.max(1, Style.space(1))
    radius: Style.cornerRadius
  }

  ColumnLayout {
    anchors.fill: parent
    anchors.margins: Style.space(14)
    spacing: Style.space(10)

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(10)
      Text {
        text: "Buzz"
        textFormat: Text.PlainText
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.body * 1.3
        font.bold: true
      }
      Text {
        Layout.fillWidth: true
        text: !root.service ? "Service unavailable" : root.service.sampleMode ? "Sample data"
          : (root.service.sendSupported || root.service.connection !== "authenticated" ? "" : "Read-only · ") + root.service.statusLabel
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: Color.foreground
        opacity: 0.6
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
      Ui.Button {
        visible: root.presentationSwitchEnabled
        text: root.windowMode ? "Overlay" : "Window"
        fontSize: Style.font.caption
        focusable: true
        onClicked: root.presentationRequested()
      }
      Ui.Button {
        text: root.service && root.service.notificationsEnabled ? "Alerts: on" : "Alerts: off"
        fontSize: Style.font.caption
        focusable: true
        onClicked: if (root.service) root.service.notificationsEnabled = !root.service.notificationsEnabled
      }
      Ui.Button {
        text: "Close · Esc"
        fontSize: Style.font.caption
        focusable: true
        onClicked: root.closeRequested()
      }
    }

    Rectangle {
      visible: !!root.service && root.service.sampleMode
      Layout.fillWidth: true
      implicitHeight: previewLabel.implicitHeight + Style.space(16)
      color: Util.alpha(Color.accent, 0.1)
      radius: Style.cornerRadius
      Text {
        id: previewLabel
        anchors.fill: parent
        anchors.margins: Style.space(8)
        text: "TEST FIXTURE · Sample data only\nNo relay connected. Messages below are examples."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.body
      }
    }

    RowLayout {
      Layout.fillWidth: true
      Layout.fillHeight: true
      spacing: Style.space(14)

      ColumnLayout {
        visible: root.showRooms
        Layout.preferredWidth: Style.space(190)
        Layout.maximumWidth: root.width * 0.3
        Layout.fillHeight: true
        spacing: Style.space(4)
        Text {
          Layout.fillWidth: true
          text: root.service ? (root.service.relay ? root.service.relay.replace(/^wss?:\/\//, "").replace(/\/$/, "") : root.service.viewModel.community) : ""
          textFormat: Text.PlainText
          elide: Text.ElideMiddle
          color: Color.foreground
          opacity: 0.6
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
          Controls.ToolTip.visible: relayHover.containsMouse && text !== ""
          Controls.ToolTip.text: root.service ? (root.service.relay || root.service.viewModel.community) + " · " + root.service.catalogLabel : ""
          MouseArea {
            id: relayHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }
        }
        Text {
          Layout.fillWidth: true
          text: root.service ? root.service.catalogLabel : "Service unavailable"
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: Color.foreground
          opacity: 0.6
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
        Controls.ScrollView {
          Layout.fillWidth: true
          Layout.fillHeight: true
          contentWidth: availableWidth
          clip: true
          ColumnLayout {
            width: parent.width
            spacing: Style.space(2)
            Text {
              visible: root.service && root.service.streamRooms.length > 0
              Layout.fillWidth: true
              text: "Rooms"
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              opacity: 0.6
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Repeater {
              model: root.service ? root.service.streamRooms : []
              delegate: Ui.Button {
                required property var modelData
                Layout.fillWidth: true
                clip: true
                text: "# " + modelData.name + (root.service && root.service.roomActivityCount(modelData.id) > 0 ? " · " + root.service.roomActivityCount(modelData.id) + "+" : "")
                tooltipText: modelData.name + (root.service && root.service.roomActivityCount(modelData.id) > 0 ? " · new activity seen on this device, not synced unread" : "")
                leftAlign: true
                focusable: true
                selected: root.service && root.service.selectedRoomId === modelData.id
                onClicked: root.service.selectRoom(modelData.id)
              }
            }
            Text {
              visible: root.service && root.service.dmRooms.length > 0
              Layout.fillWidth: true
              text: "Direct messages"
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              opacity: 0.6
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Repeater {
              model: root.service ? root.service.dmRooms : []
              delegate: Ui.Button {
                required property var modelData
                Layout.fillWidth: true
                clip: true
                text: modelData.name + (root.service && root.service.roomActivityCount(modelData.id) > 0 ? " · " + root.service.roomActivityCount(modelData.id) + "+" : "")
                tooltipText: modelData.name + (root.service && root.service.roomActivityCount(modelData.id) > 0 ? " · new activity seen on this device, not synced unread" : "")
                leftAlign: true
                focusable: true
                selected: root.service && root.service.selectedRoomId === modelData.id
                onClicked: root.service.selectRoom(modelData.id)
              }
            }
          }
        }
        ColumnLayout {
          visible: root.service && root.service.agentProfiles.length > 0
          Layout.fillWidth: true
          spacing: Style.space(2)
          Text {
            Layout.fillWidth: true
            text: "Agents here · activity unknown"
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Color.foreground
            opacity: 0.6
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
          Repeater {
            model: root.service ? root.service.agentProfiles.slice(0, 4) : []
            delegate: Text {
              required property var modelData
              Layout.fillWidth: true
              text: modelData.name || modelData.key.slice(0, 12) + "…"
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
          }
        }
      }

      Rectangle {
        visible: root.showRooms
        Layout.fillHeight: true
        implicitWidth: 1
        color: Util.alpha(Color.foreground, 0.14)
      }

      ColumnLayout {
        visible: root.showTimeline
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        spacing: Style.space(8)
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.space(8)
          Text {
            text: root.service && root.service.selectedRoom ? root.service.roomTitle(root.service.selectedRoom) : "Connect Buzz"
            textFormat: Text.PlainText
            elide: Text.ElideRight
            Layout.maximumWidth: Style.space(260)
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body * 1.15
            font.bold: true
          }
          Text {
            Layout.fillWidth: true
            visible: root.connected && !!root.service.selectedRoom
            text: root.service ? (root.service.sampleMode && root.service.selectedRoom ? root.service.selectedRoom.description : root.service.historyLabel) : ""
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Color.foreground
            opacity: 0.6
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            Controls.ToolTip.visible: historyHover.containsMouse && truncated
            Controls.ToolTip.text: text
            MouseArea {
              id: historyHover
              anchors.fill: parent
              hoverEnabled: true
              acceptedButtons: Qt.NoButton
            }
          }
          Item { Layout.fillWidth: true; visible: !root.connected || !root.service.selectedRoom }
          Ui.Button {
            objectName: "buzzRefreshRoom"
            text: "↻"
            tooltipText: "Refresh this room"
            horizontalPadding: Style.space(6)
            verticalPadding: Style.space(2)
            focusable: true
            visible: root.service && !root.service.sampleMode && root.service.connection === "authenticated" && root.service.historySupported && root.service.selectedRoom !== null
            onClicked: if (root.service) { root.service.refreshHistory(); root.service.refreshRecipients() }
          }
        }
        ColumnLayout {
          visible: !root.connected
          Layout.fillWidth: true
          spacing: Style.space(8)
          RowLayout {
            spacing: Style.space(8)
            Ui.Button {
              text: "Buzz hosted"
              selected: root.service && root.service.setupProvider === "hosted"
              focusable: true
              onClicked: if (root.service) root.service.chooseSetupProvider("hosted")
            }
            Ui.Button {
              text: "Custom relay"
              selected: root.service && root.service.setupProvider === "custom"
              focusable: true
              onClicked: if (root.service) root.service.chooseSetupProvider("custom")
            }
          }
          Ui.Button {
            visible: root.service && root.service.setupProvider === "hosted"
            text: "Open Buzz hosted setup"
            focusable: true
            // Fixed upstream URL, opened only by this explicit user action.
            onClicked: Qt.openUrlExternally("https://buzz.xyz")
          }
          Text {
            Layout.fillWidth: true
            text: root.service ? root.service.setupInstructions : "Enable the plugin and reopen this panel."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Color.foreground
            opacity: 0.7
            font.family: Style.font.family
            font.pixelSize: Style.font.body
          }
          Ui.Button {
            text: "Retry connection"
            focusable: true
            visible: !!root.service
            onClicked: if (root.service) root.service.retry()
          }
        }
        BuzzScroll {
          id: historyScroll
          objectName: "buzzHistoryScroll"
          Layout.fillWidth: true
          Layout.fillHeight: true
          // The viewport takes the space left over; its content never sizes the panel.
          Layout.preferredHeight: Style.space(80)
          contentHeight: historyList.height
          restOnEnd: true
          Column {
            id: historyList
            width: parent.width
            height: childrenRect.height
            Repeater {
              model: messageModel
              delegate: BuzzMessage {
                required property string payload
                width: historyList.width
                topPadding: row.grouped ? Style.space(3) : Style.space(10)
                service: root.service
                row: JSON.parse(payload)
                threadLink: true
                threadSelected: !!root.service && root.service.threadRootId === row.id
                onThreadRequested: root.toggleThread(row.id)
              }
            }
          }
        }
        Text {
          Layout.fillWidth: true
          visible: root.connected && !!root.service && !root.service.sampleMode && root.service.selectedRoom !== null
            && root.service.historyState === "snapshot" && messageModel.count === 0
          text: "No messages in this recent snapshot."
          textFormat: Text.PlainText
          color: Color.foreground
          opacity: 0.6
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
        BuzzComposer {
          id: roomComposer
          Layout.fillWidth: true
          visible: !!root.service && !root.service.sampleMode && root.service.selectedRoom !== null
            && root.service.connection === "authenticated" && (root.service.sendSupported || root.service.deliveryState === "unknown")
          service: root.service
          placeholder: root.service && root.service.selectedRoom ? "Message #" + root.service.selectedRoom.name : ""
          showDelivery: !threadComposer.showDelivery
        }
        Text {
          visible: !!root.service && root.service.sampleMode
          Layout.fillWidth: true
          text: "Messaging is not connected. This preview cannot send messages or start agents."
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          opacity: 0.7
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
      }

      Rectangle {
        visible: root.threadOpen && root.showTimeline
        Layout.fillHeight: true
        implicitWidth: 1
        color: Util.alpha(Color.foreground, 0.14)
      }

      ColumnLayout {
        id: threadPanel
        objectName: "buzzThreadPanel"
        visible: root.threadOpen
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(340)
        spacing: Style.space(8)
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.space(8)
          Ui.Button {
            visible: !root.showTimeline
            text: "‹"
            tooltipText: "Back to the room"
            horizontalPadding: Style.space(6)
            verticalPadding: Style.space(2)
            focusable: true
            onClicked: root.closeThread()
          }
          Text {
            text: "Thread"
            textFormat: Text.PlainText
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body * 1.15
            font.bold: true
          }
          Text {
            Layout.fillWidth: true
            text: root.service && root.service.selectedRoom ? root.service.roomTitle(root.service.selectedRoom) : ""
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Color.foreground
            opacity: 0.6
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
          Ui.Button {
            objectName: "buzzRefreshThread"
            text: "↻"
            tooltipText: "Refresh replies"
            horizontalPadding: Style.space(6)
            verticalPadding: Style.space(2)
            focusable: true
            enabled: !!root.service && root.service.threadState !== "loading"
            opacity: enabled ? 1 : 0.5
            onClicked: root.service.refreshThread()
          }
          Ui.Button {
            objectName: "buzzCloseThread"
            text: "✕"
            tooltipText: "Close thread"
            horizontalPadding: Style.space(6)
            verticalPadding: Style.space(2)
            focusable: true
            onClicked: root.closeThread()
          }
        }
        BuzzScroll {
          id: threadScroll
          objectName: "buzzThreadScroll"
          Layout.fillWidth: true
          Layout.fillHeight: true
          Layout.preferredHeight: Style.space(80)
          contentHeight: threadList.childrenRect.height
          Column {
            id: threadList
            width: parent.width
            height: childrenRect.height
            BuzzMessage {
              objectName: "buzzThreadRoot"
              width: threadList.width
              service: root.service
              row: root.threadOpen ? root.service.threadRoot : ({})
              showDate: true
            }
            Item {
              objectName: "buzzThreadDetails"
              width: threadList.width
              height: Style.space(26)
              Text {
                id: replyCount
                anchors.verticalCenter: parent.verticalCenter
                text: root.service ? root.service.threadCountLabel : ""
                textFormat: Text.PlainText
                color: Color.foreground
                opacity: 0.7
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
              }
              Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: replyCount.right
                anchors.leftMargin: Style.space(8)
                anchors.right: parent.right
                height: 1
                color: Util.alpha(Color.foreground, 0.14)
              }
            }
            Repeater {
              model: replyModel
              delegate: BuzzMessage {
                required property string payload
                objectName: "buzzThreadReply"
                width: threadList.width
                topPadding: row.grouped ? Style.space(3) : Style.space(8)
                service: root.service
                row: JSON.parse(payload)
              }
            }
          }
        }
        BuzzComposer {
          id: threadComposer
          Layout.fillWidth: true
          visible: !!root.service && root.service.threadSendSupported
          service: root.service
          rootId: root.threadOpen ? root.service.threadRootId : ""
          fieldName: "buzzThreadComposer"
          placeholder: "Reply…"
          showDelivery: root.threadOpen && root.service.deliveryState !== "idle"
            && root.service.submissionDraftKey === root.service.selectedRoomId + ":" + root.service.threadRootId
          onEscaped: root.closeThread()
        }
      }
    }
  }
}
