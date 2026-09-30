import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons
import "AnsiArt.js" as AnsiArt

FocusScope {
  id: root
  property var service: null
  // This helper's public key (never a secret): short form, full key on hover, copy.
  component PublicKeyRow: RowLayout {
    id: keyRow
    property var service: null
    Layout.fillWidth: true
    spacing: Style.space(8)
    Text {
      objectName: "buzzPublicKey"
      Layout.fillWidth: true
      text: keyRow.service ? "Public key " + keyRow.service.shortPublicKey : ""
      textFormat: Text.PlainText
      elide: Text.ElideRight
      color: Color.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.body
      Controls.ToolTip.visible: keyHover.containsMouse
      Controls.ToolTip.text: keyRow.service ? keyRow.service.identity : ""
      MouseArea { id: keyHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
    }
    Ui.Button {
      objectName: "buzzCopyPublicKey"
      text: keyRow.service && keyRow.service.publicKeyCopied ? "Copied" : "Copy public key"
      tooltipText: "Copy the public key to share it with a community owner"
      focusable: true
      onClicked: if (keyRow.service) keyRow.service.copyPublicKey()
    }
  }
  property bool presentationSwitchEnabled: false
  property bool windowMode: false
  property alias recipientPickerExpanded: roomComposer.pickerExpanded
  readonly property bool connected: !!service && (service.sampleMode || service.connection === "authenticated")
  // Agents (docs/AGENTS_SERVICE.md): shown only while the agent service is
  // connected and offers `agent_manager`. The editor replaces the room view;
  // the room, its drafts and any open thread come back unchanged.
  readonly property var agentService: service ? service.agents : null
  readonly property bool agentsVisible: !!agentService && agentService.available
  readonly property bool agentsUnavailableShown: !!service && !service.sampleMode && service.connection === "authenticated"
    && !!agentService && !agentsVisible && agentService.connection !== "connecting"
  property bool agentEditorOpen: false
  property string agentEditorId: ""
  readonly property bool agentEditorShown: agentEditorOpen && agentsVisible
  onAgentsVisibleChanged: if (!agentsVisible) closeAgentEditor()
  function openAgentEditor(id) {
    if (!agentsVisible || (id !== "" && !agentService.agent(id))) return false
    agentEditorId = id
    agentEditorOpen = true
    agentEditor.load()
    return true
  }
  function closeAgentEditor() {
    agentEditorOpen = false
    agentEditorId = ""
  }
  // Sidebar rows use a smaller avatar so each stays close to one button high.
  readonly property int sidebarAvatarSize: Math.max(6, Math.round(Style.font.caption * 0.8))
  // This user's own avatar art, keyed by their public key in the local avatar
  // store. Local only until profile avatars are published to the relay.
  readonly property bool myAvatarAvailable: !!service && !service.sampleMode && !!agentService
    && /^[a-f0-9]{64}$/.test(service.identity)
  readonly property string myAvatarArt: myAvatarAvailable ? agentService.avatarArtForKey(service.identity) : ""
  readonly property real myAvatarBrightness: myAvatarAvailable ? agentService.avatarBrightnessForKey(service.identity) : 0
  function openAvatarCard(key, name, art, brightness) {
    avatarCard.show(key, name, art, brightness)
    return true
  }
  // A direct message shows the first participant other than this identity.
  function dmPartner(room) {
    if (!room || !Array.isArray(room.participants) || !service) return ""
    var others = room.participants.filter(function(key) { return key !== root.service.identity })
    return others.length ? others[0] : ""
  }
  readonly property bool threadOpen: !agentEditorShown && !!service && service.threadRootId !== "" && service.threadRoot !== null
  // An open thread sits beside the room. Narrow windows give up the room list
  // first, then the room itself, so the thread always has a readable column.
  readonly property bool showRooms: !threadOpen || width >= Style.space(1100)
  readonly property bool showTimeline: !threadOpen || width >= Style.space(640)
  readonly property var activeComposer: threadOpen && service.replyRootId !== "" ? threadComposer : roomComposer
  readonly property var mentionMatches: activeComposer.mentionMatches
  readonly property bool mentionOpen: activeComposer.mentionOpen
  function chooseMention(index) { return activeComposer.chooseMention(index) }
  // New direct message: a compact picker over the open room's verified members.
  readonly property bool newDmAvailable: !!service && service.dmOpenAvailable
  property bool newDmOpen: false
  onNewDmAvailableChanged: if (!newDmAvailable) newDmOpen = false
  Connections {
    target: root.service
    function onDmOpenStateChanged() { if (root.service.dmOpenState === "acknowledged") root.newDmOpen = false }
  }

  // Keep delegates alive across snapshots; only changed rows are updated.
  readonly property var incomingMessages: service ? service.messages : []
  readonly property var incomingReplies: service ? service.threadRows : []
  onIncomingMessagesChanged: {
    var anchor = historyAnchor()
    syncRows(messageModel, incomingMessages, null)
    holdHistoryAnchor(anchor)
  }
  // A reader who scrolled up keeps the row at the top of the view where it was
  // when older rows arrive above it; a reader at the end keeps following. New
  // delegates settle their heights over a few layout passes, so the anchor is
  // re-applied whenever the list's height changes until it settles.
  property var pendingHistoryAnchor: null
  function historyAnchor() {
    if (pendingHistoryAnchor) return pendingHistoryAnchor
    if (historyScroll.follow) return null
    var flick = historyScroll.contentItem
    for (var i = 0; i < historyRepeater.count; i++) {
      var item = historyRepeater.itemAt(i)
      if (item && item.y + item.height > flick.contentY) return {id: item.row.id, offset: item.y - flick.contentY}
    }
    return null
  }
  function holdHistoryAnchor(anchor) {
    if (!anchor || !anchor.id) return
    pendingHistoryAnchor = anchor
    restoreHistoryAnchor()
    historyAnchorSettle.restart()
  }
  function restoreHistoryAnchor() {
    var anchor = pendingHistoryAnchor
    if (!anchor) return
    historyList.forceLayout()
    for (var i = 0; i < historyRepeater.count; i++) {
      var item = historyRepeater.itemAt(i)
      if (item && item.row.id === anchor.id) {
        historyScroll.contentItem.contentY = item.y - anchor.offset
        // The anchor exists only for a reader who was not following.
        historyScroll.follow = false
        return
      }
    }
    pendingHistoryAnchor = null
  }
  Timer {
    id: historyAnchorSettle
    interval: 250
    onTriggered: { root.restoreHistoryAnchor(); root.pendingHistoryAnchor = null }
  }
  onIncomingRepliesChanged: syncRows(replyModel, incomingReplies, service ? service.threadRoot : null)
  ListModel { id: messageModel; dynamicRoles: true }
  ListModel { id: replyModel; dynamicRoles: true }
  function syncRows(model, rows, before) {
    rows = rows || []
    // Thread replies name a validated parent: the root or an earlier reply.
    var authors = ({})
    if (before && before.id) authors[before.id] = before.author
    for (var i = 0; i < rows.length; i++) {
      var key = rows[i].id || ("sample-" + i)
      var found = -1
      for (var j = i; j < model.count; j++) {
        if (model.get(j).eventKey === key) { found = j; break }
      }
      var previous = i > 0 ? rows[i - 1] : before
      var timed = typeof rows[i].time === "number" && (!previous || typeof previous.time === "number")
      var dayBreak = timed && (!previous || service.dayKey(previous.time) !== service.dayKey(rows[i].time))
      // Consecutive messages from one author within five minutes share a header;
      // in a thread only while they answer the same parent.
      var grouped = timed && !!previous && previous !== before && !dayBreak && previous.author === rows[i].author
        && previous.parent === rows[i].parent && rows[i].time >= previous.time && rows[i].time - previous.time < 300
      var layout = {dayBreak: dayBreak, grouped: grouped}
      if (typeof rows[i].parent === "string") layout.parentAuthor = authors[rows[i].parent] || ""
      if (rows[i].id) authors[rows[i].id] = rows[i].author
      var serialized = JSON.stringify(Object.assign({}, rows[i], layout))
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
  Keys.onEscapePressed: avatarCard.opened ? avatarCard.close() : closeRequested()

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
        // My avatar: art from a local .ans or .txt file. Local only until profile
        // avatars are published to the relay; others still see the identicon.
        ColumnLayout {
          objectName: "buzzMyAvatar"
          visible: root.myAvatarAvailable
          Layout.fillWidth: true
          spacing: Style.space(2)
          RowLayout {
            Layout.fillWidth: true
            spacing: Style.space(4)
            BuzzAvatar {
              objectName: "buzzMyAvatarPreview"
              key: root.myAvatarAvailable ? root.service.identity : ""
              name: "You"
              art: root.myAvatarArt
              brightness: root.myAvatarBrightness
              pixelSize: root.sidebarAvatarSize
            }
            Ui.Button {
              objectName: "buzzSetMyAvatar"
              Layout.fillWidth: true
              text: myAvatarLoader.visible ? "Hide my avatar" : "Set my avatar"
              tooltipText: "Use a .ans or .txt file as your avatar on this machine"
              fontSize: Style.font.caption
              leftAlign: true
              focusable: true
              onClicked: myAvatarLoader.visible = !myAvatarLoader.visible
            }
          }
          AvatarFileLoader {
            id: myAvatarLoader
            visible: false
            Layout.fillWidth: true
            fieldName: "buzzMyAvatarPath"
            showClear: true
            canClear: root.myAvatarArt !== ""
            // A newly loaded file starts at the default brightness (auto-levels, 1.5).
            onArtLoaded: function(art) {
              if (!root.myAvatarAvailable || !root.agentService.setOwnAvatarArt(root.service.identity, art, AnsiArt.DEFAULT_BRIGHTNESS))
                fail("The avatar could not be kept.")
            }
            onClearRequested: if (root.myAvatarAvailable) root.agentService.setOwnAvatarArt(root.service.identity, "")
          }
          // Applied as it is changed, so the thumbnail beside it is the preview.
          AvatarBrightness {
            visible: myAvatarLoader.visible && root.myAvatarBrightness > 0
            Layout.fillWidth: true
            fieldName: "buzzMyAvatarBrightness"
            value: root.myAvatarBrightness > 0 ? root.myAvatarBrightness : AnsiArt.DEFAULT_BRIGHTNESS
            onChosen: function(value) { root.agentService.setOwnAvatarArt(root.service.identity, root.myAvatarArt, value) }
          }
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
              visible: root.service && (root.service.dmRooms.length > 0 || root.newDmAvailable)
              Layout.fillWidth: true
              text: "Direct messages"
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              opacity: 0.6
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Ui.Button {
              objectName: "buzzNewDm"
              visible: root.newDmAvailable
              Layout.fillWidth: true
              text: root.newDmOpen ? "Cancel new message" : "+ New message"
              tooltipText: "Message people from this room"
              fontSize: Style.font.caption
              leftAlign: true
              focusable: true
              onClicked: root.newDmOpen = !root.newDmOpen
            }
            ColumnLayout {
              objectName: "buzzNewDmPicker"
              visible: root.newDmAvailable && root.newDmOpen
              Layout.fillWidth: true
              Layout.leftMargin: Style.space(6)
              spacing: Style.space(2)
              Text {
                Layout.fillWidth: true
                text: root.service && root.service.dmCandidates.length
                  ? "Members of " + root.service.roomTitle(root.service.selectedRoom) + " · up to 8"
                  : "Open a room to choose its members"
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Color.foreground
                opacity: 0.6
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
              }
              Repeater {
                model: root.service ? root.service.dmCandidates : []
                delegate: Ui.Button {
                  required property var modelData
                  readonly property string key: modelData.key
                  objectName: "buzzNewDmCandidate"
                  Layout.fillWidth: true
                  clip: true
                  readonly property bool chosen: root.service.dmSelection.indexOf(modelData.key) !== -1
                  text: (chosen ? "✓ " : "") + (modelData.name.trim() || modelData.key.slice(0, 12) + "…")
                    + " · " + modelData.key.slice(0, 8) + " · " + modelData.label
                  tooltipText: modelData.key
                  fontSize: Style.font.caption
                  leftAlign: true
                  focusable: true
                  selected: chosen
                  onClicked: root.service.toggleDmParticipant(modelData.key)
                }
              }
              Ui.Button {
                objectName: "buzzNewDmStart"
                text: "Start" + (root.service && root.service.dmSelection.length ? " · " + root.service.dmSelection.length : "")
                fontSize: Style.font.caption
                focusable: true
                enabled: !!root.service && root.service.canStartDm
                opacity: enabled ? 1 : 0.5
                onClicked: if (root.service && root.service.canStartDm) root.service.startDm()
              }
            }
            Text {
              objectName: "buzzNewDmStatus"
              visible: root.newDmAvailable && text !== ""
              Layout.fillWidth: true
              text: root.service ? root.service.dmOpenLabel : ""
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              opacity: 0.7
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
              Controls.ToolTip.visible: dmStatusHover.containsMouse && truncated
              Controls.ToolTip.text: text
              MouseArea {
                id: dmStatusHover
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
              }
            }
            Repeater {
              model: root.service ? root.service.dmRooms : []
              delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                spacing: Style.space(4)
                BuzzAvatar {
                  key: root.dmPartner(modelData)
                  name: modelData.name
                  pixelSize: root.sidebarAvatarSize
                }
                Ui.Button {
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
            Text {
              objectName: "buzzAgentsHeading"
              visible: root.agentsVisible
              Layout.fillWidth: true
              text: "Agents"
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              opacity: 0.6
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Repeater {
              model: root.agentsVisible ? root.agentService.agents : []
              delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                spacing: Style.space(4)
                BuzzAvatar {
                  key: root.agentService.avatarKey(modelData)
                  name: modelData.name
                  art: root.agentService.avatarArtFor(modelData.id)
                  brightness: root.agentService.avatarBrightnessFor(modelData.id)
                  pixelSize: root.sidebarAvatarSize
                }
                Ui.Button {
                  objectName: "buzzAgentRow"
                  readonly property string agentId: modelData.id
                  Layout.fillWidth: true
                  clip: true
                  // Status in the row, harness in the tooltip: the sidebar is too narrow for both.
                  text: modelData.name + " · " + root.agentService.statusWord(modelData)
                  tooltipText: modelData.name + " · " + root.agentService.harnessLabel(modelData.harness) + " · " + root.agentService.statusWord(modelData)
                  leftAlign: true
                  focusable: true
                  selected: root.agentEditorShown && root.agentEditorId === modelData.id
                  onClicked: root.openAgentEditor(modelData.id)
                }
              }
            }
            Ui.Button {
              objectName: "buzzNewAgent"
              visible: root.agentsVisible
              Layout.fillWidth: true
              text: "+ New agent"
              tooltipText: "Create an agent that runs on this machine"
              fontSize: Style.font.caption
              leftAlign: true
              focusable: true
              selected: root.agentEditorShown && root.agentEditorId === ""
              onClicked: root.openAgentEditor("")
            }
            Text {
              objectName: "buzzAgentsUnavailable"
              visible: root.agentsUnavailableShown
              Layout.fillWidth: true
              text: "Agent manager unavailable"
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              opacity: 0.6
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
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

      AgentEditor {
        id: agentEditor
        objectName: "buzzAgentEditor"
        visible: root.agentEditorShown
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        agents: root.agentService
        agentId: root.agentEditorId
        onBackRequested: root.closeAgentEditor()
        // A create finished while its editor is still open: show the new agent.
        onAgentChosen: function(agentId) { if (root.agentEditorShown && root.agentEditorId === "") root.openAgentEditor(agentId) }
      }

      ColumnLayout {
        visible: root.showTimeline && !root.agentEditorShown
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
            objectName: "buzzHistoryLabel"
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
          // Setup assist: only with a helper that offers `setup_assist` and is
          // not authenticated. Older helpers keep the terminal instructions.
          RowLayout {
            Layout.fillWidth: true
            spacing: Style.space(8)
            visible: !!root.service && root.service.setupAssistAvailable
            Ui.TextField {
              id: relayField
              objectName: "buzzSetupRelayUrl"
              Layout.fillWidth: true
              verticalPadding: Style.space(4)
              maximumLength: 2048
              placeholderText: "wss://your-community.example"
              inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
              onAccepted: if (root.service) root.service.setupRelay(text)
              Component.onCompleted: if (root.service && root.service.relay) text = root.service.relay
            }
            Ui.Button {
              objectName: "buzzSetupRelay"
              text: "Use this relay"
              tooltipText: "Save this relay address in the helper"
              focusable: true
              onClicked: if (root.service) root.service.setupRelay(relayField.text)
            }
          }
          Connections {
            target: root.service
            // Show the helper's saved relay once, without overwriting typing.
            function onRelayChanged() { if (root.service.relay && !relayField.activeFocus) relayField.text = root.service.relay }
          }
          ColumnLayout {
            Layout.fillWidth: true
            spacing: Style.space(6)
            visible: !!root.service && root.service.identitySetupAvailable
            Text {
              objectName: "buzzExistingIdentityNote"
              Layout.fillWidth: true
              text: "I already have a Buzz identity: enroll it in a terminal with hidden input, then Retry. Never paste a key into this panel.\nomarchy-buzz setup identity enroll"
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: Color.foreground
              opacity: 0.7
              font.family: Style.font.family
              font.pixelSize: Style.font.body
            }
            Ui.Button {
              objectName: "buzzCreateIdentity"
              visible: !!root.service && root.service.identity === ""
              text: "Create a new identity on this device"
              tooltipText: "The helper generates a key and keeps it in your secret store"
              focusable: true
              onClicked: if (root.service) root.service.createIdentity()
            }
            Text {
              objectName: "buzzNewIdentityNote"
              visible: !!root.service && root.service.identity === ""
              Layout.fillWidth: true
              text: "A new identity belongs to no community yet. To take part, you will need an invitation to a community or an open room to join."
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: Color.foreground
              opacity: 0.7
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
          }
          Text {
            objectName: "buzzSetupStatus"
            Layout.fillWidth: true
            visible: text !== "" && !!root.service && root.service.setupAssistAvailable
            text: root.service ? root.service.setupCategoryLabel : ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body
          }
          PublicKeyRow {
            service: root.service
            visible: !!root.service && root.service.setupAssistAvailable && root.service.shortPublicKey !== ""
          }
          Ui.Button {
            text: "Retry connection"
            focusable: true
            visible: !!root.service
            onClicked: if (root.service) root.service.retry()
          }
        }
        // A new identity created here, now connected: it still belongs to no community.
        ColumnLayout {
          objectName: "buzzCreatedIdentity"
          Layout.fillWidth: true
          spacing: Style.space(4)
          visible: root.connected && !!root.service && !root.service.sampleMode
            && root.service.createdIdentity !== "" && root.service.createdIdentity === root.service.identity
          Text {
            Layout.fillWidth: true
            text: "New identity created on this device. It belongs to no community yet: ask for an invitation, or join an open room, to start chatting."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Color.foreground
            opacity: 0.7
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
          PublicKeyRow { service: root.service }
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
            onHeightChanged: root.restoreHistoryAnchor()
            Ui.Button {
              objectName: "buzzLoadOlder"
              anchors.horizontalCenter: parent.horizontalCenter
              visible: !!root.service && root.service.canLoadOlder
              text: root.service && root.service.olderLoading ? "Loading older messages…" : "Load older messages"
              tooltipText: "Show the next older messages in this room"
              fontSize: Style.font.caption
              horizontalPadding: Style.space(8)
              verticalPadding: Style.space(3)
              opacity: 0.7
              focusable: true
              onClicked: if (root.service) root.service.loadOlder()
              // Appearing or leaving above the rows must not move them either.
              onVisibleChanged: root.holdHistoryAnchor(root.historyAnchor())
            }
            Repeater {
              id: historyRepeater
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
                onAvatarRequested: function(key, name, art, brightness) { root.openAvatarCard(key, name, art, brightness) }
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
              onAvatarRequested: function(key, name, art, brightness) { root.openAvatarCard(key, name, art, brightness) }
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
                onAvatarRequested: function(key, name, art, brightness) { root.openAvatarCard(key, name, art, brightness) }
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

  // Profile card over the whole panel; closing it returns to the composer.
  AvatarCard {
    id: avatarCard
    anchors.fill: parent
    onClosed: root.activeComposer.field.forceActiveFocus()
  }
}
