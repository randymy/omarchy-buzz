import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons

FocusScope {
  id: root
  property var service: null
  property bool recipientPickerExpanded: false
  property bool presentationSwitchEnabled: false
  property bool windowMode: false
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
    anchors.margins: Style.space(22)
    spacing: Style.space(16)

    RowLayout {
      Layout.fillWidth: true
      ColumnLayout {
        spacing: Style.space(4)
        Text {
          text: "Buzz"
          textFormat: Text.PlainText
          color: Color.foreground
          font.family: Style.font.family
          font.pixelSize: Style.font.body * 1.7
          font.bold: true
        }
        Text {
          text: "People and agents, together"
          textFormat: Text.PlainText
          color: Color.foreground
          opacity: 0.7
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }
      }
      Item { Layout.fillWidth: true }
      Ui.Button {
        visible: root.presentationSwitchEnabled
        text: root.windowMode ? "Overlay" : "Window"
        focusable: true
        onClicked: root.presentationRequested()
      }
      Ui.Button {
        text: root.service && root.service.notificationsEnabled ? "Alerts: on" : "Alerts: off"
        focusable: true
        onClicked: if (root.service) root.service.notificationsEnabled = !root.service.notificationsEnabled
      }
      Ui.Button {
        text: "Close · Esc"
        focusable: true
        onClicked: root.closeRequested()
      }
    }

    Rectangle {
      Layout.fillWidth: true
      implicitHeight: previewLabel.implicitHeight + Style.space(20)
      color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.1)
      radius: Style.cornerRadius
      Text {
        id: previewLabel
        anchors.fill: parent
        anchors.margins: Style.space(10)
        text: root.service && root.service.sampleMode ? "TEST FIXTURE · Sample data only\nNo relay connected. Messages below are examples." : (root.service && root.service.sendSupported ? "Messaging preview · " : "Read-only preview · ") + (root.service ? root.service.statusLabel : "Service unavailable")
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
      spacing: Style.space(22)

      ColumnLayout {
        Layout.preferredWidth: Style.space(170)
        Layout.maximumWidth: root.width * 0.3
        Layout.fillHeight: true
        spacing: Style.space(6)
        Text {
          Layout.fillWidth: true
          text: root.service ? (root.service.relay || root.service.viewModel.community) + "\n" + root.service.catalogLabel : "Service unavailable"
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          opacity: 0.7
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }
        ColumnLayout {
          visible: root.service && root.service.agentProfiles.length > 0
          Layout.fillWidth: true
          Text {
            text: "Agents in this room · execution unknown"
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
          Controls.ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(Style.space(80), agentList.implicitHeight)
            contentWidth: availableWidth
            clip: true
            Column {
              id: agentList
              width: parent.width
              Repeater {
                model: root.service ? root.service.agentProfiles : []
                delegate: Text {
                  required property var modelData
                  width: agentList.width
                  text: (modelData.name || "Agent") + " · " + modelData.key.slice(0, 12) + "…"
                  textFormat: Text.PlainText
                  elide: Text.ElideRight
                  color: Color.foreground
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                }
              }
            }
          }
        }
        Controls.ScrollView {
          Layout.fillWidth: true
          Layout.fillHeight: true
          contentWidth: availableWidth
          clip: true
          ColumnLayout {
            width: parent.width
            spacing: Style.space(6)
            Repeater {
              model: root.service ? root.service.rooms : []
              delegate: Ui.Button {
                required property var modelData
                Layout.fillWidth: true
                text: "# " + modelData.name + (root.service && root.service.roomActivityCount(modelData.id) > 0 ? " · " + root.service.roomActivityCount(modelData.id) + "+" : "")
                leftAlign: true
                focusable: true
                selected: root.service && root.service.selectedRoomId === modelData.id
                onClicked: root.service.selectRoom(modelData.id)
              }
            }
          }
        }
        Text {
          Layout.fillWidth: true
          text: "Connection\n" + (root.service ? root.service.statusLabel : "Unavailable")
            + "\nActivity is local, sampled, and not synced unread state."
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          opacity: 0.7
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }
      }

      Rectangle {
        Layout.fillHeight: true
        implicitWidth: 1
        color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.14)
      }

      ColumnLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: Style.space(10)
        Text {
          Layout.fillWidth: true
          text: root.service && root.service.selectedRoom ? "# " + root.service.selectedRoom.name : "Connect Buzz"
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: Color.foreground
          font.family: Style.font.family
          font.pixelSize: Style.font.body * 1.2
          font.bold: true
        }
        RowLayout {
          visible: root.service && !root.service.sampleMode && root.service.connection !== "authenticated"
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
          visible: root.service && !root.service.sampleMode && root.service.setupProvider === "hosted" && root.service.connection !== "authenticated"
          text: "Open Buzz hosted setup"
          focusable: true
          // Fixed upstream URL, opened only by this explicit user action.
          onClicked: Qt.openUrlExternally("https://buzz.xyz")
        }
        Text {
          Layout.fillWidth: true
          text: root.service && root.service.selectedRoom ? (root.service.sampleMode ? root.service.selectedRoom.description : root.service.historyLabel) : (root.service ? root.service.setupInstructions : "Enable the plugin and reopen this panel.")
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          opacity: 0.7
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }

        RowLayout {
          visible: root.service && !root.service.sampleMode
            && (root.service.connection !== "authenticated" || (root.service.historySupported && root.service.selectedRoom !== null))
          spacing: Style.space(8)
          Ui.Button {
            text: "Retry connection"
            focusable: true
            visible: root.service && root.service.connection !== "authenticated"
            onClicked: if (root.service) root.service.retry()
          }
          Ui.Button {
            text: "Refresh room"
            focusable: true
            visible: root.service && root.service.connection === "authenticated" && root.service.historySupported && root.service.selectedRoom !== null
            onClicked: if (root.service) { root.service.refreshHistory(); root.service.refreshRecipients() }
          }
        }
        ColumnLayout {
          Layout.fillWidth: true
          visible: root.service && !root.service.sampleMode && (root.service.recipientsSupported || root.service.selectedRecipients.length > 0) && root.service.selectedRoom !== null
          spacing: Style.space(4)
          Ui.Button {
            text: root.service ? root.service.recipientsLabel + " · " + root.service.selectedRecipients.length + " selected" + (root.recipientPickerExpanded ? " · Hide" : " · Choose") : ""
            focusable: true
            onClicked: root.recipientPickerExpanded = !root.recipientPickerExpanded
          }
          Controls.ScrollView {
            visible: root.recipientPickerExpanded
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(Style.space(72), people.implicitHeight)
            clip: true
            contentWidth: availableWidth
            Column {
              id: people
              width: parent.width
              Repeater {
                model: root.service ? root.service.recipientEntries : []
                delegate: Controls.CheckBox {
                  required property var modelData
                  width: people.width
                  text: (modelData.name || "Unnamed") + " · " + modelData.key.slice(0, 12) + "…" + modelData.key.slice(-8) + " · " + root.service.participantLabel(modelData.key)
                  hoverEnabled: true
                  Controls.ToolTip.visible: hovered
                  Controls.ToolTip.text: modelData.key
                  contentItem: Text {
                    text: parent.text
                    textFormat: Text.PlainText
                    leftPadding: parent.indicator ? parent.indicator.width + Style.space(6) : 0
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                    elide: Text.ElideRight
                  }
                  checked: root.service && root.service.selectedRecipients.indexOf(modelData.key) !== -1
                  enabled: root.service && !root.service.recipientPickerLocked && root.service.recipientsState === "snapshot"
                  onClicked: root.service.toggleRecipient(modelData.key)
                }
              }
            }
          }
          Controls.ScrollView {
            visible: root.service && root.service.unavailableRecipients.length > 0
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(Style.space(72), missingPeople.implicitHeight)
            clip: true
            contentWidth: availableWidth
            Column {
              id: missingPeople
              width: parent.width
              Repeater {
                model: root.service ? root.service.unavailableRecipients : []
                delegate: RowLayout {
                  required property string modelData
                  width: missingPeople.width
                  Text {
                    Layout.fillWidth: true
                    text: "Selected " + modelData.slice(0, 12) + "…" + modelData.slice(-8) + " · unavailable in current roster"
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                  }
                  Ui.Button {
                    text: "Deselect"
                    focusable: true
                    enabled: root.service && !root.service.recipientPickerLocked
                    onClicked: root.service.toggleRecipient(modelData)
                  }
                }
              }
            }
          }
          Text {
            Layout.fillWidth: true
            visible: root.recipientPickerExpanded
            text: "Names self-asserted. Select recipients to create exact mentions; agent execution configured separately."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Color.foreground
            opacity: 0.7
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }
        Controls.ScrollView {
          Layout.fillWidth: true
          Layout.fillHeight: true
          clip: true
          contentWidth: availableWidth
          Column {
            width: parent.width
            spacing: Style.space(root.service && root.service.sampleMode ? 22 : 10)
            Repeater {
              model: root.service ? root.service.messages : []
              delegate: Column {
                id: messageRow
                required property var modelData
                width: parent.width
                spacing: Style.space(6)
                Text {
                  width: parent.width
                  text: root.service && root.service.sampleMode ? modelData.author + " · " + modelData.role + " · " + modelData.time
                    : modelData.author.slice(0, 12) + "… · " + root.service.participantLabel(modelData.author) + " · " + root.service.formatTimestamp(modelData.time)
                  textFormat: Text.PlainText
                  wrapMode: Text.WordWrap
                  color: Color.accent
                  font.family: Style.font.family
                  font.pixelSize: Style.font.body
                  font.bold: true
                }
                Text {
                  width: parent.width
                  text: root.service && root.service.sampleMode ? modelData.text : (modelData.unavailable ? "Content unavailable" : modelData.text)
                    + (modelData.edited ? "\n[edited]" : "") + (modelData.truncated ? "\n[truncated]" : "")
                  textFormat: Text.PlainText
                  wrapMode: Text.WordWrap
                  color: Color.foreground
                  font.family: Style.font.family
                  font.pixelSize: Style.font.body
                }
                Controls.Button {
                  visible: root.service && root.service.canOpenThread(messageRow.modelData.id)
                  text: root.service && root.service.threadRootId === messageRow.modelData.id ? "Hide replies" : "View replies"
                  onClicked: {
                    if (root.service.threadRootId === messageRow.modelData.id) root.service.closeThread()
                    else root.service.openThread(messageRow.modelData.id)
                  }
                }
                Column {
                  visible: root.service && root.service.threadRootId === messageRow.modelData.id
                  width: parent.width
                  spacing: Style.space(8)
                  Text {
                    width: parent.width
                    text: root.service ? root.service.threadLabel : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                  }
                  Repeater {
                    model: parent.visible && root.service ? root.service.threadRows : []
                    delegate: Column {
                      required property var modelData
                      width: parent.width
                      spacing: Style.space(4)
                      Text {
                        width: parent.width
                        text: modelData.author.slice(0, 12) + "… · " + root.service.participantLabel(modelData.author) + " · " + root.service.formatTimestamp(modelData.time)
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                        color: Color.accent
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                      }
                      Text {
                        width: parent.width
                        text: modelData.unavailable ? "Content unavailable" : modelData.text + (modelData.edited ? "\n[edited]" : "") + (modelData.truncated ? "\n[truncated]" : "")
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                        color: Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.body
                      }
                    }
                  }
                  Controls.Button {
                    text: "Refresh replies"
                    enabled: root.service && root.service.threadState !== "loading"
                    onClicked: root.service.refreshThread()
                  }
                }
              }
            }
          }
        }

        ColumnLayout {
          Layout.fillWidth: true
          visible: root.service && !root.service.sampleMode && (root.service.sendSupported || root.service.deliveryState === "unknown")
          spacing: Style.space(6)
          Text {
            Layout.fillWidth: true
            visible: root.service && root.service.deliveryState !== "idle"
            text: root.service ? root.service.deliveryLabel : ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body
          }
          Controls.TextArea {
            id: composer
            Layout.fillWidth: true
            Layout.preferredHeight: Style.space(70)
            visible: root.service && root.service.selectedRoom !== null && root.service.connection === "authenticated"
            placeholderText: "Plain text · choose exact recipients above"
            textFormat: TextEdit.PlainText
            wrapMode: TextEdit.Wrap
            text: root.service ? root.service.draftText : ""
            readOnly: !root.service || root.service.deliveryState === "sending" || root.service.deliveryState === "unknown" || root.service.deliveryState === "rejected" || root.service.deliveryCategory === "send_request_reused"
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            background: Rectangle { color: Color.popups.background; border.color: Color.popups.border; radius: Style.cornerRadius }
            onTextChanged: {
              if (text.length > 4096) text = text.slice(0, 4096)
              if (root.service) root.service.updateDraft(text)
            }
          }
          Connections {
            target: root.service
            function onDraftTextChanged() {
              if (composer.text !== root.service.draftText) composer.text = root.service.draftText
            }
          }
          RowLayout {
            visible: root.service && root.service.selectedRoom !== null && root.service.connection === "authenticated"
            Ui.Button {
              text: "Send message"
              focusable: true
              enabled: root.service && root.service.canSend
              onClicked: if (root.service) root.service.submitDraft()
            }
            Ui.Button {
              text: root.service && (root.service.deliveryState === "rejected" || root.service.deliveryCategory === "send_request_reused") ? "Start new submission" : "Start new draft"
              focusable: true
              visible: root.service && (root.service.deliveryState === "unknown" || root.service.deliveryState === "rejected" || root.service.deliveryCategory === "send_request_reused")
              onClicked: if (root.service) root.service.newDraft(root.service.deliveryState === "rejected" || root.service.deliveryCategory === "send_request_reused")
            }
            Text {
              visible: root.service && root.service.deliveryState !== "unknown"
              text: "4096 bytes maximum"
              color: Color.foreground
              opacity: 0.7
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
          }
        }

        Rectangle {
          visible: root.service && root.service.sampleMode
          Layout.fillWidth: true
          implicitHeight: composerLabel.implicitHeight + Style.space(22)
          color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.04)
          border.color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.2)
          radius: Style.cornerRadius
          Text {
            id: composerLabel
            anchors.fill: parent
            anchors.margins: Style.space(11)
            text: "Messaging is not connected yet.\nThis preview cannot send messages or start agents."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Color.foreground
            opacity: 0.7
            font.family: Style.font.family
            font.pixelSize: Style.font.body
          }
        }
      }
    }
  }
}
