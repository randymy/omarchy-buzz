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
  // Onboarding step two: redeem an invite (the helper parses and claims it),
  // read and accept the community's terms, and join open rooms. Used in the
  // setup view and in the sidebar footer.
  component JoinCommunity: ColumnLayout {
    id: joinBox
    property var service: null
    property bool showOpenRooms: false
    Layout.fillWidth: true
    spacing: Style.space(6)
    Text {
      Layout.fillWidth: true
      text: "Join with an invite"
      textFormat: Text.PlainText
      elide: Text.ElideRight
      color: Color.foreground
      opacity: 0.6
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(6)
      Ui.TextField {
        id: inviteField
        objectName: "buzzInviteInput"
        Layout.fillWidth: true
        verticalPadding: Style.space(4)
        maximumLength: 4096
        placeholderText: "Invite link or code"
        inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
        onAccepted: if (joinBox.service) joinBox.service.redeemInvite(text)
      }
      Ui.Button {
        objectName: "buzzInviteRedeem"
        text: "Redeem"
        tooltipText: "Check this invite with your relay"
        fontSize: Style.font.caption
        focusable: true
        enabled: !!joinBox.service && joinBox.service.canRedeemInvite
        opacity: enabled ? 1 : 0.5
        onClicked: if (joinBox.service) joinBox.service.redeemInvite(inviteField.text)
      }
    }
    Text {
      objectName: "buzzInviteStatus"
      Layout.fillWidth: true
      visible: text !== ""
      text: joinBox.service ? joinBox.service.inviteLabel : ""
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    ColumnLayout {
      id: policyBox
      objectName: "buzzJoinPolicy"
      Layout.fillWidth: true
      spacing: Style.space(4)
      visible: !!joinBox.service && joinBox.service.policyShown
      readonly property var policy: joinBox.service && joinBox.service.policyShown ? joinBox.service.joinSetup.joinPolicy : null
      Text {
        Layout.fillWidth: true
        text: "Joining means accepting this community's terms (version " + (policyBox.policy ? policyBox.policy.version : "") + "):"
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Color.foreground
        opacity: 0.7
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
      Controls.ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(Style.space(160), policyText.implicitHeight + Style.space(8))
        clip: true
        contentWidth: availableWidth
        Text {
          id: policyText
          objectName: "buzzJoinPolicyText"
          width: parent.width
          text: policyBox.policy ? (policyBox.policy.text || "(No text was provided.)") : ""
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
      }
      Text {
        Layout.fillWidth: true
        visible: !!policyBox.policy && (policyBox.policy.truncated || policyBox.policy.ageRequired)
        text: !policyBox.policy ? "" : (policyBox.policy.truncated ? "The text is longer than shown; the full terms are at "
            + joinBox.service.relay.replace(/^ws/, "http").replace(/\/$/, "") + "/api/join-policy/terms. " : "")
          + (policyBox.policy.ageRequired ? "Accepting also confirms you meet the community's minimum age." : "")
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Color.foreground
        opacity: 0.7
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
      Ui.Button {
        objectName: "buzzInviteAccept"
        text: "I accept"
        tooltipText: "Accept these terms and join the community"
        fontSize: Style.font.caption
        focusable: true
        enabled: !!joinBox.service && joinBox.service.canAcceptInvite
        opacity: enabled ? 1 : 0.5
        onClicked: if (joinBox.service) joinBox.service.acceptInvite()
      }
    }
    ColumnLayout {
      objectName: "buzzOpenRooms"
      Layout.fillWidth: true
      spacing: Style.space(2)
      visible: joinBox.showOpenRooms && !!joinBox.service && joinBox.service.openRoomsAvailable
      RowLayout {
        Layout.fillWidth: true
        spacing: Style.space(4)
        Text {
          Layout.fillWidth: true
          text: "Open rooms"
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: Color.foreground
          opacity: 0.6
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
        Ui.Button {
          objectName: "buzzOpenRoomsRefresh"
          text: "↻"
          tooltipText: "Refresh open rooms"
          horizontalPadding: Style.space(6)
          verticalPadding: Style.space(2)
          focusable: true
          enabled: !!joinBox.service && joinBox.service.openRoomsState !== "loading"
          opacity: enabled ? 1 : 0.5
          onClicked: joinBox.service.refreshOpenRooms()
        }
      }
      Text {
        Layout.fillWidth: true
        text: "Anyone in this community can join an open room; no approval is needed."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Color.foreground
        opacity: 0.6
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
      Repeater {
        model: joinBox.service ? joinBox.service.openRooms : []
        delegate: RowLayout {
          required property var modelData
          objectName: "buzzOpenRoom"
          readonly property string roomId: modelData.id
          Layout.fillWidth: true
          spacing: Style.space(4)
          Text {
            Layout.fillWidth: true
            text: "# " + modelData.name
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            Controls.ToolTip.visible: openRoomHover.containsMouse && modelData.description !== ""
            Controls.ToolTip.text: modelData.description
            MouseArea { id: openRoomHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
          }
          Ui.Button {
            objectName: "buzzOpenRoomJoin"
            text: "Join"
            tooltipText: "Join #" + modelData.name + " now; open rooms need no approval"
            fontSize: Style.font.caption
            focusable: true
            enabled: !!joinBox.service && !joinBox.service.roomActionBusy
            opacity: enabled ? 1 : 0.5
            onClicked: joinBox.service.joinRoom(modelData.id)
          }
        }
      }
      Text {
        objectName: "buzzOpenRoomsStatus"
        Layout.fillWidth: true
        visible: text !== ""
        text: joinBox.service ? joinBox.service.openRoomsLabel : ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Color.foreground
        opacity: 0.7
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
    }
    Text {
      objectName: "buzzRoomActionStatus"
      Layout.fillWidth: true
      visible: joinBox.showOpenRooms && text !== ""
      text: joinBox.service ? joinBox.service.roomActionLabel : ""
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
  }
  // Settings view: a section caption and a wrapped note.
  component SettingsCaption: Text {
    Layout.fillWidth: true
    Layout.topMargin: Style.space(6)
    textFormat: Text.PlainText
    elide: Text.ElideRight
    color: Color.foreground
    opacity: 0.6
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
    font.bold: true
  }
  // Invite people: one invite link with its Copy button.
  component InviteLinkRow: ColumnLayout {
    id: linkRow
    property string caption: ""
    property string link: ""
    property string linkName: ""
    property string copyName: ""
    property bool copied: false
    signal copyRequested()
    Layout.fillWidth: true
    spacing: Style.space(2)
    Text {
      Layout.fillWidth: true
      text: linkRow.caption
      textFormat: Text.PlainText
      elide: Text.ElideRight
      color: Color.foreground
      opacity: 0.6
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(6)
      Text {
        objectName: linkRow.linkName
        Layout.fillWidth: true
        text: linkRow.link
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
        Controls.ToolTip.visible: linkHover.containsMouse
        Controls.ToolTip.text: linkRow.link
        MouseArea { id: linkHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
      }
      Ui.Button {
        objectName: linkRow.copyName
        text: linkRow.copied ? "Copied" : "Copy"
        tooltipText: "Copy to the clipboard"
        fontSize: Style.font.caption
        focusable: true
        onClicked: linkRow.copyRequested()
      }
    }
  }
  component SettingsNote: Text {
    Layout.fillWidth: true
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    color: Color.foreground
    opacity: 0.7
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
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
    settingsOpen = false
    agentEditorId = id
    agentEditorOpen = true
    agentEditor.load()
    return true
  }
  function closeAgentEditor() {
    agentEditorOpen = false
    agentEditorId = ""
  }
  // The account control (bottom of the sidebar), its menu and the settings
  // view. Settings replaces the room view like the agent editor; the room, its
  // drafts and any open thread come back unchanged.
  property var manifest: null
  property bool settingsOpen: false
  property bool accountMenuOpen: false
  // Invite people: the chosen limits (the panel offers 1/5/25 uses, 1/7/30 days).
  property int inviteUses: 1
  property int inviteHours: 168
  // Fixed issue-tracker URL, opened only when Send feedback is chosen. Tests
  // may set feedbackOpener to record the call instead of opening a browser.
  readonly property string feedbackUrl: "https://github.com/randymy/omarchy-buzz/issues/new"
  property var feedbackOpener: null
  // sample, online, connecting, unset or offline.
  readonly property string accountState: !service ? "offline" : service.sampleMode ? "sample"
    : service.connection === "authenticated" && !service.sessionFailed ? "online"
    : service.connection === "connecting" ? "connecting"
    : service.connection === "unconfigured" ? "unset" : "offline"
  readonly property string accountStateLabel: ({sample: "Sample data", online: "Online", connecting: "Connecting",
    unset: "Not set up", offline: "Offline"})[accountState]
  readonly property color accountStateColor: ({online: "#3fb950", connecting: "#d29922", offline: Color.urgent})[accountState] || Color.muted
  readonly property bool myKeyKnown: !!service && /^[a-f0-9]{64}$/.test(service.identity)
  // My roster name when a verified roster lists me, else "Me".
  readonly property string myName: {
    if (!myKeyKnown || service.recipientsState !== "snapshot") return "Me"
    var entry = service.recipientEntries.find(function(item) { return item.key === root.service.identity })
    return entry && entry.name.trim() ? entry.name.trim() : "Me"
  }
  readonly property string accountName: accountState === "online" || accountState === "sample" ? myName : "Not connected"
  readonly property string communityHost: service && service.relay ? service.relay.replace(/^wss?:\/\//, "").replace(/\/$/, "") : ""
  readonly property string pluginVersion: {
    var source = manifest || (service ? service.manifest : null)
    var value = source ? source.version : ""
    return typeof value === "string" && /^[0-9A-Za-z.+-]{1,32}$/.test(value) ? value : ""
  }
  function openSettings() {
    closeAccountMenu()
    closeAgentEditor()
    settingsOpen = true
    Qt.callLater(function() { settingsBack.forceActiveFocus() })
    return true
  }
  function closeSettings() {
    if (!settingsOpen) return
    settingsOpen = false
    accountControl.forceActiveFocus()
  }
  function openAccountMenu() {
    if (!service) return false
    var origin = accountControl.mapToItem(root, 0, 0)
    accountMenu.anchorX = origin.x
    accountMenu.anchorY = origin.y
    accountMenuOpen = true
    accountMenu.forceActiveFocus()
    return true
  }
  function closeAccountMenu() {
    if (!accountMenuOpen) return
    accountMenuOpen = false
    accountControl.forceActiveFocus()
  }
  function sendFeedback() {
    closeAccountMenu()
    if (typeof feedbackOpener === "function") feedbackOpener(feedbackUrl)
    else Qt.openUrlExternally(feedbackUrl)
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
  readonly property bool threadOpen: !agentEditorShown && !settingsOpen && !!service && service.threadRootId !== "" && service.threadRoot !== null
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
  // The sidebar's join section: always when connected without rooms, else on request.
  property bool joinOpen: false
  readonly property bool joinFooterShown: connected && !!service && !service.sampleMode && service.joinAvailable
    && (service.streamRooms.length === 0 || joinOpen)
  // Leave room: a second click within five seconds confirms, like Delete.
  property bool leaveArmed: false
  Timer { id: leaveDisarm; interval: 5000; onTriggered: root.leaveArmed = false }
  function requestLeave() {
    if (!service || !service.canLeaveRoom) return false
    if (!leaveArmed) { leaveArmed = true; leaveDisarm.restart(); return true }
    leaveArmed = false
    return service.leaveRoom(service.selectedRoomId)
  }
  onNewDmAvailableChanged: if (!newDmAvailable) newDmOpen = false
  Connections {
    target: root.service
    function onDmOpenStateChanged() { if (root.service.dmOpenState === "acknowledged") root.newDmOpen = false }
    function onSelectedRoomIdChanged() { root.leaveArmed = false }
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
  Keys.onEscapePressed: accountMenuOpen ? closeAccountMenu() : avatarCard.opened ? avatarCard.close() : closeRequested()
  // Ctrl+, opens Settings, as Buzz Desktop's ⌘, does.
  Keys.onPressed: function(event) {
    if (event.key === Qt.Key_Comma && (event.modifiers & Qt.ControlModifier)) { event.accepted = true; openSettings() }
  }

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
        objectName: "buzzClose"
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
            Ui.Button {
              objectName: "buzzJoinRoomsToggle"
              visible: root.connected && !!root.service && !root.service.sampleMode && root.service.joinAvailable
                && root.service.streamRooms.length > 0
              Layout.fillWidth: true
              text: root.joinOpen ? "Hide open rooms" : "+ Join rooms"
              tooltipText: "Open rooms and invites"
              fontSize: Style.font.caption
              leftAlign: true
              focusable: true
              onClicked: { root.joinOpen = !root.joinOpen; if (root.joinOpen) root.service.refreshOpenRooms() }
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
        JoinCommunity {
          objectName: "buzzJoinFooter"
          visible: root.joinFooterShown
          service: root.service
          showOpenRooms: true
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
        // My account, pinned below everything else: avatar, name and connection
        // state. A click (or Enter) opens the account menu above it.
        Rectangle {
          id: accountControl
          objectName: "buzzAccount"
          visible: !!root.service
          Layout.fillWidth: true
          implicitHeight: accountRow.implicitHeight + Style.space(8)
          radius: Style.cornerRadius
          color: accountArea.pressed ? Util.alpha(Color.foreground, 0.14)
            : activeFocus || accountArea.containsMouse || root.accountMenuOpen ? Util.alpha(Color.foreground, 0.08) : "transparent"
          border.color: activeFocus ? Util.alpha(Color.accent, 0.8) : "transparent"
          border.width: Math.max(1, Style.space(1))
          activeFocusOnTab: true
          readonly property string stateName: root.accountState
          readonly property color dotColor: root.accountStateColor
          function activate() { if (root.accountMenuOpen) root.closeAccountMenu(); else root.openAccountMenu() }
          Keys.onReturnPressed: activate()
          Keys.onEnterPressed: activate()
          Keys.onSpacePressed: activate()
          RowLayout {
            id: accountRow
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: Style.space(4)
            anchors.rightMargin: Style.space(4)
            spacing: Style.space(6)
            BuzzAvatar {
              objectName: "buzzAccountAvatar"
              key: root.myKeyKnown ? root.service.identity : ""
              name: root.myName
              art: root.myAvatarArt
              brightness: root.myAvatarBrightness
              pixelSize: root.sidebarAvatarSize
            }
            Text {
              objectName: "buzzAccountName"
              Layout.fillWidth: true
              text: root.accountName
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Rectangle {
              objectName: "buzzAccountDot"
              implicitWidth: Style.space(8)
              implicitHeight: Style.space(8)
              radius: width / 2
              color: accountControl.dotColor
            }
          }
          MouseArea {
            id: accountArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: { accountControl.forceActiveFocus(); accountControl.activate() }
          }
          readonly property string tooltipText: root.accountName + " · " + root.accountStateLabel
            + (root.myKeyKnown ? " · " + root.service.identity.slice(0, 8) + "…" : "")
          Controls.ToolTip.visible: accountArea.containsMouse && !root.accountMenuOpen
          Controls.ToolTip.delay: 400
          Controls.ToolTip.text: tooltipText
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
        id: settingsView
        objectName: "buzzSettingsView"
        visible: root.settingsOpen
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        spacing: Style.space(8)
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.space(8)
          Ui.Button {
            id: settingsBack
            objectName: "buzzSettingsBack"
            text: "‹ Back to rooms"
            tooltipText: "Return to the room view"
            fontSize: Style.font.caption
            horizontalPadding: Style.space(6)
            verticalPadding: Style.space(2)
            focusable: true
            onClicked: root.closeSettings()
          }
          Text {
            Layout.fillWidth: true
            text: "Settings"
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body * 1.15
            font.bold: true
          }
        }
        Controls.ScrollView {
          Layout.fillWidth: true
          Layout.fillHeight: true
          contentWidth: availableWidth
          clip: true
          ColumnLayout {
            width: parent.width
            spacing: Style.space(4)

            // My avatar: art from a local .ans or .txt file. Local only until profile
            // avatars are published to the relay; others still see the identicon.
            SettingsCaption { text: "Avatar" }
            ColumnLayout {
              objectName: "buzzMyAvatar"
              visible: root.myAvatarAvailable
              Layout.fillWidth: true
              spacing: Style.space(4)
              RowLayout {
                Layout.fillWidth: true
                spacing: Style.space(8)
                BuzzAvatar {
                  objectName: "buzzMyAvatarPreview"
                  key: root.myAvatarAvailable ? root.service.identity : ""
                  name: "You"
                  art: root.myAvatarArt
                  brightness: root.myAvatarBrightness
                  pixelSize: Style.font.body
                }
                SettingsNote {
                  text: "Use a .ans or .txt file as your avatar on this machine. It is kept locally, not published; others still see your identicon."
                }
              }
              AvatarFileLoader {
                id: myAvatarLoader
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
                visible: root.myAvatarBrightness > 0
                Layout.fillWidth: true
                fieldName: "buzzMyAvatarBrightness"
                value: root.myAvatarBrightness > 0 ? root.myAvatarBrightness : AnsiArt.DEFAULT_BRIGHTNESS
                onChosen: function(value) { root.agentService.setOwnAvatarArt(root.service.identity, root.myAvatarArt, value) }
              }
            }
            SettingsNote {
              visible: !root.myAvatarAvailable
              text: root.service && root.service.sampleMode ? "Not available with sample data."
                : "Available once this device has a Buzz identity and the agent service is reachable."
            }

            SettingsCaption { visible: inviteSection.visible; text: "Invite people" }
            ColumnLayout {
              id: inviteSection
              objectName: "buzzInviteSection"
              visible: !!root.service && root.service.inviteMintSupported
              Layout.fillWidth: true
              spacing: Style.space(4)
              SettingsNote {
                text: "Create an invite for people who already use Buzz and for people who have never used it. Only the relay's owner or admins can create invites."
              }
              SettingsNote {
                objectName: "buzzInviteOffline"
                visible: !!root.service && !root.service.inviteMintAvailable
                text: "Connect to your relay to create invites."
              }
              RowLayout {
                visible: !!root.service && root.service.inviteMintAvailable
                Layout.fillWidth: true
                spacing: Style.space(4)
                SettingsNote { Layout.fillWidth: false; Layout.preferredWidth: Style.space(60); text: "Uses" }
                Repeater {
                  model: [1, 5, 25]
                  Ui.Button {
                    required property int modelData
                    objectName: "buzzInviteUses" + modelData
                    text: modelData === 1 ? "1 person" : modelData + " people"
                    tooltipText: "How many people can join with this invite"
                    fontSize: Style.font.caption
                    focusable: true
                    selected: root.inviteUses === modelData
                    onClicked: root.inviteUses = modelData
                  }
                }
                Item { Layout.fillWidth: true }
              }
              RowLayout {
                visible: !!root.service && root.service.inviteMintAvailable
                Layout.fillWidth: true
                spacing: Style.space(4)
                SettingsNote { Layout.fillWidth: false; Layout.preferredWidth: Style.space(60); text: "Expires" }
                Repeater {
                  model: [24, 168, 720]
                  Ui.Button {
                    required property int modelData
                    objectName: "buzzInviteHours" + modelData
                    text: ({24: "1 day", 168: "7 days", 720: "30 days"})[modelData]
                    tooltipText: "When this invite stops working"
                    fontSize: Style.font.caption
                    focusable: true
                    selected: root.inviteHours === modelData
                    onClicked: root.inviteHours = modelData
                  }
                }
                Item { Layout.fillWidth: true }
              }
              Ui.Button {
                objectName: "buzzMintInvite"
                visible: !!root.service && root.service.inviteMintAvailable
                text: "Create invite"
                tooltipText: "Ask the relay for a new invite"
                fontSize: Style.font.caption
                focusable: true
                enabled: !!root.service && root.service.canMintInvite
                opacity: enabled ? 1 : 0.5
                onClicked: if (root.service) root.service.mintInvite(root.inviteUses, root.inviteHours)
              }
              SettingsNote {
                objectName: "buzzInviteMintStatus"
                visible: text !== ""
                text: root.service ? root.service.mintLabel : ""
              }
              ColumnLayout {
                objectName: "buzzInviteResult"
                visible: !!root.service && root.service.inviteShown
                Layout.fillWidth: true
                spacing: Style.space(6)
                SettingsNote {
                  objectName: "buzzInviteDetails"
                  text: root.service ? root.service.inviteDetails : ""
                }
                InviteLinkRow {
                  caption: "Link for Buzz Desktop"
                  link: root.service ? root.service.inviteAppLink : ""
                  linkName: "buzzInviteAppLink"
                  copyName: "buzzCopyInviteApp"
                  copied: !!root.service && root.service.inviteCopied === "app"
                  onCopyRequested: if (root.service) root.service.copyInvite("app")
                }
                InviteLinkRow {
                  caption: "Web link (also pastes into Buzz for Omarchy)"
                  link: root.service ? root.service.inviteWebLink : ""
                  linkName: "buzzInviteWebLink"
                  copyName: "buzzCopyInviteWeb"
                  copied: !!root.service && root.service.inviteCopied === "web"
                  onCopyRequested: if (root.service) root.service.copyInvite("web")
                }
                Text {
                  Layout.fillWidth: true
                  text: "Message for newcomers"
                  textFormat: Text.PlainText
                  color: Color.foreground
                  opacity: 0.6
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                }
                Text {
                  objectName: "buzzInviteBlurb"
                  Layout.fillWidth: true
                  text: root.service ? root.service.inviteBlurb : ""
                  textFormat: Text.PlainText
                  wrapMode: Text.Wrap
                  color: Color.foreground
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                }
                Ui.Button {
                  objectName: "buzzCopyInviteBlurb"
                  text: root.service && root.service.inviteCopied === "blurb" ? "Copied" : "Copy message"
                  tooltipText: "Copy the message with the invite link"
                  fontSize: Style.font.caption
                  focusable: true
                  onClicked: if (root.service) root.service.copyInvite("blurb")
                }
              }
            }

            SettingsCaption { text: "Notifications" }
            RowLayout {
              Layout.fillWidth: true
              spacing: Style.space(8)
              Ui.Button {
                objectName: "buzzSettingsAlerts"
                text: root.service && root.service.notificationsEnabled ? "Alerts: on" : "Alerts: off"
                tooltipText: "Turn generic desktop notifications for new activity on or off"
                fontSize: Style.font.caption
                focusable: true
                selected: !!root.service && root.service.notificationsEnabled
                enabled: !!root.service
                onClicked: if (root.service) root.service.notificationsEnabled = !root.service.notificationsEnabled
              }
              SettingsNote {
                text: "Generic alerts for new activity outside the visible conversation, without message text or room names. Off by default; best-effort."
              }
            }

            SettingsCaption { text: "Window" }
            RowLayout {
              objectName: "buzzSettingsPresentation"
              visible: root.presentationSwitchEnabled
              Layout.fillWidth: true
              spacing: Style.space(4)
              Ui.Button {
                objectName: "buzzSettingsOverlay"
                text: "Overlay"
                tooltipText: "Show Buzz as an overlay above other windows"
                fontSize: Style.font.caption
                focusable: true
                selected: !root.windowMode
                onClicked: if (root.windowMode) root.presentationRequested()
              }
              Ui.Button {
                objectName: "buzzSettingsWindow"
                text: "Window"
                tooltipText: "Keep Buzz open as a normal resizable window"
                fontSize: Style.font.caption
                focusable: true
                selected: root.windowMode
                onClicked: if (!root.windowMode) root.presentationRequested()
              }
              Item { Layout.fillWidth: true }
            }
            SettingsNote {
              visible: !root.presentationSwitchEnabled
              text: "This view's presentation is fixed by its host."
            }

            SettingsCaption { text: "Shortcut" }
            SettingsNote {
              objectName: "buzzSettingsShortcut"
              text: "Super+B opens Buzz once the shortcut block is installed with scripts/desktop-shortcut install. This panel does not read your Hyprland configuration, so it cannot tell whether it is installed."
            }

            SettingsCaption { text: "About" }
            SettingsNote {
              objectName: "buzzSettingsVersion"
              visible: root.pluginVersion !== ""
              text: "Buzz for Omarchy " + root.pluginVersion
            }
            SettingsNote {
              objectName: "buzzSettingsRelay"
              text: root.communityHost !== "" ? "Relay " + root.communityHost : "No relay configured"
              wrapMode: Text.NoWrap
              elide: Text.ElideMiddle
            }
            PublicKeyRow {
              objectName: "buzzSettingsKey"
              visible: root.myKeyKnown
              service: root.service
            }
            SettingsNote {
              visible: !root.myKeyKnown
              text: "No identity on this device yet."
            }
          }
        }
      }

      ColumnLayout {
        visible: root.showTimeline && !root.agentEditorShown && !root.settingsOpen
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
          Ui.Button {
            objectName: "buzzLeaveRoom"
            text: root.leaveArmed ? "Confirm leave" : "Leave"
            tooltipText: "Leave this room; you can join again if it is open"
            fontSize: Style.font.caption
            horizontalPadding: Style.space(6)
            verticalPadding: Style.space(2)
            focusable: true
            selected: root.leaveArmed
            visible: !!root.service && root.service.communityJoinSupported && !root.service.sampleMode
              && root.service.connection === "authenticated" && root.service.selectedRoom !== null
              && root.service.selectedRoom.kind === "stream"
            enabled: !!root.service && root.service.canLeaveRoom
            opacity: enabled ? 1 : 0.5
            onClicked: root.requestLeave()
          }
        }
        Text {
          objectName: "buzzLeaveStatus"
          Layout.fillWidth: true
          visible: root.connected && !!root.service && !root.joinFooterShown && root.service.roomAction.action === "leave"
            && root.service.roomActionLabel !== ""
          text: root.service ? root.service.roomActionLabel : ""
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          opacity: 0.7
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
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
          JoinCommunity {
            objectName: "buzzJoinSetup"
            visible: !!root.service && root.service.joinAvailable
            service: root.service
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

  // Account menu: a compact popover above the account control. A click outside
  // it, Escape or choosing an item closes it.
  FocusScope {
    id: accountMenu
    objectName: "buzzAccountMenuLayer"
    anchors.fill: parent
    visible: root.accountMenuOpen
    z: 15
    property real anchorX: 0
    property real anchorY: 0
    Keys.onEscapePressed: function(event) { event.accepted = true; root.closeAccountMenu() }
    MouseArea {
      objectName: "buzzAccountMenuOutside"
      anchors.fill: parent
      acceptedButtons: Qt.AllButtons
      onClicked: root.closeAccountMenu()
      onWheel: function(wheel) { wheel.accepted = true }
    }
    Rectangle {
      id: accountMenuCard
      objectName: "buzzAccountMenu"
      width: Math.min(root.width - Style.space(16), Math.max(Style.space(230), accountControl.width))
      height: accountMenuColumn.implicitHeight + Style.space(16)
      x: Math.max(Style.space(8), Math.min(accountMenu.anchorX, root.width - width - Style.space(8)))
      y: Math.max(Style.space(8), accountMenu.anchorY - height - Style.space(4))
      color: Color.popups.background
      border.color: Color.popups.border
      border.width: Math.max(1, Style.space(1))
      radius: Style.cornerRadius
      MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
      ColumnLayout {
        id: accountMenuColumn
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Style.space(8)
        spacing: Style.space(4)
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.space(8)
          BuzzAvatar {
            key: root.myKeyKnown ? root.service.identity : ""
            name: root.myName
            art: root.myAvatarArt
            brightness: root.myAvatarBrightness
            pixelSize: Style.font.caption
          }
          ColumnLayout {
            Layout.fillWidth: true
            spacing: Style.space(2)
            Text {
              objectName: "buzzAccountMenuName"
              Layout.fillWidth: true
              text: root.accountName
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.body
              font.bold: true
            }
            Rectangle {
              objectName: "buzzAccountStatePill"
              readonly property string text: root.accountStateLabel
              implicitWidth: pillText.implicitWidth + Style.space(12)
              implicitHeight: pillText.implicitHeight + Style.space(4)
              radius: height / 2
              color: Util.alpha(root.accountStateColor, 0.18)
              border.color: root.accountStateColor
              border.width: Math.max(1, Style.space(1))
              Text {
                id: pillText
                anchors.centerIn: parent
                text: root.accountStateLabel
                textFormat: Text.PlainText
                color: Color.foreground
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
              }
            }
          }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Util.alpha(Color.foreground, 0.14) }
        // One community per helper for now; the row names it without switching.
        Item {
          objectName: "buzzAccountCommunity"
          Layout.fillWidth: true
          implicitHeight: communityRow.implicitHeight
          readonly property string tooltipText: "Switching communities is not available yet"
          Controls.ToolTip.visible: communityHover.containsMouse
          Controls.ToolTip.delay: 400
          Controls.ToolTip.text: tooltipText
          MouseArea { id: communityHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
          RowLayout {
            id: communityRow
            anchors.fill: parent
            spacing: Style.space(6)
            Text {
              text: "⬢"
              textFormat: Text.PlainText
              color: Color.accent
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            ColumnLayout {
              Layout.fillWidth: true
              spacing: 0
              Text {
                Layout.fillWidth: true
                text: "Community"
                textFormat: Text.PlainText
                color: Color.foreground
                opacity: 0.6
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
              }
              Text {
                objectName: "buzzAccountCommunityHost"
                Layout.fillWidth: true
                text: root.communityHost !== "" ? root.communityHost : root.service && root.service.sampleMode ? root.service.viewModel.community : "No relay"
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
                color: Color.foreground
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
              }
            }
            Text {
              text: "›"
              textFormat: Text.PlainText
              color: Color.foreground
              opacity: 0.4
              font.family: Style.font.family
              font.pixelSize: Style.font.body
            }
          }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Util.alpha(Color.foreground, 0.14) }
        Ui.Button {
          objectName: "buzzAccountFeedback"
          Layout.fillWidth: true
          text: "Send feedback"
          tooltipText: "Open a new issue for Buzz for Omarchy in your browser"
          fontSize: Style.font.caption
          leftAlign: true
          focusable: true
          onClicked: root.sendFeedback()
        }
        Ui.Button {
          objectName: "buzzAccountSettings"
          Layout.fillWidth: true
          text: "Settings · Ctrl+,"
          tooltipText: "Avatar, notifications, window and about"
          fontSize: Style.font.caption
          leftAlign: true
          focusable: true
          onClicked: root.openSettings()
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
