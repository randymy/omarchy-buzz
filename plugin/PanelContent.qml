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
  property var pendingRevealRow: null
  property var pendingRevealDetails: null
  property int mentionIndex: 0
  property bool mentionDismissed: false
  readonly property var mentionQuery: {
    if (!service || !composer.visible || composer.readOnly || service.recipientsState !== "snapshot"
        || service.recipientsRoomId !== service.selectedRoomId || service.recipientPickerLocked) return null
    var before = composer.text.slice(0, composer.cursorPosition)
    var match = /(^|[\s([{,;:])@([^\s@]*)$/.exec(before)
    if (!match) return null
    return {start: before.length - match[2].length - 1, end: before.length, prefix: match[2].toLocaleLowerCase()}
  }
  readonly property var mentionMatches: {
    if (!mentionQuery || !service) return []
    var prefix = mentionQuery.prefix
    return service.recipientEntries.filter(function(entry) {
      if (!entry.name || !entry.name.trim()) return false
      return (entry.name || "").toLocaleLowerCase().indexOf(prefix) === 0
        || entry.key.toLowerCase().indexOf(prefix) === 0
    }).slice(0, 8)
  }
  readonly property bool mentionOpen: !mentionDismissed && mentionMatches.length > 0
  onMentionMatchesChanged: mentionIndex = 0
  function chooseMention(index) {
    if (!mentionOpen || !mentionQuery || index < 0 || index >= mentionMatches.length || !service) return false
    var query = mentionQuery
    var nextCursor = service.insertMention(mentionMatches[index].key, query.start, query.end)
    if (nextCursor < 0) return false
    if (composer.text !== service.draftText) composer.text = service.draftText
    composer.cursorPosition = nextCursor
    composer.forceActiveFocus()
    return true
  }
  function revealThread(row, details) {
    pendingRevealRow = row
    pendingRevealDetails = details
    revealThreadTimer.restart()
  }
  Timer {
    id: revealThreadTimer
    interval: 50
    onTriggered: {
      var row = root.pendingRevealRow
      var details = root.pendingRevealDetails
      root.pendingRevealRow = null
      root.pendingRevealDetails = null
      if (!row || !details || !service || service.threadRootId !== row.modelData.id || !details.visible) return
      var viewport = historyScroll.contentItem
      if (!viewport || typeof viewport.contentY === "undefined") return
      var top = row.y + details.y - Style.space(12)
      viewport.contentY = Math.max(0, Math.min(top, viewport.contentHeight - historyScroll.availableHeight))
    }
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
          id: historyScroll
          objectName: "buzzHistoryScroll"
          Layout.fillWidth: true
          Layout.fillHeight: true
          clip: true
          contentWidth: availableWidth
          contentHeight: historyList.childrenRect.height
          Column {
            id: historyList
            width: parent.width
            height: childrenRect.height
            spacing: Style.space(root.service && root.service.sampleMode ? 22 : 10)
            Repeater {
              model: root.service ? root.service.messages : []
              delegate: Column {
                id: messageRow
                required property var modelData
                width: parent.width
                height: childrenRect.height
                spacing: Style.space(6)
                Text {
                  width: parent.width
                  text: root.service && root.service.sampleMode ? modelData.author + " · " + modelData.role + " · " + modelData.time
                    : root.service.messageAuthorLabel(modelData.author) + " · " + root.service.formatTimestamp(modelData.time)
                  textFormat: Text.PlainText
                  wrapMode: Text.WordWrap
                  color: Color.accent
                  font.family: Style.font.family
                  font.pixelSize: Style.font.body
                  font.bold: true
                  Controls.ToolTip.visible: messageAuthorHover.containsMouse && !root.service.sampleMode
                  Controls.ToolTip.text: messageRow.modelData.author
                  MouseArea {
                    id: messageAuthorHover
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.NoButton
                  }
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
                Row {
                  objectName: "buzzMessageReactions"
                  visible: !!modelData.reactions && (modelData.reactions.seen > 0 || modelData.reactions.working > 0)
                  spacing: Style.space(10)
                  Text {
                    visible: !!modelData.reactions && modelData.reactions.seen > 0
                    text: "👀 " + (modelData.reactions ? modelData.reactions.seen : 0)
                    textFormat: Text.PlainText
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                    Controls.ToolTip.visible: seenReactionHover.containsMouse
                    Controls.ToolTip.text: "Queued reaction (snapshot)"
                    MouseArea {
                      id: seenReactionHover
                      anchors.fill: parent
                      hoverEnabled: true
                      acceptedButtons: Qt.NoButton
                    }
                  }
                  Text {
                    visible: !!modelData.reactions && modelData.reactions.working > 0
                    text: "💬 " + (modelData.reactions ? modelData.reactions.working : 0)
                    textFormat: Text.PlainText
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                    Controls.ToolTip.visible: workingReactionHover.containsMouse
                    Controls.ToolTip.text: "Working reaction (snapshot)"
                    MouseArea {
                      id: workingReactionHover
                      anchors.fill: parent
                      hoverEnabled: true
                      acceptedButtons: Qt.NoButton
                    }
                  }
                }
                Ui.Button {
                  objectName: "buzzThreadToggle"
                  focusable: true
                  visible: root.service && root.service.canOpenThread(messageRow.modelData.id)
                  text: root.service && root.service.threadRootId === messageRow.modelData.id ? "💬 Hide replies" : "💬 View replies"
                  onClicked: {
                    if (root.service.threadRootId === messageRow.modelData.id) root.service.closeThread()
                    else {
                      root.service.openThread(messageRow.modelData.id)
                      root.revealThread(messageRow, threadDetails)
                    }
                  }
                }
                Rectangle {
                  id: threadDetails
                  objectName: "buzzThreadDetails"
                  visible: root.service && root.service.threadRootId === messageRow.modelData.id
                  width: parent.width
                  implicitHeight: threadBody.height + Style.space(16)
                  color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.07)
                  border.color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.25)
                  radius: Style.cornerRadius
                  Column {
                    id: threadBody
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: Style.space(8)
                    height: childrenRect.height
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
                    Ui.Button {
                      text: "Reply in thread"
                      focusable: true
                      visible: root.service && root.service.threadSendSupported
                      enabled: root.service && !root.service.recipientPickerLocked && root.service.canReplyTo(messageRow.modelData.id)
                      onClicked: if (root.service.composeReply(messageRow.modelData.id)) composer.forceActiveFocus()
                    }
                    Repeater {
                      model: threadDetails.visible && root.service ? root.service.threadRows : []
                      delegate: Column {
                        required property var modelData
                        width: parent.width
                        height: childrenRect.height
                        spacing: Style.space(4)
                        Text {
                          width: parent.width
                          text: root.service.messageAuthorLabel(modelData.author) + " · " + root.service.formatTimestamp(modelData.time)
                          Controls.ToolTip.visible: replyAuthorHover.containsMouse
                          Controls.ToolTip.text: modelData.author
                          MouseArea {
                            id: replyAuthorHover
                            anchors.fill: parent
                            hoverEnabled: true
                            acceptedButtons: Qt.NoButton
                          }
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
                    Ui.Button {
                      focusable: true
                      text: "Refresh replies"
                      enabled: root.service && root.service.threadState !== "loading"
                      onClicked: root.service.refreshThread()
                    }
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
          RowLayout {
            Layout.fillWidth: true
            Text {
              Layout.fillWidth: true
              text: root.service ? root.service.composerLabel : ""
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: Color.accent
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Ui.Button {
              text: "Back to room"
              focusable: true
              visible: root.service && root.service.replyRootId !== ""
              enabled: root.service && !root.service.recipientPickerLocked
              onClicked: root.service.composeRoom()
            }
          }
          ColumnLayout {
            id: mentionSuggestions
            objectName: "buzzMentionSuggestions"
            Layout.fillWidth: true
            visible: root.mentionOpen
            spacing: Style.space(2)
            Text {
              text: "Mention someone in this room"
              textFormat: Text.PlainText
              color: Color.foreground
              opacity: 0.7
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Repeater {
              model: root.mentionOpen ? root.mentionMatches : []
              delegate: Ui.Button {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                objectName: "buzzMentionOption"
                text: "@" + (modelData.name || "Unnamed") + " · " + modelData.key.slice(0, 12) + "…" + modelData.key.slice(-8)
                  + " · " + (root.service ? root.service.participantLabel(modelData.key) : "")
                tooltipText: modelData.key
                leftAlign: true
                focusable: false
                selected: root.mentionIndex === index
                onClicked: root.chooseMention(index)
              }
            }
          }
          Controls.TextArea {
            id: composer
            objectName: "buzzComposer"
            Layout.fillWidth: true
            Layout.preferredHeight: Style.space(70)
            visible: root.service && root.service.selectedRoom !== null && root.service.connection === "authenticated"
            placeholderText: "Plain text · type @ to mention someone in this room"
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
              root.mentionDismissed = false
            }
            onCursorPositionChanged: root.mentionDismissed = false
            Keys.onPressed: function(event) {
              if (!root.mentionOpen) return
              if (event.key === Qt.Key_Down) {
                root.mentionIndex = (root.mentionIndex + 1) % root.mentionMatches.length
                event.accepted = true
              } else if (event.key === Qt.Key_Up) {
                root.mentionIndex = (root.mentionIndex + root.mentionMatches.length - 1) % root.mentionMatches.length
                event.accepted = true
              } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                event.accepted = root.chooseMention(root.mentionIndex)
              } else if (event.key === Qt.Key_Escape) {
                root.mentionDismissed = true
                event.accepted = true
              }
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
              text: root.service && root.service.replyRootId ? "Send reply" : "Send message"
              focusable: true
              enabled: root.service && root.service.canSend
              onClicked: if (root.service) root.service.submitDraft()
            }
            Ui.Button {
              text: !root.service ? "" : root.service.deliveryState === "unknown"
                ? "Discard uncertain draft" + (root.service.deliveryScopeMismatch ? " from " + root.service.submissionScopeLabel : "")
                : "Start new submission" + (root.service.deliveryScopeMismatch ? " for " + root.service.submissionScopeLabel : "")
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
