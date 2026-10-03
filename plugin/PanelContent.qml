import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons
import "AnsiArt.js" as AnsiArt
import "PlainText.js" as PlainText

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
      Controls.ToolTip.text: PlainText.tip(keyRow.service ? keyRow.service.identity : "")
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
    // The welcome pane shows open rooms and the invite field as separate
    // blocks; its copies carry their own object names (prefix "buzzWelcome").
    property bool showInvite: true
    property string prefix: "buzz"
    property string inviteCaption: "Join with an invite"
    Layout.fillWidth: true
    spacing: Style.space(6)
    Text {
      Layout.fillWidth: true
      visible: joinBox.showInvite
      text: joinBox.inviteCaption
      textFormat: Text.PlainText
      elide: Text.ElideRight
      color: Color.foreground
      opacity: 0.6
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    RowLayout {
      Layout.fillWidth: true
      visible: joinBox.showInvite
      spacing: Style.space(6)
      Ui.TextField {
        id: inviteField
        objectName: joinBox.prefix + "InviteInput"
        Layout.fillWidth: true
        verticalPadding: Style.space(4)
        maximumLength: 4096
        placeholderText: "Invite link or code"
        inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
        onAccepted: if (joinBox.service) joinBox.service.redeemInvite(text)
      }
      Ui.Button {
        objectName: joinBox.prefix + "InviteRedeem"
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
      objectName: joinBox.prefix + "InviteStatus"
      Layout.fillWidth: true
      visible: joinBox.showInvite && text !== ""
      text: joinBox.service ? joinBox.service.inviteLabel : ""
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    ColumnLayout {
      id: policyBox
      objectName: joinBox.prefix + "JoinPolicy"
      Layout.fillWidth: true
      spacing: Style.space(4)
      visible: joinBox.showInvite && !!joinBox.service && joinBox.service.policyShown
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
          objectName: joinBox.prefix + "JoinPolicyText"
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
        objectName: joinBox.prefix + "InviteAccept"
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
      objectName: joinBox.prefix + "OpenRooms"
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
          objectName: joinBox.prefix + "OpenRoomsRefresh"
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
          objectName: joinBox.prefix + "OpenRoom"
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
            Controls.ToolTip.text: PlainText.tip(modelData.description)
            MouseArea { id: openRoomHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
          }
          Ui.Button {
            objectName: joinBox.prefix + "OpenRoomJoin"
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
        objectName: joinBox.prefix + "OpenRoomsStatus"
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
      objectName: joinBox.prefix + "RoomActionStatus"
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
        Controls.ToolTip.text: PlainText.tip(linkRow.link)
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
  // Join an existing community: Desktop's field and button
  // (`InviteRedeemForm.tsx`, add-community variant). The helper parses the text
  // (community URL, invite link or code) and answers with a fixed category.
  // Used in the Join and Create views and in first setup.
  component CommunityJoinForm: ColumnLayout {
    id: joinForm
    property var service: null
    property string label: "Community URL or invite link"
    property string fieldName: "buzzCommunityInput"
    property string buttonName: "buzzCommunityJoin"
    property string statusName: "buzzCommunityJoinStatus"
    property alias field: communityField
    // Older helpers (no `communities`) take a relay address in first setup.
    readonly property bool legacy: !!service && !service.communitiesSupported && !service.sampleMode
    readonly property bool canSubmit: !!service && communityField.text.trim() !== ""
      && (legacy ? service.setupAssistAvailable && service.setupState !== "sending" : service.canJoinCommunity)
    function submit() { if (canSubmit) service.joinCommunity(communityField.text) }
    Layout.fillWidth: true
    spacing: Style.space(4)
    Text {
      Layout.fillWidth: true
      text: joinForm.label
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
        id: communityField
        objectName: joinForm.fieldName
        Layout.fillWidth: true
        verticalPadding: Style.space(4)
        maximumLength: 4096
        placeholderText: "https://community.example.com or paste an invite link"
        inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
        onAccepted: joinForm.submit()
      }
      Ui.Button {
        objectName: joinForm.buttonName
        text: "Join community"
        tooltipText: "Join with this community URL or invite link"
        focusable: true
        enabled: joinForm.canSubmit
        opacity: enabled ? 1 : 0.5
        onClicked: joinForm.submit()
      }
    }
    Text {
      objectName: joinForm.statusName
      Layout.fillWidth: true
      visible: text !== ""
      // An older helper's first-setup outcome is shown by buzzSetupStatus.
      text: !joinForm.service || joinForm.legacy ? ""
        : joinForm.service.communityRequestKind === "join" ? joinForm.service.communityLabel : ""
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
  }
  // Create a new community: Desktop creates it on Builderlab, which a
  // third-party client cannot call (DESIGN.md). It happens at buzz.xyz in the
  // browser; the new community's URL or invite link then joins as usual.
  component CommunityCreateSteps: ColumnLayout {
    id: createSteps
    property var service: null
    property var opener: null
    property string prefix: "buzzCommunityCreate"
    Layout.fillWidth: true
    spacing: Style.space(6)
    Text {
      objectName: createSteps.prefix + "Description"
      Layout.fillWidth: true
      text: createSteps.service ? createSteps.service.createCommunityDescription : ""
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      opacity: 0.7
      font.family: Style.font.family
      font.pixelSize: Style.font.body
    }
    Ui.Button {
      objectName: createSteps.prefix + "Open"
      text: "Open buzz.xyz"
      tooltipText: "Open buzz.xyz in your browser to create a community"
      focusable: true
      // Fixed upstream URL, opened only by this explicit user action.
      onClicked: if (typeof createSteps.opener === "function") createSteps.opener("https://buzz.xyz"); else Qt.openUrlExternally("https://buzz.xyz")
    }
    CommunityJoinForm {
      service: createSteps.service
      label: "Paste your new community's URL or invite link"
      fieldName: createSteps.prefix + "Input"
      buttonName: createSteps.prefix + "Join"
      statusName: createSteps.prefix + "Status"
    }
  }
  // An invite's terms after joining a community that has them (`setup.policy`).
  component CommunityTerms: ColumnLayout {
    id: terms
    property var service: null
    readonly property var policy: service && service.policyShown ? service.joinSetup.joinPolicy : null
    Layout.fillWidth: true
    spacing: Style.space(4)
    visible: policy !== null
    Text {
      Layout.fillWidth: true
      text: "Joining means accepting this community's terms (version " + (terms.policy ? terms.policy.version : "") + "):"
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      opacity: 0.7
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    Controls.ScrollView {
      Layout.fillWidth: true
      Layout.preferredHeight: Math.min(Style.space(160), termsText.implicitHeight + Style.space(8))
      clip: true
      contentWidth: availableWidth
      Text {
        id: termsText
        objectName: "buzzCommunityTermsText"
        width: parent.width
        text: terms.policy ? (terms.policy.text || "(No text was provided.)") : ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
    }
    Text {
      Layout.fillWidth: true
      visible: !!terms.policy && terms.policy.ageRequired
      text: "Accepting also confirms you meet the community's minimum age."
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      opacity: 0.7
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    Ui.Button {
      objectName: "buzzCommunityAccept"
      text: "Accept and join"
      tooltipText: "Accept these terms and join the community"
      focusable: true
      enabled: !!terms.service && terms.service.canAcceptInvite
      opacity: enabled ? 1 : 0.5
      onClicked: terms.service.acceptInvite()
    }
    Text {
      objectName: "buzzCommunityTermsStatus"
      Layout.fillWidth: true
      visible: text !== ""
      text: terms.service ? terms.service.inviteLabel : ""
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
  }
  property bool presentationSwitchEnabled: false
  property bool windowMode: true
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
    statusOpen = false
    communityView = ""
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
  // sample, online, connecting, reconnecting, unset or offline.
  readonly property string accountState: !service ? "offline" : service.sampleMode ? "sample"
    : service.connection === "authenticated" && !service.sessionFailed ? "online"
    : service.reconnecting ? "reconnecting"
    : service.connection === "connecting" ? "connecting"
    : service.connection === "unconfigured" ? "unset" : "offline"
  readonly property string accountStateLabel: ({sample: "Sample data", online: "Online", connecting: "Connecting",
    reconnecting: "Reconnecting…", unset: "Not set up", offline: "Offline"})[accountState]
  readonly property color accountStateColor: ({online: "#3fb950", connecting: "#d29922", reconnecting: "#d29922", offline: Color.urgent})[accountState] || Color.muted
  readonly property bool myKeyKnown: !!service && /^[a-f0-9]{64}$/.test(service.identity)
  // My roster name when a verified roster lists me; otherwise my key's short
  // form, as message authors without a profile name are shown ("Me" only in
  // sample data, which has no key).
  readonly property bool myNameKnown: {
    if (!myKeyKnown || service.recipientsState !== "snapshot") return false
    var entry = service.recipientEntries.find(function(item) { return item.key === root.service.identity })
    return !!entry && entry.name.trim() !== ""
  }
  readonly property string myName: {
    if (!myKeyKnown) return "Me"
    if (!myNameKnown) return service.identity.slice(0, 12) + "…"
    return service.recipientEntries.find(function(item) { return item.key === root.service.identity }).name.trim()
  }
  readonly property string accountName: accountState === "online" || accountState === "sample" ? myName : "Not connected"
  readonly property string communityHost: service && service.relay ? service.relay.replace(/^wss?:\/\//, "").replace(/\/$/, "") : ""
  readonly property string pluginVersion: {
    var source = manifest || (service ? service.manifest : null)
    var value = source ? source.version : ""
    return typeof value === "string" && /^[0-9A-Za-z.+-]{1,32}$/.test(value) ? value : ""
  }
  // Presence: the menu row and your own dot (`presence`).
  readonly property bool presenceEntryShown: !!service && (service.presenceSupported || service.sampleMode)
  readonly property string myPresenceLabel: service && (accountState === "online" || accountState === "sample") ? service.presenceLabel(service.myPresence) : ""
  // Communities: Join an existing community and Create a new community, opened
  // from the account menu like Desktop's dialog. A join that succeeded returns
  // to the rooms (after accepting the community's terms, if it has them).
  property string communityView: ""
  property bool communityAwaiting: false
  // Settings: the community being renamed, and the one whose leave is armed.
  property string renamingRelay: ""
  property string leavingRelay: ""
  // Tests may set linkOpener to record buzz.xyz instead of opening a browser.
  property var linkOpener: null
  function openCommunityView(kind) {
    if (!service || (kind !== "join" && kind !== "create")) return false
    closeAccountMenu()
    closeAgentEditor()
    settingsOpen = false
    statusOpen = false
    statusAwaiting = false
    communityAwaiting = false
    service.resetCommunityRequest()
    communityView = kind
    Qt.callLater(function() { headerBack.forceActiveFocus() })
    return true
  }
  function switchCommunity(relay) {
    closeAccountMenu()
    if (!service) return false
    var target = service.communityEntries.find(function(entry) { return entry.relay === relay }) || null
    menuSwitchPending = true
    var sent = service.switchCommunity(relay)
    menuSwitchPending = sent && !service.sampleMode
    // Sample data switches at once (presentation only).
    if (sent && service.sampleMode && target) showArrival("Switched to " + target.name + ".")
    return sent
  }
  // After a join or a switch chosen from the menu: one dismissible line at the
  // top of the room pane, gone after a few seconds or on any navigation.
  // Switches nobody chose here (startup, leaving the active community) say nothing.
  property bool menuSwitchPending: false
  property string arrivalText: ""
  property int arrivalTimeout: 8000
  property string arrivalRoom: ""
  function showArrival(text) {
    arrivalText = text
    arrivalRoom = service ? service.selectedRoomId : ""
    arrivalTimer.interval = arrivalTimeout
    arrivalTimer.restart()
  }
  function dismissArrival() {
    arrivalTimer.stop()
    arrivalText = ""
  }
  Timer { id: arrivalTimer; interval: 8000; onTriggered: root.arrivalText = "" }
  onSubViewOpenChanged: if (subViewOpen) dismissArrival()
  onThreadOpenChanged: if (threadOpen) dismissArrival()
  Connections {
    target: root.service
    function onCommunityArrived(kind, name) {
      if (kind === "joined") root.showArrival("You joined " + name + ".")
      else if (kind === "switched" && root.menuSwitchPending) root.showArrival("Switched to " + name + ".")
      root.menuSwitchPending = false
    }
    // Choosing another room is navigation; the first room a new community
    // selects by itself (from none) is not.
    function onSelectedRoomIdChanged() {
      var now = root.service.selectedRoomId
      if (root.arrivalText !== "" && root.arrivalRoom !== "" && now !== root.arrivalRoom) root.dismissArrival()
      root.arrivalRoom = now
    }
  }
  // The active community lists no joined room: a welcome pane replaces the
  // empty room view, with the open rooms, the invite field and the switcher.
  readonly property bool welcomeShown: connected && !!service && !service.sampleMode && service.noRoomsJoined
    && service.selectedRoom === null
  readonly property string welcomeName: service && service.activeCommunity ? service.activeCommunity.name : communityHost
  Connections {
    target: root.service
    function onCommunityRequestDone(kind, ok) {
      if (kind === "join" && root.communityView !== "") root.communityAwaiting = ok && root.service.policyShown
      if (kind === "join" && ok && root.communityView !== "" && !root.service.policyShown) root.backToRooms()
      if (kind === "rename" && ok) root.renamingRelay = ""
      if (kind === "leave") root.leavingRelay = ""
      if (kind === "switch" && !ok) root.menuSwitchPending = false
    }
    // The terms were accepted and the claim answered: back to the rooms.
    function onJoinSetupChanged() {
      if (root.communityView !== "" && root.communityAwaiting && root.service.joinSetup.state === "joined") root.backToRooms()
    }
  }
  // Update your status: a compact view like Settings, opened from the account
  // menu. The helper signs and publishes; the panel sends text, emoji and hours.
  property bool statusOpen: false
  property int statusHours: 24
  property bool statusAwaiting: false
  readonly property bool statusEntryShown: !!service && (service.userStatusSupported || service.sampleMode)
  readonly property string myStatusEmoji: service && service.myStatus ? service.statusEmojiText(service.myStatus) : ""
  readonly property string myStatusLine: service && service.myStatus
    ? myStatusEmoji + (service.myStatus.text ? " " + service.myStatus.text : "") : ""
  readonly property string statusDraftEmoji: statusEmojiField.text.trim()
  readonly property bool statusDraftValid: !!service && service.utf8Size(statusField.text.trim()) <= service.statusTextBytes
    && (statusDraftEmoji === "" || service.validStatusEmoji(statusDraftEmoji))
    && (statusField.text.trim() !== "" || statusDraftEmoji !== "")
  readonly property string statusDraftNote: !service ? ""
    : service.utf8Size(statusField.text.trim()) > service.statusTextBytes ? "Up to 200 characters."
    : statusDraftEmoji !== "" && !service.validStatusEmoji(statusDraftEmoji) ? "Use one emoji or a :shortcode:."
    : ""
  function openStatus() {
    if (!statusEntryShown) return false
    closeAccountMenu()
    closeAgentEditor()
    settingsOpen = false
    communityView = ""
    var mine = service.myStatus
    statusField.text = mine ? mine.text : ""
    statusEmojiField.text = mine && mine.emoji ? mine.emoji : ""
    statusHours = 24
    statusAwaiting = false
    service.resetStatusRequest()
    statusOpen = true
    Qt.callLater(function() { statusField.forceActiveFocus() })
    return true
  }
  function closeStatus() {
    if (!statusOpen) return
    statusOpen = false
    statusAwaiting = false
    focusRooms()
  }
  function chooseStatusEmoji(emoji) {
    statusEmojiField.text = statusDraftEmoji === emoji ? "" : emoji
  }
  function submitStatus() {
    if (!service || !statusDraftValid) return false
    statusAwaiting = service.setStatus(statusField.text, statusDraftEmoji, statusHours)
    return statusAwaiting
  }
  function submitClearStatus() {
    if (!service) return false
    statusAwaiting = service.clearStatus()
    return statusAwaiting
  }
  Connections {
    target: root.service
    // A change the relay accepted closes the view, as Desktop's dialog does.
    function onUserStatusChanged() {
      if (root.statusOpen && root.statusAwaiting && root.service.statusRequestState === "idle" && root.service.userStatus.state === "ready") root.closeStatus()
    }
  }
  function openSettings() {
    closeAccountMenu()
    closeAgentEditor()
    statusOpen = false
    communityView = ""
    settingsOpen = true
    Qt.callLater(function() { headerBack.forceActiveFocus() })
    return true
  }
  function closeSettings() {
    if (!settingsOpen) return
    settingsOpen = false
    focusRooms()
  }
  // Header navigation. Every view that replaces the room view is listed here;
  // the header then shows "← Back to rooms" and the view's title in place of
  // the Buzz title and room name.
  readonly property string subView: settingsOpen ? "settings" : statusOpen ? "status"
    : communityView === "join" ? "join-community" : communityView === "create" ? "create-community"
    : communityView === "new-room" ? "new-room" : communityView === "room-settings" ? "room-settings"
    : agentEditorShown ? (agentEditorId === "" ? "new-agent" : "agent") : ""
  readonly property bool subViewOpen: subView !== ""
  readonly property string subViewTitle: {
    if (subView === "settings") return "Settings"
    if (subView === "status") return "Update your status"
    if (subView === "join-community") return "Join an existing community"
    if (subView === "create-community") return "Create a new community"
    if (subView === "new-room") return "New room"
    if (subView === "room-settings") return "Room settings"
    if (subView === "new-agent") return "New agent"
    if (subView === "agent") {
      var entry = agentService ? agentService.agent(agentEditorId) : null
      return entry && entry.name ? entry.name : "Agent"
    }
    return ""
  }
  // Focus after returning: the composer that is in use, else the panel itself.
  function focusRooms() {
    var composer = activeComposer
    if (composer && composer.visible && composer.field.visible) composer.field.forceActiveFocus()
    else root.forceActiveFocus()
  }
  // Close whichever view replaced the room view; the room, its drafts and any
  // open thread come back unchanged.
  function backToRooms() {
    if (!subViewOpen) return false
    settingsOpen = false
    statusOpen = false
    statusAwaiting = false
    communityView = ""
    communityAwaiting = false
    closeAgentEditor()
    focusRooms()
    return true
  }
  // Escape, in this order (one step per key press):
  //   1. an open menu, card or picker: the account menu, the profile card, the
  //      composer's mention list, recipient picker or attach field, the new
  //      direct message picker, the sidebar's join section;
  //   2. the thread panel;
  //   3. a view that replaced the room view (Settings, Update your status, an
  //      agent or New agent): back to rooms;
  //   4. otherwise close Buzz.
  function escapeKey() {
    if (accountMenuOpen) { closeAccountMenu(); return "menu" }
    if (avatarCard.opened) { avatarCard.close(); return "menu" }
    var composer = activeComposer
    if (composer.mentionOpen) { composer.mentionDismissed = true; composer.field.forceActiveFocus(); return "menu" }
    if (composer.pickerExpanded) { composer.pickerExpanded = false; return "menu" }
    if (composer.attachOpen) { composer.attachOpen = false; composer.field.forceActiveFocus(); return "menu" }
    if (newDmOpen) { newDmOpen = false; return "menu" }
    if (joinOpen) { joinOpen = false; return "menu" }
    if (threadOpen) { closeThread(); return "thread" }
    if (backToRooms()) return "view"
    closeRequested()
    return "close"
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
    // A Repeater's modelData turns arrays into array-likes (Array.isArray is
    // false there), so the participants are copied into a plain array first.
    var participants = room && room.participants && typeof room.participants.length === "number"
      ? Array.prototype.slice.call(room.participants) : null
    if (!participants || !service) return ""
    var others = participants.filter(function(key) { return typeof key === "string" && key !== root.service.identity })
    return others.length ? others[0] : ""
  }
  readonly property bool threadOpen: !agentEditorShown && !settingsOpen && !statusOpen && communityView === "" && !!service && service.threadRootId !== "" && service.threadRoot !== null
  // An open thread sits beside the room. Narrow windows give up the room list
  // first, then the room itself, so the thread always has a readable column.
  readonly property bool showRooms: !threadOpen || width >= Style.space(1100)
  readonly property bool showTimeline: !threadOpen || width >= Style.space(640)
  readonly property var activeComposer: threadOpen && service.replyRootId !== "" ? threadComposer : roomComposer
  readonly property var mentionMatches: activeComposer.mentionMatches
  readonly property bool mentionOpen: activeComposer.mentionOpen
  function chooseMention(index) { return activeComposer.chooseMention(index) }
  // New direct message: a compact picker over people the helper verified.
  readonly property bool newDmAvailable: !!service && service.dmOpenAvailable
  property bool newDmOpen: false
  // A pick clears the search. Choosing rebuilds the list and destroys the
  // button that called, so this runs from here, not from the button.
  function pickDmPerson(key, name) { if (service.toggleDmParticipant(key, name)) dmSearch.text = "" }
  // What the picker's list cannot show: the helper's states, never guessed ones.
  readonly property string newDmNote: {
    if (!service) return ""
    var state = service.peopleStatus
    if (state === "loading") return service.peopleText ? "Searching people…" : "Loading people…"
    if (state === "failed" || (state === "idle" && service.peopleSupported))
      return ({people_timeout: "People search timed out", people_invalid: "The relay's people list was not usable"})[service.peopleViewCategory]
        || "People are not available right now"
    if (service.dmCandidates.length > 0) return ""
    if (!service.peopleSupported) return "Open a room to choose its members"
    return service.peopleText ? "No people match \u201c" + service.peopleText + "\u201d" : "No people found"
  }
  readonly property bool newDmRetryable: !!service && (service.peopleStatus === "failed" || (service.peopleStatus === "idle" && service.peopleSupported))
  // The sidebar's join section: on request, or when connected without rooms
  // while the welcome pane is not already listing the same open rooms.
  property bool joinOpen: false
  readonly property bool joinFooterShown: connected && !!service && !service.sampleMode && service.joinAvailable
    && (joinOpen || (service.streamRooms.length === 0 && !welcomeShown))
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
  // New room and Room settings (`room_manage`) replace the room view like the
  // community views do, through `communityView` ("new-room", "room-settings").
  property bool newRoomPrivate: false
  property bool newRoomAwaiting: false
  property bool topicTouched: false
  property string armedMember: ""
  Timer { id: memberDisarm; interval: 5000; onTriggered: root.armedMember = "" }
  readonly property bool roomManageShown: !!service && !service.sampleMode && service.roomManageAvailable
  function openNewRoom() {
    if (!roomManageShown) return false
    closeAccountMenu()
    closeAgentEditor()
    settingsOpen = false
    statusOpen = false
    statusAwaiting = false
    communityAwaiting = false
    service.resetRoomRequest()
    newRoomName.text = ""
    newRoomAbout.text = ""
    newRoomPrivate = false
    newRoomAwaiting = false
    communityView = "new-room"
    Qt.callLater(function() { newRoomName.forceActiveFocus() })
    return true
  }
  readonly property bool newRoomValid: !!service && service.validRoomText(newRoomName.text, 128, true) && service.validRoomText(newRoomAbout.text, 512, false)
  function submitNewRoom() {
    if (!newRoomValid || !service.canManageRooms) return false
    newRoomAwaiting = service.createRoom(newRoomName.text, newRoomAbout.text, newRoomPrivate)
    return newRoomAwaiting
  }
  function openRoomSettings() {
    if (!roomManageShown || !service.selectedRoom || service.selectedRoom.kind !== "stream") return false
    closeAccountMenu()
    closeAgentEditor()
    settingsOpen = false
    statusOpen = false
    statusAwaiting = false
    communityAwaiting = false
    service.resetRoomRequest()
    communityView = "room-settings"
    fillRoomSettings()
    service.refreshRoomDetail()
    Qt.callLater(function() { headerBack.forceActiveFocus() })
    return true
  }
  // The edit fields start from what the relay lists for the selected room.
  function fillRoomSettings() {
    var room = service ? service.selectedRoom : null
    editRoomName.text = room ? room.name : ""
    editRoomAbout.text = room ? room.description : ""
    topicTouched = false
    editRoomTopic.text = service && service.roomDetailShown ? service.roomDetail.topic : ""
    addMemberKey.text = ""
    armedMember = ""
  }
  // One outcome line for a view: the last change's, when it is one of `kinds`
  // (or when nothing was sent at all).
  function changeLine(kinds) {
    if (!service || service.roomActionLabel === "") return ""
    return service.roomActionLocal === "failed" || kinds.indexOf(service.roomAction.action) !== -1 || service.roomActionLocal === "sending"
      ? service.roomActionLabel : ""
  }
  function submitMemberRemoval(key) {
    if (!service || !service.canEditRoom) return false
    if (armedMember !== key) { armedMember = key; memberDisarm.restart(); return true }
    armedMember = ""
    return service.removeRoomMember(key)
  }
  function finishCreate() {
    var action = service.roomAction
    if (communityView === "new-room" && newRoomAwaiting && action.action === "create" && action.state === "acknowledged"
        && service.selectedRoomId === action.roomId) { newRoomAwaiting = false; backToRooms() }
  }
  function memberLabel(member) {
    var who = member.name.trim() || member.key.slice(0, 12) + "…"
    return who + " · " + member.role + (service && member.key === service.identity ? " · you" : "")
  }
  Connections {
    target: root.service
    function onSelectedRoomIdChanged() {
      root.finishCreate()
      if (root.communityView === "room-settings") { root.fillRoomSettings(); root.service.refreshRoomDetail() }
    }
    function onRoomDetailChanged() {
      if (root.communityView === "room-settings" && !root.topicTouched && !editRoomTopic.activeFocus && root.service.roomDetailShown)
        editRoomTopic.text = root.service.roomDetail.topic
    }
    // Back to the rooms once the room just created is listed and selected.
    function onRoomActionChanged() { root.finishCreate() }
  }
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
  Keys.onEscapePressed: escapeKey()
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

    // Header: where you are on the left (Buzz and the room, or Back to rooms and
    // the open view), the connection status, and × to close Buzz on the right.
    RowLayout {
      objectName: "buzzHeader"
      Layout.fillWidth: true
      // One height on every view, so opening or leaving a view never moves the page.
      Layout.preferredHeight: Math.max(headerBack.implicitHeight, headerClose.implicitHeight)
      spacing: Style.space(10)
      Text {
        objectName: "buzzHeaderTitle"
        visible: !root.subViewOpen
        text: "Buzz"
        textFormat: Text.PlainText
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.body * 1.3
        font.bold: true
      }
      Ui.Button {
        id: headerBack
        objectName: "buzzHeaderBack"
        visible: root.subViewOpen
        text: "← Back to rooms"
        tooltipText: "Return to the room view · Esc"
        fontSize: Style.font.body * 1.15
        bordered: true
        horizontalPadding: Style.space(8)
        verticalPadding: Style.space(3)
        focusable: true
        onClicked: root.backToRooms()
      }
      Text {
        visible: root.subViewOpen
        text: "·"
        textFormat: Text.PlainText
        color: Color.foreground
        opacity: 0.6
        font.family: Style.font.family
        font.pixelSize: Style.font.body * 1.15
      }
      Text {
        objectName: "buzzHeaderPlace"
        // The room view's title lives in the timeline's own row; the header
        // names only a sub-view, so the room is never shown twice (an empty
        // text keeps the slot, so opening a view needs no relayout).
        Layout.maximumWidth: Style.space(260)
        text: root.subViewOpen ? root.subViewTitle : ""
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: Color.foreground
        opacity: root.subViewOpen ? 1 : 0.8
        font.family: Style.font.family
        font.pixelSize: Style.font.body * 1.15
        font.bold: true
      }
      Text {
        objectName: "buzzHeaderStatus"
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
      // Only the overlay closes from here: a normal window has its own frame
      // and closes like any other (Super+W on Omarchy); Esc from the room view
      // still closes either presentation.
      Ui.Button {
        id: headerClose
        objectName: "buzzHeaderClose"
        visible: !root.windowMode
        text: "×"
        tooltipText: root.subViewOpen || root.threadOpen ? "Close Buzz" : "Close Buzz · Esc"
        fontSize: Style.font.body * 1.3
        horizontalPadding: Style.space(8)
        verticalPadding: Style.space(1)
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
          Controls.ToolTip.text: PlainText.tip(root.service ? (root.service.relay || root.service.viewModel.community) + " · " + root.service.catalogLabel : "")
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
            Ui.Button {
              objectName: "buzzNewRoom"
              visible: root.connected && root.roomManageShown
              Layout.fillWidth: true
              text: "+ New room"
              tooltipText: "Create an open or private room"
              fontSize: Style.font.caption
              leftAlign: true
              focusable: true
              onClicked: root.openNewRoom()
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
              tooltipText: "Message people"
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
              onVisibleChanged: if (visible) {
                dmSearch.text = ""
                dmSearchDebounce.stop()
                if (root.service) root.service.searchPeople("")
              }
              Text {
                Layout.fillWidth: true
                text: root.service && root.service.dmSelection.length ? root.service.dmSelection.length + " of 8 chosen" : "Choose up to 8 people"
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Color.foreground
                opacity: 0.6
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
              }
              Flow {
                objectName: "buzzNewDmChips"
                visible: !!root.service && root.service.dmSelection.length > 0
                Layout.fillWidth: true
                spacing: Style.space(2)
                Repeater {
                  model: root.service ? root.service.dmSelection : []
                  delegate: Ui.Button {
                    required property string modelData
                    objectName: "buzzNewDmChip"
                    readonly property string key: modelData
                    text: ((root.service.dmNames[modelData] || "").trim() || modelData.slice(0, 8) + "…") + " ×"
                    fontSize: Style.font.caption
                    focusable: true
                    selected: true
                    onClicked: root.service.toggleDmParticipant(modelData)
                  }
                }
              }
              Ui.TextField {
                id: dmSearch
                objectName: "buzzNewDmSearch"
                visible: !!root.service && root.service.peopleSupported
                Layout.fillWidth: true
                verticalPadding: Style.space(4)
                maximumLength: 64
                placeholderText: "Search people"
                onTextChanged: dmSearchDebounce.restart()
                onAccepted: { dmSearchDebounce.stop(); if (root.service) root.service.searchPeople(text) }
              }
              Timer {
                id: dmSearchDebounce
                interval: 250
                onTriggered: if (root.service) root.service.searchPeople(dmSearch.text)
              }
              Repeater {
                model: root.service ? root.service.dmCandidates : []
                delegate: RowLayout {
                  required property var modelData
                  Layout.fillWidth: true
                  spacing: Style.space(4)
                  PresenceDot {
                    objectName: "buzzNewDmPresence"
                    service: root.service
                    presence: parent.modelData.presence || ""
                  }
                  Ui.Button {
                    readonly property var modelData: parent.modelData
                    readonly property string key: modelData.key
                    objectName: "buzzNewDmCandidate"
                    Layout.fillWidth: true
                    clip: true
                    readonly property bool chosen: root.service.dmSelection.indexOf(modelData.key) !== -1
                    text: (chosen ? "✓ " : "") + (modelData.name.trim() || modelData.key.slice(0, 12) + "…")
                      + (modelData.status ? " " + root.service.statusEmojiText(modelData.status) : "")
                      + " · " + modelData.key.slice(0, 8) + " · " + modelData.label
                    tooltipText: modelData.key + (modelData.status && modelData.status.text ? " · " + modelData.status.text : "")
                    fontSize: Style.font.caption
                    leftAlign: true
                    focusable: true
                    selected: chosen
                    onClicked: root.pickDmPerson(modelData.key, modelData.name)
                  }
                }
              }
              RowLayout {
                visible: root.newDmNote !== ""
                Layout.fillWidth: true
                spacing: Style.space(4)
                Text {
                  objectName: "buzzNewDmNote"
                  Layout.fillWidth: true
                  text: root.newDmNote
                  textFormat: Text.PlainText
                  elide: Text.ElideRight
                  color: Color.foreground
                  opacity: 0.6
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                }
                Ui.Button {
                  objectName: "buzzNewDmRetry"
                  visible: root.newDmRetryable
                  text: "Retry"
                  fontSize: Style.font.caption
                  focusable: true
                  onClicked: if (root.service) root.service.searchPeople(dmSearch.text)
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
              Controls.ToolTip.text: PlainText.tip(text)
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
                PresenceDot {
                  objectName: "buzzDmPresence"
                  service: root.service
                  presence: root.service ? root.service.presenceOf(root.dmPartner(modelData)) : ""
                }
                readonly property var partnerStatus: root.service ? root.service.authorStatus(root.dmPartner(modelData)) : null
                Ui.Button {
                  objectName: "buzzDmRow"
                  Layout.fillWidth: true
                  clip: true
                  text: modelData.name + (parent.partnerStatus ? " " + root.service.statusEmojiText(parent.partnerStatus) : "")
                    + (root.service && root.service.roomActivityCount(modelData.id) > 0 ? " · " + root.service.roomActivityCount(modelData.id) + "+" : "")
                  tooltipText: modelData.name + (parent.partnerStatus && parent.partnerStatus.text ? " · " + parent.partnerStatus.text : "")
                    + (root.service && root.service.roomActivityCount(modelData.id) > 0 ? " · new activity seen on this device, not synced unread" : "")
                  leftAlign: true
                  focusable: true
                  selected: root.service && root.service.selectedRoomId === modelData.id
                  onClicked: root.service.selectRoom(modelData.id)
                }
              }
            }
            // The catalog holds more joined rooms than one page: the next page is
            // read on request and merged; a failure keeps every room listed.
            Ui.Button {
              objectName: "buzzLoadMoreRooms"
              visible: !!root.service && (root.service.catalogMore === "available" || root.service.catalogMore === "failed")
              Layout.fillWidth: true
              text: root.service && root.service.catalogMore === "failed" ? "Try again · load more rooms" : "Load more rooms"
              tooltipText: "Show more of the rooms and messages you have joined"
              fontSize: Style.font.caption
              leftAlign: true
              focusable: true
              enabled: !!root.service && root.service.canLoadMoreRooms
              opacity: enabled ? 1 : 0.5
              onClicked: root.service.loadMoreRooms()
            }
            Text {
              objectName: "buzzLoadMoreStatus"
              visible: text !== ""
              Layout.fillWidth: true
              text: root.service ? root.service.catalogMoreLabel : ""
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: Color.foreground
              opacity: 0.7
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
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
              // Agents with an instance in the community shown here (in any
              // position, not only their first); the others are listed below.
              model: root.agentsVisible ? root.agentService.currentAgents : []
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
              objectName: "buzzAgentsOtherHeading"
              visible: root.agentsVisible && root.agentService.otherAgents.length > 0
              Layout.fillWidth: true
              text: "In other communities"
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              opacity: 0.45
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Repeater {
              // Muted and read-only: no Start or direct message here; the
              // editor explains where the agent can be managed.
              model: root.agentsVisible ? root.agentService.otherAgents : []
              delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                spacing: Style.space(4)
                opacity: 0.55
                BuzzAvatar {
                  key: root.agentService.avatarKey(modelData)
                  name: modelData.name
                  art: root.agentService.avatarArtFor(modelData.id)
                  brightness: root.agentService.avatarBrightnessFor(modelData.id)
                  pixelSize: root.sidebarAvatarSize
                }
                Ui.Button {
                  objectName: "buzzAgentOtherRow"
                  readonly property string agentId: modelData.id
                  Layout.fillWidth: true
                  clip: true
                  text: modelData.name + " · in " + root.agentService.communityNames(modelData)
                  tooltipText: modelData.name + " · enrolled in " + root.agentService.communityNames(modelData)
                    + (root.agentService.canAddToCurrent(modelData) ? " · open it to add it here" : " · switch to that community to manage it")
                  fontSize: Style.font.caption
                  leftAlign: true
                  focusable: true
                  selected: root.agentEditorShown && root.agentEditorId === modelData.id
                  onClicked: root.openAgentEditor(modelData.id)
                }
              }
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
              // Your own presence on the avatar's corner; the connection dot stays at the end.
              PresenceDot {
                objectName: "buzzAccountPresenceDot"
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.rightMargin: -Style.space(2)
                anchors.bottomMargin: -Style.space(2)
                service: root.service
                presence: root.service && (root.accountState === "online" || root.accountState === "sample") ? root.service.myPresence : ""
              }
            }
            Text {
              objectName: "buzzAccountName"
              Layout.fillWidth: !statusEmojiBadge.visible
              Layout.maximumWidth: accountRow.width - Style.space(40)
              text: root.accountName
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Text {
              id: statusEmojiBadge
              objectName: "buzzAccountStatusEmoji"
              visible: root.myStatusEmoji !== "" && (root.accountState === "online" || root.accountState === "sample")
              Layout.fillWidth: true
              text: root.myStatusEmoji
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
          readonly property string tooltipText: (root.myKeyKnown && !root.myNameKnown && root.accountState === "online"
              ? "No profile name on this community yet" : root.accountName) + " · " + root.accountStateLabel
            + (root.myPresenceLabel !== "" ? " · " + root.myPresenceLabel : "")
            + (statusEmojiBadge.visible ? " · " + root.myStatusLine : "")
            + (root.myKeyKnown ? " · " + root.service.identity.slice(0, 8) + "…" : "")
          Controls.ToolTip.visible: accountArea.containsMouse && !root.accountMenuOpen
          Controls.ToolTip.delay: 400
          Controls.ToolTip.text: PlainText.tip(tooltipText)
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
        service: root.service
        agentId: root.agentEditorId
        onBackRequested: root.backToRooms()
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
        // Back to rooms and the title are in the header (buzzHeaderBack).
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
                  text: "Pick an avatar from the gallery, paste your own art, or use a .ans or .txt file. It is kept locally, not published; others still see your identicon."
                }
              }
              AvatarFileLoader {
                id: myAvatarLoader
                service: root.service
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
              // Built-in ASCII avatars to browse, and a box to paste your own art into.
              AvatarGallery {
                Layout.fillWidth: true
                key: root.myAvatarAvailable ? root.service.identity : ""
                savedArt: root.myAvatarArt
                onSaveRequested: function(art) {
                  if (root.myAvatarAvailable) root.agentService.setOwnAvatarArt(root.service.identity, art, AnsiArt.DEFAULT_BRIGHTNESS)
                }
              }
            }
            SettingsNote {
              visible: !root.myAvatarAvailable
              text: root.service && root.service.sampleMode ? "Not available with sample data."
                : "Available once this device has a Buzz identity and the agent service is reachable."
            }

            // Communities: rename (a local label) and Leave community, like
            // Desktop's account menu; the last community cannot be left.
            SettingsCaption { visible: communitySettings.visible; text: "Communities" }
            ColumnLayout {
              id: communitySettings
              objectName: "buzzCommunitySettings"
              visible: !!root.service && root.service.communitiesShown
              Layout.fillWidth: true
              spacing: Style.space(4)
              Repeater {
                model: root.service ? root.service.communityEntries : []
                delegate: ColumnLayout {
                  id: communityRow
                  required property var modelData
                  required property int index
                  objectName: "buzzCommunityRow"
                  readonly property string relay: modelData.relay
                  readonly property bool renaming: root.renamingRelay === modelData.relay
                  readonly property bool leaving: root.leavingRelay === modelData.relay
                  Layout.fillWidth: true
                  spacing: Style.space(2)
                  RowLayout {
                    Layout.fillWidth: true
                    spacing: Style.space(6)
                    visible: !communityRow.renaming
                    ColumnLayout {
                      Layout.fillWidth: true
                      spacing: 0
                      Text {
                        objectName: "buzzCommunityRowName"
                        Layout.fillWidth: true
                        text: communityRow.modelData.name + (communityRow.modelData.active ? " · current" : "")
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                        font.bold: communityRow.modelData.active
                      }
                      Text {
                        objectName: "buzzCommunityRowHost"
                        Layout.fillWidth: true
                        text: communityRow.modelData.host + (communityRow.modelData.hint && communityRow.modelData.hint !== communityRow.modelData.name
                          ? " · calls itself " + communityRow.modelData.hint : "")
                        textFormat: Text.PlainText
                        elide: Text.ElideMiddle
                        color: Color.foreground
                        opacity: 0.6
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                      }
                    }
                    Ui.Button {
                      objectName: "buzzCommunityRename"
                      text: "Rename"
                      tooltipText: "Change the name this device shows for this community"
                      fontSize: Style.font.caption
                      focusable: true
                      enabled: !!root.service && root.service.communitiesAvailable && !root.service.communityBusy
                      opacity: enabled ? 1 : 0.5
                      onClicked: {
                        root.leavingRelay = ""
                        root.service.resetCommunityRequest()
                        root.renamingRelay = communityRow.relay
                        renameField.text = communityRow.modelData.name
                        renameField.forceActiveFocus()
                      }
                    }
                    Ui.Button {
                      objectName: "buzzCommunityLeave"
                      text: communityRow.leaving ? "Confirm leave" : "Leave community"
                      tooltipText: root.service && root.service.communityEntries.length <= 1
                        ? "You can't leave your only community" : "Leave this community; your identity stays on this device"
                      fontSize: Style.font.caption
                      focusable: true
                      selected: communityRow.leaving
                      enabled: !!root.service && root.service.canLeaveCommunity
                      opacity: enabled ? 1 : 0.5
                      onClicked: {
                        if (!communityRow.leaving) { root.service.resetCommunityRequest(); root.renamingRelay = ""; root.leavingRelay = communityRow.relay; return }
                        if (!root.service.leaveCommunity(communityRow.relay)) root.leavingRelay = ""
                      }
                    }
                    Ui.Button {
                      objectName: "buzzCommunityLeaveCancel"
                      visible: communityRow.leaving && !(root.service && root.service.communityBusy)
                      text: "Cancel"
                      fontSize: Style.font.caption
                      focusable: true
                      onClicked: root.leavingRelay = ""
                    }
                  }
                  RowLayout {
                    Layout.fillWidth: true
                    spacing: Style.space(6)
                    visible: communityRow.renaming
                    Ui.TextField {
                      id: renameField
                      objectName: "buzzCommunityRenameField"
                      Layout.fillWidth: true
                      verticalPadding: Style.space(4)
                      maximumLength: 64
                      placeholderText: "Community name"
                      onAccepted: root.service.renameCommunity(communityRow.relay, text)
                      Keys.onEscapePressed: function(event) { event.accepted = true; root.renamingRelay = "" }
                    }
                    Ui.Button {
                      objectName: "buzzCommunityRenameSave"
                      text: "Save"
                      fontSize: Style.font.caption
                      focusable: true
                      enabled: renameField.text.trim() !== "" && !!root.service && !root.service.communityBusy
                      opacity: enabled ? 1 : 0.5
                      onClicked: root.service.renameCommunity(communityRow.relay, renameField.text)
                    }
                    Ui.Button {
                      objectName: "buzzCommunityRenameCancel"
                      text: "Cancel"
                      fontSize: Style.font.caption
                      focusable: true
                      onClicked: root.renamingRelay = ""
                    }
                  }
                  SettingsNote {
                    objectName: "buzzCommunityLeaveNote"
                    visible: communityRow.leaving
                    text: "Leaving tells " + communityRow.modelData.host + " to end your membership. Your identity stays on this device, and you can join again with a new invite."
                  }
                }
              }
              SettingsNote {
                objectName: "buzzCommunityLastNote"
                visible: !!root.service && root.service.communityEntries.length === 1
                text: "You can't leave your only community. Join another one first."
              }
              SettingsNote {
                visible: !!root.service && root.service.sampleMode
                text: "Not available with sample data."
              }
              SettingsNote {
                objectName: "buzzCommunitySettingsStatus"
                visible: text !== ""
                text: !root.service ? "" : root.service.communityNoticeLabel !== "" ? root.service.communityNoticeLabel
                  : ["rename", "leave"].indexOf(root.service.communityRequestKind) !== -1 ? root.service.communityLabel : ""
              }
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
            Flow {
              Layout.fillWidth: true
              spacing: Style.space(8)
              Repeater {
                model: [{mode: "direct", label: "Mentions & DMs", note: "Mentions of you, direct messages and replies in threads you are in"},
                  {mode: "mentions", label: "Mentions", note: "Mentions of you and replies in threads you are in"},
                  {mode: "dms", label: "DMs", note: "Direct messages only"},
                  {mode: "all", label: "All activity", note: "Every new message observed in your rooms"},
                  {mode: "none", label: "None", note: "No desktop notifications"}]
                Ui.Button {
                  required property var modelData
                  objectName: "buzzNotify_" + modelData.mode
                  text: modelData.label
                  tooltipText: modelData.note
                  fontSize: Style.font.caption
                  focusable: true
                  selected: !!root.service && root.service.notificationMode === modelData.mode
                  enabled: !!root.service
                  onClicked: if (root.service) root.service.notificationMode = modelData.mode
                }
              }
              Ui.Button {
                objectName: "buzzSettingsNotifyText"
                text: root.service && root.service.notificationText ? "Message text: on" : "Message text: off"
                tooltipText: "Show the message text in notifications, or only who wrote"
                fontSize: Style.font.caption
                focusable: true
                selected: !!root.service && root.service.notificationText
                enabled: !!root.service && root.service.notificationMode !== "none"
                onClicked: if (root.service) root.service.notificationText = !root.service.notificationText
              }
            }
            SettingsNote {
              text: "Titled by sender and room; clicking opens that room. Not shown while that room is open in the focused panel. Counts such as \u201c3 new messages\u201d are what this session observed, not Buzz unread counts. Best-effort."
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
        id: statusView
        objectName: "buzzStatusView"
        visible: root.statusOpen
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        spacing: Style.space(8)
        // Back to rooms and the title are in the header (buzzHeaderBack).
        ColumnLayout {
          Layout.fillWidth: true
          spacing: Style.space(4)
          SettingsNote {
            objectName: "buzzStatusCurrent"
            text: root.myStatusLine !== "" ? "Now: " + root.myStatusLine : "No status set."
          }
          SettingsNote {
            visible: !!root.service && root.service.sampleMode
            text: "Not available with sample data."
          }
          SettingsNote {
            objectName: "buzzStatusOffline"
            visible: !!root.service && !root.service.sampleMode && !root.service.userStatusAvailable
            text: "Connect to your relay to change your status."
          }
          SettingsCaption { text: "Status" }
          RowLayout {
            Layout.fillWidth: true
            spacing: Style.space(6)
            Ui.TextField {
              id: statusEmojiField
              objectName: "buzzStatusEmojiField"
              Layout.preferredWidth: Style.space(70)
              verticalPadding: Style.space(4)
              maximumLength: 64
              placeholderText: "Emoji"
              inputMethodHints: Qt.ImhNoPredictiveText
              onAccepted: root.submitStatus()
            }
            Ui.TextField {
              id: statusField
              objectName: "buzzStatusText"
              Layout.fillWidth: true
              verticalPadding: Style.space(4)
              maximumLength: 200
              placeholderText: "What's your status?"
              onAccepted: root.submitStatus()
            }
          }
          Flow {
            objectName: "buzzStatusChips"
            Layout.fillWidth: true
            spacing: Style.space(4)
            Repeater {
              model: root.service ? root.service.statusPresetEmoji : []
              Ui.Button {
                required property string modelData
                objectName: "buzzStatusChip"
                readonly property string emoji: modelData
                text: modelData
                tooltipText: "Use " + modelData
                fontSize: Style.font.caption
                horizontalPadding: Style.space(6)
                verticalPadding: Style.space(2)
                focusable: true
                selected: root.statusDraftEmoji === modelData
                onClicked: root.chooseStatusEmoji(modelData)
              }
            }
          }
          SettingsCaption { text: "Clear after" }
          RowLayout {
            Layout.fillWidth: true
            spacing: Style.space(4)
            Repeater {
              model: root.service ? root.service.statusDurations : []
              Ui.Button {
                required property int modelData
                objectName: "buzzStatusHours" + modelData
                text: ({1: "1 hour", 4: "4 hours", 24: "1 day", 168: "1 week"})[modelData]
                tooltipText: "When this status clears itself"
                fontSize: Style.font.caption
                focusable: true
                selected: root.statusHours === modelData
                onClicked: root.statusHours = modelData
              }
            }
            Item { Layout.fillWidth: true }
          }
          SettingsNote {
            objectName: "buzzStatusDraftNote"
            visible: text !== ""
            text: root.statusDraftNote
          }
          RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Style.space(4)
            spacing: Style.space(6)
            Ui.Button {
              objectName: "buzzStatusSet"
              text: "Set status"
              tooltipText: "Publish this status to your relay"
              fontSize: Style.font.caption
              focusable: true
              enabled: !!root.service && root.service.canSetStatus && root.statusDraftValid
              opacity: enabled ? 1 : 0.5
              onClicked: root.submitStatus()
            }
            Ui.Button {
              objectName: "buzzStatusClear"
              text: "Clear status"
              tooltipText: "Remove your status"
              fontSize: Style.font.caption
              focusable: true
              enabled: !!root.service && root.service.canSetStatus && !!root.service.myStatus
              opacity: enabled ? 1 : 0.5
              onClicked: root.submitClearStatus()
            }
            Item { Layout.fillWidth: true }
          }
          SettingsNote {
            objectName: "buzzStatusMessage"
            visible: text !== ""
            text: root.service ? root.service.userStatusLabel : ""
          }
          SettingsNote {
            text: "Everyone in your community can see your status. It clears itself after the time you choose."
          }
        }
        Item { Layout.fillHeight: true }
      }

      // Join an existing community / Create a new community (account menu).
      // Back to rooms and the title are in the header (buzzHeaderBack).
      ColumnLayout {
        id: communityJoinView
        objectName: "buzzCommunityJoinView"
        visible: root.communityView === "join"
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        spacing: Style.space(8)
        SettingsNote {
          objectName: "buzzCommunityJoinDescription"
          text: root.service ? root.service.joinCommunityDescription : ""
        }
        SettingsNote {
          visible: !!root.service && root.service.sampleMode
          text: "Not available with sample data."
        }
        CommunityJoinForm {
          id: communityJoinForm
          service: root.service
          visible: !root.communityAwaiting
        }
        CommunityTerms {
          service: root.service
          visible: root.communityAwaiting && policy !== null
        }
        Item { Layout.fillHeight: true }
      }
      ColumnLayout {
        id: communityCreateView
        objectName: "buzzCommunityCreateView"
        visible: root.communityView === "create"
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        spacing: Style.space(8)
        CommunityCreateSteps {
          service: root.service
          opener: root.linkOpener
          visible: !root.communityAwaiting
        }
        CommunityTerms {
          service: root.service
          visible: root.communityAwaiting && policy !== null
        }
        Item { Layout.fillHeight: true }
      }

      // New room: name, optional description, open or private. The helper
      // signs; the relay answers; the list then selects the new room.
      ColumnLayout {
        id: newRoomView
        objectName: "buzzNewRoomView"
        visible: root.communityView === "new-room"
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        spacing: Style.space(8)
        SettingsNote {
          text: "Anyone in this community can join an open room. A private room is joined only by people you add."
        }
        SettingsCaption { text: "Name" }
        Ui.TextField {
          id: newRoomName
          objectName: "buzzNewRoomName"
          Layout.fillWidth: true
          verticalPadding: Style.space(4)
          maximumLength: 128
          placeholderText: "Room name"
          onAccepted: root.submitNewRoom()
        }
        SettingsCaption { text: "Description (optional)" }
        Ui.TextField {
          id: newRoomAbout
          objectName: "buzzNewRoomAbout"
          Layout.fillWidth: true
          verticalPadding: Style.space(4)
          maximumLength: 512
          placeholderText: "What is this room for?"
          onAccepted: root.submitNewRoom()
        }
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.space(6)
          Ui.Button {
            objectName: "buzzNewRoomOpen"
            text: "Open"
            tooltipText: "Anyone in this community can join"
            fontSize: Style.font.caption
            focusable: true
            selected: !root.newRoomPrivate
            onClicked: root.newRoomPrivate = false
          }
          Ui.Button {
            objectName: "buzzNewRoomPrivate"
            text: "Private"
            tooltipText: "Only people you add can join"
            fontSize: Style.font.caption
            focusable: true
            selected: root.newRoomPrivate
            onClicked: root.newRoomPrivate = true
          }
          Item { Layout.fillWidth: true }
          Ui.Button {
            objectName: "buzzNewRoomCreate"
            text: "Create room"
            tooltipText: "Create this room"
            focusable: true
            enabled: root.newRoomValid && !!root.service && root.service.canManageRooms
            opacity: enabled ? 1 : 0.5
            onClicked: root.submitNewRoom()
          }
        }
        Text {
          objectName: "buzzNewRoomStatus"
          Layout.fillWidth: true
          visible: text !== ""
          text: root.changeLine(["create"])
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
        Item { Layout.fillHeight: true }
      }
      // Room settings: everyone sees the topic and the members; owners and
      // admins (by the relay's roster) edit details and manage members. The
      // relay decides, and a refusal is shown in its own words.
      ColumnLayout {
        id: roomSettingsView
        objectName: "buzzRoomSettingsView"
        visible: root.communityView === "room-settings"
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        spacing: Style.space(8)
        Controls.ScrollView {
          Layout.fillWidth: true
          Layout.fillHeight: true
          contentWidth: availableWidth
          clip: true
          ColumnLayout {
            width: parent.width
            spacing: Style.space(4)
            Text {
              objectName: "buzzRoomSettingsTitle"
              Layout.fillWidth: true
              text: root.service ? root.service.roomTitle(root.service.selectedRoom) : ""
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.body * 1.15
              font.bold: true
            }
            SettingsNote {
              objectName: "buzzRoomDetailStatus"
              visible: text !== ""
              text: root.service ? root.service.roomDetailLabel : ""
            }
            SettingsCaption { text: "Topic" }
            SettingsNote {
              objectName: "buzzRoomTopic"
              visible: !!root.service && root.service.roomDetailShown
              text: root.service && root.service.roomDetailShown
                ? (root.service.roomDetail.topic !== "" ? root.service.roomDetail.topic : "No topic set.") : ""
            }
            SettingsNote {
              objectName: "buzzRoomVisibility"
              visible: !!root.service && root.service.roomDetailShown
              text: root.service && root.service.roomDetailShown
                ? (root.service.roomDetail.visibility === "private" ? "Private room · only people added to it can join" : "Open room · anyone in this community can join") : ""
            }
            SettingsNote {
              objectName: "buzzRoomEditNote"
              visible: !!root.service && root.service.roomDetailShown && !root.service.canEditRoom
              text: root.service && root.service.roomDetailShown && !root.service.canEditRoom
                ? "Only this room's owners and admins change its name, description, topic and members." : ""
            }
            ColumnLayout {
              objectName: "buzzRoomEdit"
              visible: !!root.service && root.service.canEditRoom
              Layout.fillWidth: true
              spacing: Style.space(4)
              SettingsCaption { text: "Name and description" }
              Ui.TextField {
                id: editRoomName
                objectName: "buzzRoomEditName"
                Layout.fillWidth: true
                verticalPadding: Style.space(4)
                maximumLength: 128
                placeholderText: "Room name"
              }
              Ui.TextField {
                id: editRoomAbout
                objectName: "buzzRoomEditAbout"
                Layout.fillWidth: true
                verticalPadding: Style.space(4)
                maximumLength: 512
                placeholderText: "Description"
              }
              Ui.Button {
                objectName: "buzzRoomSaveDetails"
                text: "Save name and description"
                tooltipText: "Change this room's name and description"
                fontSize: Style.font.caption
                focusable: true
                enabled: !!root.service && root.service.canEditRoom && root.service.validRoomText(editRoomName.text, 128, true)
                  && root.service.validRoomText(editRoomAbout.text, 512, false)
                opacity: enabled ? 1 : 0.5
                onClicked: root.service.updateRoomDetails(editRoomName.text, editRoomAbout.text)
              }
              SettingsCaption { text: "Change topic" }
              Ui.TextField {
                id: editRoomTopic
                objectName: "buzzRoomEditTopic"
                Layout.fillWidth: true
                verticalPadding: Style.space(4)
                maximumLength: 256
                placeholderText: "Topic"
                onTextEdited: root.topicTouched = true
              }
              Ui.Button {
                objectName: "buzzRoomSaveTopic"
                text: "Save topic"
                tooltipText: "Change this room's topic"
                fontSize: Style.font.caption
                focusable: true
                enabled: !!root.service && root.service.canEditRoom && root.service.validRoomText(editRoomTopic.text, 256, false)
                opacity: enabled ? 1 : 0.5
                onClicked: root.service.setRoomTopic(editRoomTopic.text)
              }
            }
            SettingsCaption {
              visible: !!root.service && root.service.roomDetailShown
              text: root.service && root.service.roomDetailShown
                ? "Members · " + root.service.roomDetail.members.length + (root.service.roomDetail.truncated ? " shown, more not listed" : "") : ""
            }
            Repeater {
              model: root.service && root.service.roomDetailShown ? root.service.roomDetail.members : []
              delegate: RowLayout {
                required property var modelData
                objectName: "buzzRoomMember"
                readonly property string memberKey: modelData.key
                Layout.fillWidth: true
                spacing: Style.space(4)
                Text {
                  objectName: "buzzRoomMemberName"
                  Layout.fillWidth: true
                  text: root.memberLabel(modelData)
                  textFormat: Text.PlainText
                  elide: Text.ElideRight
                  color: Color.foreground
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                }
                Ui.Button {
                  objectName: "buzzRoomRemoveMember"
                  visible: !!root.service && root.service.canEditRoom && modelData.key !== root.service.identity
                  text: root.armedMember === modelData.key ? "Confirm remove" : "Remove"
                  tooltipText: "Remove this member from the room"
                  fontSize: Style.font.caption
                  horizontalPadding: Style.space(6)
                  verticalPadding: Style.space(2)
                  focusable: true
                  selected: root.armedMember === modelData.key
                  onClicked: root.submitMemberRemoval(modelData.key)
                }
              }
            }
            ColumnLayout {
              objectName: "buzzRoomAdd"
              visible: !!root.service && root.service.canEditRoom
              Layout.fillWidth: true
              spacing: Style.space(4)
              SettingsCaption { text: "Add a member" }
              RowLayout {
                Layout.fillWidth: true
                spacing: Style.space(6)
                Ui.TextField {
                  id: addMemberKey
                  objectName: "buzzRoomAddKey"
                  Layout.fillWidth: true
                  verticalPadding: Style.space(4)
                  maximumLength: 80
                  placeholderText: "Member's public key (64 hex characters)"
                  inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhPreferLowercase
                  onAccepted: if (root.service.addRoomMember(text)) text = ""
                }
                Ui.Button {
                  objectName: "buzzRoomAddMember"
                  text: "Add"
                  tooltipText: "Add this key to the room"
                  fontSize: Style.font.caption
                  focusable: true
                  enabled: !!root.service && root.service.canEditRoom && /^[A-Fa-f0-9]{64}$/.test(addMemberKey.text.trim())
                  opacity: enabled ? 1 : 0.5
                  onClicked: if (root.service.addRoomMember(addMemberKey.text)) addMemberKey.text = ""
                }
              }
              SettingsNote {
                visible: candidates.count > 0
                text: "People you message"
              }
              Repeater {
                id: candidates
                model: root.service ? root.service.dmPeople.filter(function(person) {
                  return !root.service.roomDetail.members.some(function(m) { return m.key === person.key })
                }).slice(0, 8) : []
                delegate: RowLayout {
                  required property var modelData
                  objectName: "buzzRoomAddCandidate"
                  Layout.fillWidth: true
                  spacing: Style.space(4)
                  Text {
                    Layout.fillWidth: true
                    text: modelData.name + " · " + modelData.key.slice(0, 8)
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                  }
                  Ui.Button {
                    objectName: "buzzRoomAddCandidateButton"
                    text: "Add"
                    tooltipText: "Add this person to the room"
                    fontSize: Style.font.caption
                    horizontalPadding: Style.space(6)
                    verticalPadding: Style.space(2)
                    focusable: true
                    enabled: !!root.service && root.service.canEditRoom
                    opacity: enabled ? 1 : 0.5
                    onClicked: root.service.addRoomMember(modelData.key)
                  }
                }
              }
            }
            Text {
              objectName: "buzzRoomManageStatus"
              Layout.fillWidth: true
              visible: text !== ""
              text: root.changeLine(["details", "topic", "add_member", "remove_member"])
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
          }
        }
      }

      ColumnLayout {
        visible: root.showTimeline && !root.agentEditorShown && !root.settingsOpen && !root.statusOpen && root.communityView === ""
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: Style.space(400)
        spacing: Style.space(8)
        // You joined … / Switched to …: dismissible, and gone after a few seconds.
        Rectangle {
          objectName: "buzzCommunityArrival"
          visible: root.arrivalText !== ""
          Layout.fillWidth: true
          implicitHeight: arrivalRow.implicitHeight + Style.space(10)
          radius: Style.cornerRadius
          color: Util.alpha("#3fb950", 0.14)
          border.color: Util.alpha("#3fb950", 0.5)
          border.width: Math.max(1, Style.space(1))
          RowLayout {
            id: arrivalRow
            anchors.fill: parent
            anchors.leftMargin: Style.space(8)
            anchors.rightMargin: Style.space(4)
            spacing: Style.space(6)
            Text {
              objectName: "buzzCommunityJoined"
              Layout.fillWidth: true
              text: root.arrivalText
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.body
            }
            Ui.Button {
              objectName: "buzzCommunityJoinedDismiss"
              text: "✕"
              tooltipText: "Dismiss"
              horizontalPadding: Style.space(6)
              verticalPadding: Style.space(1)
              focusable: true
              onClicked: root.dismissArrival()
            }
          }
        }
        RowLayout {
          visible: !root.welcomeShown
          Layout.fillWidth: true
          spacing: Style.space(8)
          Text {
            objectName: "buzzRoomTitle"
            // "Connect Buzz" belongs to setup only; a connected community
            // without rooms shows the welcome pane instead of this row.
            text: root.service && root.service.selectedRoom ? root.service.roomTitle(root.service.selectedRoom) : root.connected ? "" : "Connect Buzz"
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
            Controls.ToolTip.text: PlainText.tip(text)
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
            objectName: "buzzRoomSettings"
            text: "Room settings"
            tooltipText: "Topic, members and, for owners and admins, changes"
            fontSize: Style.font.caption
            horizontalPadding: Style.space(6)
            verticalPadding: Style.space(2)
            focusable: true
            visible: root.roomManageShown && root.service.connection === "authenticated" && root.service.selectedRoom !== null
              && root.service.selectedRoom.kind === "stream"
            onClicked: root.openRoomSettings()
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
          // First setup in Desktop's order: Join an existing community, or
          // Create a new community (at buzz.xyz), with the same components as
          // the views opened from the account menu.
          RowLayout {
            spacing: Style.space(8)
            Ui.Button {
              objectName: "buzzSetupJoinOption"
              text: "Join an existing community"
              tooltipText: "Use a community URL or invite link"
              selected: !!root.service && root.service.setupProvider === "join"
              focusable: true
              onClicked: if (root.service) root.service.chooseSetupProvider("join")
            }
            Ui.Button {
              objectName: "buzzSetupCreateOption"
              text: "Create a new community"
              tooltipText: "Claim a Buzz address for your team at buzz.xyz"
              selected: !!root.service && root.service.setupProvider === "create"
              focusable: true
              onClicked: if (root.service) root.service.chooseSetupProvider("create")
            }
          }
          Text {
            objectName: "buzzSetupInstructions"
            Layout.fillWidth: true
            text: root.service ? root.service.setupInstructions : "Enable the plugin and reopen this panel."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Color.foreground
            opacity: 0.7
            font.family: Style.font.family
            font.pixelSize: Style.font.body
          }
          // Only with a helper that offers `setup_assist` and is not
          // authenticated; older helpers keep the terminal instructions.
          ColumnLayout {
            objectName: "buzzSetupJoin"
            Layout.fillWidth: true
            spacing: Style.space(4)
            visible: !!root.service && root.service.setupAssistAvailable && root.service.setupProvider === "join"
            SettingsNote {
              objectName: "buzzSetupJoinDescription"
              text: root.service ? root.service.joinCommunityDescription : ""
            }
            CommunityJoinForm {
              id: setupJoinForm
              service: root.service
              fieldName: "buzzSetupRelayUrl"
              buttonName: "buzzSetupRelay"
              statusName: "buzzSetupJoinStatus"
              Component.onCompleted: if (root.service && root.service.relay) field.text = root.service.relay
            }
          }
          CommunityCreateSteps {
            objectName: "buzzSetupCreate"
            visible: !!root.service && root.service.setupProvider === "create"
            service: root.service
            opener: root.linkOpener
            prefix: "buzzSetupCreate"
          }
          Connections {
            target: root.service
            // Show the helper's saved relay once, without overwriting typing.
            function onRelayChanged() { if (root.service.relay && !setupJoinForm.field.activeFocus) setupJoinForm.field.text = root.service.relay }
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
        // Welcome to a community with no joined room yet: what to do next.
        Controls.ScrollView {
          id: welcomeScroll
          objectName: "buzzWelcomePane"
          visible: root.welcomeShown
          Layout.fillWidth: true
          Layout.fillHeight: true
          Layout.preferredHeight: Style.space(80)
          contentWidth: availableWidth
          clip: true
          ColumnLayout {
            width: welcomeScroll.availableWidth
            spacing: Style.space(8)
            Text {
              objectName: "buzzWelcomeTitle"
              Layout.fillWidth: true
              Layout.topMargin: Style.space(6)
              text: "Welcome to " + root.welcomeName
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.body * 1.4
              font.bold: true
            }
            Text {
              objectName: "buzzWelcomeHost"
              Layout.fillWidth: true
              Layout.topMargin: -Style.space(6)
              visible: root.communityHost !== "" && root.communityHost !== root.welcomeName
              text: root.communityHost
              textFormat: Text.PlainText
              elide: Text.ElideMiddle
              color: Color.foreground
              opacity: 0.6
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
            Text {
              objectName: "buzzWelcomeLead"
              Layout.fillWidth: true
              text: "Pick a room to get started."
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: Color.foreground
              font.family: Style.font.family
              font.pixelSize: Style.font.body
            }
            JoinCommunity {
              objectName: "buzzWelcomeRooms"
              service: root.service
              prefix: "buzzWelcome"
              showInvite: false
              showOpenRooms: true
            }
            Rectangle {
              Layout.fillWidth: true
              Layout.topMargin: Style.space(4)
              implicitHeight: Math.max(1, Style.space(1))
              color: Util.alpha(Color.foreground, 0.14)
            }
            JoinCommunity {
              objectName: "buzzWelcomeInvite"
              visible: !!root.service && root.service.joinAvailable
              service: root.service
              prefix: "buzzWelcome"
              inviteCaption: "Have an invite? Paste it below"
            }
            Ui.Button {
              objectName: "buzzWelcomeSwitch"
              visible: !!root.service && root.service.communitiesShown
              text: "Switch community"
              tooltipText: "Open the account menu to choose or join another community"
              fontSize: Style.font.caption
              horizontalPadding: Style.space(4)
              verticalPadding: Style.space(2)
              opacity: 0.8
              focusable: true
              onClicked: root.openAccountMenu()
            }
          }
        }
        BuzzScroll {
          id: historyScroll
          objectName: "buzzHistoryScroll"
          visible: !root.welcomeShown
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
          onEscaped: root.escapeKey()
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
        // Communities, like Desktop's account menu: the current one, the others
        // to switch to, then Join an existing community and Create a new one.
        ColumnLayout {
          objectName: "buzzAccountCommunities"
          Layout.fillWidth: true
          spacing: Style.space(2)
          Text {
            Layout.fillWidth: true
            text: "Communities"
            textFormat: Text.PlainText
            color: Color.foreground
            opacity: 0.6
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
          Item {
            objectName: "buzzAccountCommunity"
            Layout.fillWidth: true
            implicitHeight: menuCommunityRow.implicitHeight
            readonly property string tooltipText: root.service && root.service.activeCommunity
              ? root.service.activeCommunity.relay : root.service && root.service.relay ? root.service.relay : "No community yet"
            Controls.ToolTip.visible: communityHover.containsMouse
            Controls.ToolTip.delay: 400
            Controls.ToolTip.text: PlainText.tip(tooltipText)
            MouseArea { id: communityHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
            RowLayout {
              id: menuCommunityRow
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
                  objectName: "buzzAccountCommunityName"
                  Layout.fillWidth: true
                  text: root.service && root.service.activeCommunity ? "✓ " + root.service.activeCommunity.name : "Community"
                  textFormat: Text.PlainText
                  elide: Text.ElideRight
                  color: Color.foreground
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                  font.bold: !!root.service && !!root.service.activeCommunity
                }
                Text {
                  objectName: "buzzAccountCommunityHost"
                  Layout.fillWidth: true
                  text: root.service && root.service.activeCommunity ? root.service.activeCommunity.host
                    : root.communityHost !== "" ? root.communityHost : root.service && root.service.sampleMode ? root.service.viewModel.community : "No relay"
                  textFormat: Text.PlainText
                  elide: Text.ElideMiddle
                  color: Color.foreground
                  opacity: 0.6
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                }
              }
            }
          }
          Repeater {
            model: root.service ? root.service.otherCommunities : []
            Ui.Button {
              required property var modelData
              required property int index
              objectName: "buzzCommunitySwitch"
              readonly property string relay: modelData.relay
              Layout.fillWidth: true
              clip: true
              text: modelData.name + " · " + modelData.host
              tooltipText: "Switch to this community; your identity stays the same"
              fontSize: Style.font.caption
              leftAlign: true
              focusable: true
              enabled: !!root.service && root.service.canSwitchCommunity
              opacity: enabled ? 1 : 0.5
              onClicked: root.switchCommunity(relay)
            }
          }
          Text {
            objectName: "buzzAccountCommunityStatus"
            Layout.fillWidth: true
            visible: text !== ""
            text: root.service && root.service.communityRequestKind === "switch" ? root.service.communityLabel : ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
          Ui.Button {
            objectName: "buzzAccountJoinCommunity"
            visible: !!root.service && root.service.communitiesShown
            Layout.fillWidth: true
            text: "Join an existing community…"
            tooltipText: "Use a community URL or invite link"
            fontSize: Style.font.caption
            leftAlign: true
            focusable: true
            onClicked: root.openCommunityView("join")
          }
          Ui.Button {
            objectName: "buzzAccountCreateCommunity"
            visible: !!root.service && root.service.communitiesShown
            Layout.fillWidth: true
            text: "Create a new community…"
            tooltipText: "Claim a Buzz address for your team at buzz.xyz"
            fontSize: Style.font.caption
            leftAlign: true
            focusable: true
            onClicked: root.openCommunityView("create")
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
          objectName: "buzzAccountStatus"
          visible: root.statusEntryShown
          Layout.fillWidth: true
          clip: true
          text: root.myStatusLine !== "" ? root.myStatusLine : "☺ Update your status"
          tooltipText: root.myStatusLine !== "" ? "Change or clear your status" : "Tell people what you are up to"
          fontSize: Style.font.caption
          leftAlign: true
          focusable: true
          onClicked: root.openStatus()
        }
        // Set yourself as…: Desktop's presence preference. The helper derives
        // and publishes the state; Auto follows idleness.
        ColumnLayout {
          objectName: "buzzPresenceRow"
          visible: root.presenceEntryShown
          Layout.fillWidth: true
          spacing: Style.space(2)
          Text {
            Layout.fillWidth: true
            text: "Set yourself as…"
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Color.foreground
            opacity: 0.6
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
          RowLayout {
            Layout.fillWidth: true
            spacing: Style.space(4)
            Repeater {
              model: root.service ? root.service.presenceModes : []
              Ui.Button {
                required property string modelData
                objectName: "buzzPresenceMode_" + modelData
                readonly property string mode: modelData
                readonly property bool current: !!root.service && root.service.presenceMode === modelData
                text: (current ? "✓ " : "") + root.service.presenceModeLabels[modelData]
                tooltipText: ({auto: "Online while you are active, away after 10 minutes idle",
                  away: "Show as away", offline: "Appear offline to others"})[modelData]
                fontSize: Style.font.caption
                horizontalPadding: Style.space(6)
                verticalPadding: Style.space(2)
                focusable: true
                selected: current
                onClicked: root.service.setPresenceMode(modelData)
              }
            }
            Item { Layout.fillWidth: true }
          }
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
