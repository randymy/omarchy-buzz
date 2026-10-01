import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons
import "Identicon.js" as Identicon
import "AnsiArt.js" as AnsiArt

// Editor for one agent persona in the middle column. It edits a local draft and
// sends only structured, locally validated requests through the agent service.
ColumnLayout {
  id: root
  property var agents: null
  property var service: null
  // Empty for a new agent, otherwise the service's persona id.
  property string agentId: ""
  signal backRequested()
  signal agentChosen(string agentId)

  readonly property bool creating: agentId === ""
  readonly property var entry: agents && !creating ? agents.agent(agentId) : null
  // An agent is enrolled in one or more communities (its instances). It is
  // edited here through its instance in the current community; an agent with
  // none here is shown read-only (its rooms are not this community's, and the
  // service checks rooms only in the active community) and may be added here.
  readonly property var instance: agents && entry ? agents.currentInstance(entry) : null
  readonly property bool readOnly: !!entry && !instance
  // Only the first community's workspace is edited here; the others use their default.
  readonly property bool workspaceEditable: creating || (!!instance && agents.isFirstInstance(entry, instance))
  readonly property string communityName: instance ? instance.community : entry ? entry.community : agents ? agents.currentCommunityName : ""
  readonly property var roomChoices: agents ? agents.roomChoicesFor(entry) : []
  readonly property bool canAdd: !!agents && !!entry && agents.canAddToCurrent(entry)
  // Rooms chosen for "Add to <community>".
  property var addRooms: []
  // The instance whose Leave is armed (its relay), or "".
  property string leaveArmed: ""
  property string draftName: ""
  property string draftDescription: ""
  property string draftInstructions: ""
  property string draftHarness: "codex"
  property string draftModel: ""
  property var draftRooms: []
  property string draftRespondTo: "owner-only"
  property string draftWorkspace: ""
  property bool draftStartAtLogin: false
  property bool draftAnswersDms: false
  // Pasted avatar art: kept on this machine only, never sent to the agent service.
  property string draftArt: ""
  property string pendingCreateArt: ""
  // Brightness of colored art (0 for plain art); saved with the art.
  property real draftBrightness: 0
  property real pendingCreateBrightness: 0
  property bool deleteArmed: false
  readonly property alias artPreview: artPreview

  function load() {
    deleteArmed = false
    leaveArmed = ""
    addRooms = []
    var source = entry && agents ? agents.savedFields(entry) : entry
    draftName = source ? source.name : ""
    draftDescription = source ? source.description : ""
    draftInstructions = source ? source.instructions : ""
    draftHarness = source ? source.harness : "codex"
    draftModel = source ? source.model : ""
    draftRooms = source ? source.rooms.slice() : []
    draftRespondTo = source ? source.respondTo : "owner-only"
    draftWorkspace = source ? source.workspace : ""
    draftStartAtLogin = source ? source.startAtLogin : false
    draftAnswersDms = source ? source.answersDms : false
    draftArt = source ? agents.avatarArtFor(source.id) : ""
    draftBrightness = source ? agents.avatarBrightnessFor(source.id) : 0
  }
  onAgentIdChanged: load()
  // Fields follow the draft when it is loaded; typing updates the draft.
  onDraftNameChanged: if (nameField.text !== draftName) nameField.text = draftName
  onDraftDescriptionChanged: if (descriptionField.text !== draftDescription) descriptionField.text = draftDescription
  onDraftInstructionsChanged: if (instructions.text !== draftInstructions) instructions.text = draftInstructions
  onDraftModelChanged: if (modelField.text !== draftModel) modelField.text = draftModel
  onDraftWorkspaceChanged: if (workspaceField.text !== draftWorkspace) workspaceField.text = draftWorkspace
  // Colored art loaded from a file is not typed: the field keeps plain art only.
  onDraftArtChanged: if (!AnsiArt.isAnsi(draftArt) && artField.text !== draftArt) artField.text = draftArt
  Component.onCompleted: load()

  readonly property var draftFields: {
    var fields = {name: draftName, description: draftDescription, instructions: draftInstructions,
      harness: draftHarness, model: draftModel, rooms: draftRooms, respondTo: draftRespondTo, workspace: draftWorkspace,
      answersDms: draftAnswersDms}
    if (!workspaceEditable) delete fields.workspace
    return fields
  }
  // The service stops a running agent before saving these; it runs again on
  // Start. Harness and answering apply in every community.
  readonly property bool stopsOnSave: !!entry && (
    (!!instance && instance.state === "active" && ["workspace", "rooms"].some(function(key) { return root.changedFields.hasOwnProperty(key) }))
    || (entry.instances.some(function(i) { return i.state === "active" })
      && ["harness", "respondTo", "answersDms"].some(function(key) { return root.changedFields.hasOwnProperty(key) })))
  // Only what differs from the saved persona (this community's rooms) is sent on update.
  readonly property var changedFields: {
    if (creating || !entry) return ({})
    var changed = {}
    var fields = draftFields
    var saved = agents.savedFields(entry)
    Object.keys(fields).forEach(function(key) {
      if (JSON.stringify(fields[key]) !== JSON.stringify(saved[key])) changed[key] = fields[key]
    })
    return changed
  }
  readonly property var saveFields: {
    if (!creating) return changedFields
    var fields = Object.assign({}, draftFields)
    fields.startAtLogin = draftStartAtLogin
    return fields
  }
  readonly property string problem: agents ? agents.fieldsProblem(saveFields, creating, entry ? agents.savedFields(entry) : null) : ""
  readonly property bool serviceChanged: Object.keys(changedFields).length > 0
  readonly property real shownBrightness: AnsiArt.isAnsi(draftArt) ? draftBrightness : 0
  readonly property bool artChanged: !creating && !!entry && (AnsiArt.storedArt(draftArt) !== agents.avatarArtFor(agentId)
    || shownBrightness !== agents.avatarBrightnessFor(agentId))
  readonly property bool dirty: creating || serviceChanged || artChanged
  // Art alone is saved locally and needs no agent service request.
  readonly property bool canSave: !!agents && (creating || !!entry) && !readOnly
    && (creating || serviceChanged ? agents.canMutate && problem === "" : artChanged)
  readonly property var harnessState: agents ? agents.harness(entry ? entry.harness : draftHarness) : null
  readonly property bool bundleStale: !!harnessState && harnessState.bundle === "stale"
  // The probe tests the saved model; an unsaved model or harness is saved first.
  readonly property bool modelUnsaved: changedFields.hasOwnProperty("model") || changedFields.hasOwnProperty("harness")
  readonly property var probe: agents && entry ? agents.probeFor(agentId) : null
  readonly property bool canProbe: !!agents && !!entry && !modelUnsaved && agents.canMutate && agents.canProbeModel(agentId)
  readonly property string probeCaption: {
    if (!entry) return ""
    if (probe && probe.model === entry.model && probe.state === "running") return "Testing " + probe.model + "…"
    if (probe && probe.model === entry.model && probe.detail) return probe.model + ": " + probe.detail
    if (modelUnsaved) return "Save to test this model."
    if (entry.model === "") return "Set a model to test it; the harness default is not tested."
    if (!harnessState || harnessState.bundle !== "ready") return "The harness bundle must be ready to test a model."
    if (harnessState.signedIn !== true) return "Sign in to " + agents.harnessLabel(harnessState.id) + " to test a model."
    return ""
  }

  function save() {
    if (!canSave) return false
    if (creating) {
      pendingCreateArt = AnsiArt.storedArt(draftArt)
      pendingCreateBrightness = shownBrightness
      return agents.createAgent(saveFields)
    }
    if (artChanged && !agents.setAvatarArt(agentId, draftArt, shownBrightness > 0 ? shownBrightness : undefined)) return false
    return serviceChanged ? agents.updateAgent(agentId, saveFields) : true
  }
  function toggleRoom(id) {
    var copy = draftRooms.slice()
    var index = copy.indexOf(id)
    if (index !== -1) copy.splice(index, 1)
    else if (copy.length < 8 && roomChoices.some(function(room) { return room.id === id })) copy.push(id)
    else return false
    draftRooms = copy
    return true
  }
  function toggleAddRoom(id) {
    var copy = addRooms.slice()
    var index = copy.indexOf(id)
    if (index !== -1) copy.splice(index, 1)
    else if (copy.length < 8 && agents && agents.roomChoices.some(function(room) { return room.id === id })) copy.push(id)
    else return false
    addRooms = copy
    return true
  }
  function addToCommunity() {
    if (!canAdd || !agents.canMutate) return false
    return agents.enrollAgentIn(agentId, addRooms)
  }
  // Leaving a community is confirmed by a second click.
  function requestLeave(relay) {
    if (!entry || readOnly || !agents.canMutate || entry.instances.length < 2) return false
    if (leaveArmed !== relay) { leaveArmed = relay; disarm.restart(); return true }
    leaveArmed = ""
    return agents.leaveCommunity(agentId, relay)
  }
  function requestDelete() {
    if (!entry || readOnly || !agents.canMutate) return false
    if (!deleteArmed) { deleteArmed = true; disarm.restart(); return true }
    deleteArmed = false
    return agents.deleteAgent(agentId, false)
  }
  Timer { id: disarm; interval: 5000; onTriggered: { root.deleteArmed = false; root.leaveArmed = "" } }
  Connections {
    target: root.agents
    function onAgentCreated(agentId) {
      if (!root.creating) return
      if (root.pendingCreateArt !== "")
        root.agents.setAvatarArt(agentId, root.pendingCreateArt, root.pendingCreateBrightness > 0 ? root.pendingCreateBrightness : undefined)
      root.pendingCreateArt = ""
      root.agentChosen(agentId)
    }
    function onRequestStateChanged() {
      if (!root.agents || root.agents.requestState !== "done") return
      if (root.agents.requestType === "delete_agent" && root.agents.requestAgent === root.agentId) root.backRequested()
      else if (["update_agent", "enroll_agent_in", "leave_agent_community"].indexOf(root.agents.requestType) !== -1
          && root.agents.requestAgent === root.agentId) root.load()
    }
    // A persona removed elsewhere closes its editor.
    function onAgentsChanged() { if (!root.creating && !root.entry) root.backRequested() }
  }

  spacing: Style.space(6)

  component Caption: Text {
    Layout.fillWidth: true
    textFormat: Text.PlainText
    elide: Text.ElideRight
    color: Color.foreground
    opacity: 0.6
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }

  RowLayout {
    Layout.fillWidth: true
    spacing: Style.space(8)
    BuzzAvatar {
      key: root.agents && root.entry ? root.agents.avatarKey(root.entry) : ""
      name: root.creating ? "New agent" : root.entry ? root.entry.name : ""
      art: root.agents && root.entry ? root.agents.avatarArtFor(root.agentId) : ""
      brightness: root.agents && root.entry ? root.agents.avatarBrightnessFor(root.agentId) : 0
    }
    Text {
      objectName: "buzzAgentTitle"
      text: root.creating ? "New agent" : root.entry ? root.entry.name : ""
      textFormat: Text.PlainText
      elide: Text.ElideRight
      Layout.maximumWidth: Style.space(260)
      color: Color.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.body * 1.15
      font.bold: true
    }
    Caption {
      text: root.entry ? root.agents.statusWord(root.entry) + " · " + root.agents.harnessLabel(root.entry.harness)
        + ((root.instance || root.entry).published ? " · published" : "") : "Not saved"
    }
  }
  Caption {
    objectName: "buzzAgentCommunity"
    visible: root.communityName !== ""
    text: "Community: " + root.communityName
  }
  Caption {
    objectName: "buzzAgentOtherCommunity"
    visible: root.readOnly
    opacity: 0.8
    text: "Enrolled in " + (root.agents ? root.agents.communityNames(root.entry) : "") + ". Switch to "
      + (root.entry && root.entry.instances.length > 1 ? "one of those communities" : "that community") + " to manage it"
      + (root.canAdd ? ", or add it to " + root.agents.currentCommunityName + " below." : ".")
    wrapMode: Text.WordWrap
    elide: Text.ElideNone
  }

  Controls.ScrollView {
    Layout.fillWidth: true
    Layout.fillHeight: true
    Layout.preferredHeight: Style.space(80)
    contentWidth: availableWidth
    clip: true
    ColumnLayout {
      width: parent.width
      spacing: Style.space(4)
      // The communities this agent is enrolled in, each with its rooms; Start,
      // Stop and Leave per community for an agent managed here.
      Caption {
        objectName: "buzzAgentCommunitiesHeading"
        visible: !!root.entry
        text: "Communities"
      }
      Repeater {
        model: root.entry ? root.entry.instances : []
        delegate: ColumnLayout {
          id: instanceRow
          required property var modelData
          objectName: "buzzAgentInstance"
          readonly property string relay: modelData.relay
          readonly property bool here: !!root.instance && root.instance.relay === modelData.relay
          Layout.fillWidth: true
          spacing: Style.space(2)
          Caption {
            objectName: "buzzAgentInstanceLabel"
            opacity: instanceRow.here ? 0.9 : 0.6
            text: modelData.community + (instanceRow.here ? " (this community)" : "") + " · "
              + root.agents.statusWord(root.entry, modelData) + (modelData.published ? "" : " · not published")
              + (modelData.startAtLogin ? " · starts at login" : "")
              + (modelData.lastError ? " · " + root.agents.categorySentence(modelData.lastError) : "")
          }
          Caption {
            objectName: "buzzAgentInstanceRooms"
            // Room names are known only for this community's verified rooms.
            text: modelData.rooms.map(function(id) {
              var known = instanceRow.here ? root.agents.roomChoices.find(function(room) { return room.id === id }) : null
              return known ? "# " + known.name : "Room " + id.slice(0, 8) + "…"
            }).join(", ")
          }
          Flow {
            Layout.fillWidth: true
            spacing: Style.space(4)
            visible: !root.readOnly && root.entry.enrolled
            Ui.Button {
              objectName: "buzzAgentInstanceStart"
              readonly property string relay: modelData.relay
              // The current community's Start and Stop are the editor's own buttons below.
              visible: !instanceRow.here && modelData.state !== "active" && modelData.published
              text: "Start"
              fontSize: Style.font.caption
              focusable: true
              enabled: root.agents.canMutate && !root.bundleStale
              opacity: enabled ? 1 : 0.5
              onClicked: root.agents.startAgent(root.agentId, modelData.relay)
            }
            Ui.Button {
              objectName: "buzzAgentInstanceStop"
              readonly property string relay: modelData.relay
              visible: !instanceRow.here && modelData.state === "active"
              text: "Stop"
              fontSize: Style.font.caption
              focusable: true
              enabled: root.agents.canMutate
              opacity: enabled ? 1 : 0.5
              onClicked: root.agents.stopAgent(root.agentId, modelData.relay)
            }
            Ui.Button {
              objectName: "buzzAgentInstanceRetry"
              readonly property string relay: modelData.relay
              // Publication there failed: retried in this community only (its rooms are checked here).
              visible: !modelData.published && instanceRow.here
              text: "Retry enrollment"
              fontSize: Style.font.caption
              focusable: true
              enabled: root.agents.canMutate
              opacity: enabled ? 1 : 0.5
              onClicked: root.agents.enrollAgent(root.agentId)
            }
            Ui.Button {
              objectName: "buzzAgentLeave"
              readonly property string relay: modelData.relay
              visible: root.entry.instances.length > 1
              text: root.leaveArmed === modelData.relay ? "Confirm leave " + modelData.community : "Leave " + modelData.community
              tooltipText: "Removes the agent from its rooms there and stops it there; its identity and other communities stay"
              fontSize: Style.font.caption
              focusable: true
              selected: root.leaveArmed === modelData.relay
              enabled: root.agents.canMutate
              opacity: enabled ? 1 : 0.5
              onClicked: root.requestLeave(modelData.relay)
            }
          }
        }
      }
      Caption {
        objectName: "buzzAgentLastCommunity"
        visible: !!root.entry && !root.readOnly && root.entry.instances.length === 1 && root.entry.enrolled
        opacity: 0.5
        text: "Its only community cannot be left; delete the agent instead."
        wrapMode: Text.WordWrap
        elide: Text.ElideNone
      }
      // The same agent (identity and instructions) in the current community.
      ColumnLayout {
        objectName: "buzzAgentAddSection"
        visible: root.canAdd
        Layout.fillWidth: true
        spacing: Style.space(2)
        Caption {
          text: "Rooms in " + (root.agents ? root.agents.currentCommunityName : "") + " · " + root.addRooms.length + " of up to 8"
        }
        Repeater {
          model: root.canAdd ? root.agents.roomChoices : []
          delegate: Controls.CheckBox {
            required property var modelData
            objectName: "buzzAgentAddRoom"
            readonly property string roomId: modelData.id
            Layout.fillWidth: true
            text: "# " + modelData.name
            checked: root.addRooms.indexOf(modelData.id) !== -1
            enabled: checked || root.addRooms.length < 8
            contentItem: Text {
              text: parent.text
              textFormat: Text.PlainText
              leftPadding: parent.indicator ? parent.indicator.width + Style.space(6) : 0
              color: Color.foreground
              opacity: parent.enabled ? 1 : 0.5
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
              elide: Text.ElideRight
            }
            onClicked: { root.toggleAddRoom(modelData.id); checked = Qt.binding(function() { return root.addRooms.indexOf(modelData.id) !== -1 }) }
          }
        }
        Ui.Button {
          objectName: "buzzAgentAddToCommunity"
          text: "Add to " + (root.agents ? root.agents.currentCommunityName : "")
          tooltipText: "Admits the same agent to these rooms and publishes its profile there"
          bordered: true
          fontSize: Style.font.caption
          focusable: true
          enabled: root.agents.canMutate && root.addRooms.length > 0
          opacity: enabled ? 1 : 0.5
          onClicked: root.addToCommunity()
        }
      }
      ColumnLayout {
        Layout.fillWidth: true
        spacing: Style.space(4)
        // Read-only for an agent not enrolled here: nothing here can change it.
        enabled: !root.readOnly
        Caption { text: "Name" }
        Ui.TextField {
          id: nameField
          objectName: "buzzAgentName"
          Layout.fillWidth: true
          verticalPadding: Style.space(4)
          text: root.draftName
          maximumLength: 64
          placeholderText: "Shown in rooms and on the agent's profile"
          onTextChanged: if (text !== root.draftName) root.draftName = text
        }
        Caption { text: "Description" }
        Ui.TextField {
          id: descriptionField
          objectName: "buzzAgentDescription"
          Layout.fillWidth: true
          verticalPadding: Style.space(4)
          text: root.draftDescription
          maximumLength: 256
          placeholderText: "Public profile description"
          onTextChanged: if (text !== root.draftDescription) root.draftDescription = text
        }
        Caption { text: "Instructions" }
        Controls.ScrollView {
          Layout.fillWidth: true
          Layout.preferredHeight: Math.max(Style.space(60), Math.min(Style.space(160), instructions.implicitHeight))
          contentWidth: availableWidth
          clip: true
          Controls.TextArea {
            id: instructions
            objectName: "buzzAgentInstructions"
            textFormat: TextEdit.PlainText
            wrapMode: TextEdit.Wrap
            text: root.draftInstructions
            placeholderText: "What this agent does and how it answers"
            placeholderTextColor: Util.alpha(Color.foreground, 0.5)
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            background: Rectangle {
              color: Color.popups.background
              border.color: instructions.activeFocus ? Color.popups.border : Util.alpha(Color.foreground, 0.25)
              radius: Style.cornerRadius
            }
            onTextChanged: if (text !== root.draftInstructions) root.draftInstructions = text
          }
        }
        Caption { text: "Avatar (ASCII art, optional)" }
        RowLayout {
          spacing: Style.space(8)
          Controls.TextArea {
            id: artField
            objectName: "buzzAgentAvatarArt"
            visible: !AnsiArt.isAnsi(root.draftArt)
            Layout.preferredWidth: artMetrics.advanceWidth + leftPadding + rightPadding
            Layout.preferredHeight: artMetrics.height * 6 + topPadding + bottomPadding
            textFormat: TextEdit.PlainText
            wrapMode: TextEdit.NoWrap
            text: root.draftArt
            placeholderText: "6 × 12"
            placeholderTextColor: Util.alpha(Color.foreground, 0.5)
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            background: Rectangle {
              color: Color.popups.background
              border.color: artField.activeFocus ? Color.popups.border : Util.alpha(Color.foreground, 0.25)
              radius: Style.cornerRadius
            }
            // Input is held to 6 lines of 12 columns without control characters.
            onTextChanged: {
              var clipped = Identicon.clipArt(text)
              if (clipped !== text) {
                var position = Math.min(cursorPosition, clipped.length)
                text = clipped
                cursorPosition = position
                return
              }
              if (text !== root.draftArt) root.draftArt = text
            }
            TextMetrics {
              id: artMetrics
              font: artField.font
              text: "MMMMMMMMMMMM"
            }
          }
          BuzzAvatar {
            id: artPreview
            Layout.alignment: Qt.AlignTop
            key: root.agents && root.entry ? root.agents.avatarKey(root.entry) : ""
            name: "Preview"
            art: root.draftArt
            brightness: root.shownBrightness
          }
          Caption {
            Layout.alignment: Qt.AlignTop
            text: AnsiArt.isAnsi(root.draftArt) ? "Colored art from a file · kept on this machine only" : "Kept on this machine only"
          }
        }
        Ui.Button {
          objectName: "buzzAgentAvatarLoad"
          text: artLoader.visible ? "Hide file loader" : "Load from file…"
          tooltipText: "Use a .ans or .txt file as this agent's avatar"
          fontSize: Style.font.caption
          focusable: true
          onClicked: artLoader.visible = !artLoader.visible
        }
        AvatarFileLoader {
          id: artLoader
          service: root.service
          visible: false
          Layout.maximumWidth: Style.space(360)
          Layout.fillWidth: true
          fieldName: "buzzAgentAvatarPath"
          showClear: true
          canClear: AnsiArt.isAnsi(root.draftArt)
          // Loaded art is a draft like typed art: Save keeps it.
          // A newly loaded file starts at the default brightness (auto-levels, 1.5).
          onArtLoaded: function(art) { root.draftBrightness = AnsiArt.DEFAULT_BRIGHTNESS; root.draftArt = art }
          onClearRequested: { root.draftArt = ""; root.draftBrightness = 0 }
        }
        // A draft like the art: the preview above follows it and Save keeps it.
        AvatarBrightness {
          visible: AnsiArt.isAnsi(root.draftArt)
          Layout.maximumWidth: Style.space(360)
          Layout.fillWidth: true
          fieldName: "buzzAgentAvatarBrightness"
          value: root.draftBrightness > 0 ? root.draftBrightness : AnsiArt.DEFAULT_BRIGHTNESS
          onChosen: function(value) { root.draftBrightness = value }
        }
        Caption { text: "Harness" }
        RowLayout {
          spacing: Style.space(6)
          Repeater {
            model: root.agents ? root.agents.harnessIds : []
            delegate: Ui.Button {
              required property string modelData
              objectName: "buzzAgentHarness"
              readonly property string harnessId: modelData
              readonly property var harnessEntry: root.agents.harness(modelData)
              text: root.agents.harnessLabel(modelData) + (harnessEntry && harnessEntry.bundle === "missing" ? " · not installed"
                : harnessEntry && harnessEntry.bundle === "stale" ? " · needs refresh" : "")
              fontSize: Style.font.caption
              focusable: true
              selected: root.draftHarness === modelData
              onClicked: root.draftHarness = modelData
            }
          }
        }
        Caption { text: "Model" }
        Ui.TextField {
          id: modelField
          objectName: "buzzAgentModel"
          Layout.fillWidth: true
          verticalPadding: Style.space(4)
          text: root.draftModel
          maximumLength: 64
          placeholderText: "Harness default"
          onTextChanged: if (text !== root.draftModel) root.draftModel = text
        }
        // The harness CLI's own aliases; a click fills the field.
        Flow {
          Layout.fillWidth: true
          spacing: Style.space(4)
          visible: chips.count > 0
          Repeater {
            id: chips
            model: root.agents && root.agents.modelAliases.hasOwnProperty(root.draftHarness) ? root.agents.modelAliases[root.draftHarness] : []
            delegate: Ui.Button {
              required property string modelData
              objectName: "buzzAgentModelChip"
              readonly property string alias: modelData
              text: modelData
              tooltipText: "Use the latest " + modelData + " model"
              fontSize: Style.font.caption
              focusable: true
              selected: root.draftModel === modelData
              onClicked: root.draftModel = modelData
            }
          }
        }
        Caption {
          objectName: "buzzAgentModelHint"
          text: root.draftHarness === "codex" ? "Codex model ids look like gpt-5.5, o3 or codex-…; empty uses the harness default."
            : "An alias uses the latest model of that tier; a full id looks like claude-opus-4-5."
          wrapMode: Text.WordWrap
          elide: Text.ElideNone
        }
        RowLayout {
          visible: !!root.entry && !root.readOnly
          spacing: Style.space(6)
          Ui.Button {
            objectName: "buzzProbeModel"
            text: "Test model"
            tooltipText: "Runs one short prompt with the saved model in this agent's sandbox"
            fontSize: Style.font.caption
            focusable: true
            enabled: root.canProbe
            opacity: enabled ? 1 : 0.5
            onClicked: root.agents.probeModel(root.agentId)
          }
          Caption {
            objectName: "buzzProbeModelNote"
            text: "A real model turn: it may count toward the harness's usage."
            wrapMode: Text.WordWrap
            elide: Text.ElideNone
          }
        }
        Caption {
          objectName: "buzzProbeModelResult"
          visible: text !== ""
          opacity: 0.8
          text: root.probeCaption
          wrapMode: Text.WordWrap
          elide: Text.ElideNone
        }
        Caption {
          text: "Rooms · " + root.draftRooms.length + " of up to 8"
            + (root.readOnly ? " · in " + root.communityName
              : root.roomChoices.length === 0 ? " · no verified rooms available" : "")
        }
        Repeater {
          // Verified rooms, then any saved room the helper no longer lists so it can be removed.
          model: {
            if (!root.agents) return []
            // Only this agent's community's verified rooms (none for another community).
            var choices = root.roomChoices.map(function(room) { return {id: room.id, label: "# " + room.name, verified: true} })
            root.draftRooms.forEach(function(id) {
              if (!choices.some(function(room) { return room.id === id }))
                choices.push({id: id, label: (root.readOnly ? "Room " : "Unverified room ") + id.slice(0, 8) + "…", verified: false})
            })
            return choices
          }
          delegate: Controls.CheckBox {
            required property var modelData
            objectName: "buzzAgentRoom"
            readonly property string roomId: modelData.id
            Layout.fillWidth: true
            text: modelData.label
            checked: root.draftRooms.indexOf(modelData.id) !== -1
            enabled: checked || (modelData.verified && root.draftRooms.length < 8)
            contentItem: Text {
              text: parent.text
              textFormat: Text.PlainText
              leftPadding: parent.indicator ? parent.indicator.width + Style.space(6) : 0
              color: Color.foreground
              opacity: parent.enabled ? 1 : 0.5
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
              elide: Text.ElideRight
            }
            onClicked: { root.toggleRoom(modelData.id); checked = Qt.binding(function() { return root.draftRooms.indexOf(modelData.id) !== -1 }) }
          }
        }
        Caption { text: "Answers" }
        RowLayout {
          spacing: Style.space(6)
          Ui.Button {
            objectName: "buzzAgentOwnerOnly"
            text: "Only me"
            tooltipText: "Answers only your mentions"
            fontSize: Style.font.caption
            focusable: true
            selected: root.draftRespondTo === "owner-only"
            onClicked: root.draftRespondTo = "owner-only"
          }
          Ui.Button {
            objectName: "buzzAgentMentions"
            text: "Anyone who mentions it"
            tooltipText: "Answers any room member's mention"
            fontSize: Style.font.caption
            focusable: true
            selected: root.draftRespondTo === "mentions"
            onClicked: root.draftRespondTo = "mentions"
          }
        }
        Ui.Button {
          objectName: "buzzAgentDms"
          text: "Answers direct messages: " + (root.draftAnswersDms ? "on" : "off")
          tooltipText: "Also listen in direct messages opened with this agent"
          fontSize: Style.font.caption
          focusable: true
          selected: root.draftAnswersDms
          onClicked: root.draftAnswersDms = !root.draftAnswersDms
        }
        Caption {
          Layout.fillWidth: true
          text: "On: the agent listens in every room and direct message it belongs to, and answers only you in a direct message."
          wrapMode: Text.WordWrap
          elide: Text.ElideNone
        }
        Caption { text: "Workspace" + (root.workspaceEditable ? "" : " in " + root.communityName) }
        Ui.TextField {
          id: workspaceField
          objectName: "buzzAgentWorkspace"
          visible: root.workspaceEditable || root.readOnly
          Layout.fillWidth: true
          verticalPadding: Style.space(4)
          text: root.draftWorkspace
          maximumLength: 4096
          placeholderText: "~/.local/state/omarchy-buzz-room-workspaces/" + (root.creating ? "<agent id>" : root.agentId)
          onTextChanged: if (text !== root.draftWorkspace) root.draftWorkspace = text
        }
        Caption {
          objectName: "buzzAgentInstanceWorkspace"
          visible: !root.workspaceEditable && !root.readOnly
          // Named like the instance's unit: `<id>-<12 hex>`.
          text: root.instance ? "~/.local/state/omarchy-buzz-room-workspaces/" + root.instance.unit.slice(19, -8) : ""
          elide: Text.ElideMiddle
        }
        Ui.Button {
          objectName: "buzzAgentStartAtLogin"
          text: "Start at login" + (root.instance && root.entry.instances.length > 1 ? " in " + root.communityName : "") + ": "
            + ((root.instance ? root.instance.startAtLogin : root.entry ? root.entry.startAtLogin : root.draftStartAtLogin) ? "on" : "off")
          tooltipText: root.creating ? "Saved with the new agent" : "Applied now"
          fontSize: Style.font.caption
          focusable: true
          enabled: root.creating || (!!root.agents && root.agents.canMutate)
          opacity: enabled ? 1 : 0.5
          onClicked: {
            if (root.creating) root.draftStartAtLogin = !root.draftStartAtLogin
            else if (root.instance) root.agents.setStartAtLogin(root.agentId, !root.instance.startAtLogin, root.instance.relay)
          }
        }
      }
    }
  }

  Caption {
    objectName: "buzzAgentRestartNote"
    visible: root.stopsOnSave
    text: "Saving stops the running agent; start it again to apply."
    wrapMode: Text.WordWrap
    elide: Text.ElideNone
  }
  Caption {
    objectName: "buzzAgentProblem"
    visible: text !== ""
    text: root.dirty && root.problem !== "Nothing changed." ? root.problem : ""
    wrapMode: Text.WordWrap
    elide: Text.ElideNone
  }
  Caption {
    objectName: "buzzAgentStatus"
    visible: text !== ""
    opacity: 0.8
    wrapMode: Text.WordWrap
    elide: Text.ElideNone
    text: {
      if (!root.agents) return ""
      var parts = []
      if (root.agents.statusLabel) parts.push(root.agents.statusLabel)
      if (root.entry && root.entry.lastError) parts.push("Last error: " + root.agents.categorySentence(root.entry.lastError))
      if (root.bundleStale) parts.push("Harness bundle needs a refresh")
      if (root.harnessState && root.harnessState.signedIn === null) parts.push(root.agents.harnessLabel(root.harnessState.id) + " sign-in state unknown")
      return parts.join(" · ")
    }
  }
  Flow {
    Layout.fillWidth: true
    spacing: Style.space(6)
    Ui.Button {
      objectName: "buzzAgentSave"
      visible: !root.readOnly
      text: root.creating ? "Create" : "Save"
      bordered: true
      fontSize: Style.font.caption
      focusable: true
      enabled: root.canSave
      opacity: enabled ? 1 : 0.5
      onClicked: root.save()
    }
    Ui.Button {
      objectName: "buzzAgentEnroll"
      visible: !!root.entry && !root.entry.enrolled && !root.readOnly
      text: "Enroll"
      tooltipText: "Create the agent's identity and add it to its rooms"
      fontSize: Style.font.caption
      focusable: true
      enabled: !!root.agents && root.agents.canMutate
      opacity: enabled ? 1 : 0.5
      onClicked: root.agents.enrollAgent(root.agentId)
    }
    Ui.Button {
      objectName: "buzzAgentStart"
      visible: !!root.instance && root.entry.enrolled && root.instance.published && root.instance.state !== "active"
      text: "Start"
      tooltipText: root.bundleStale ? "Harness bundle needs a refresh" : ""
      fontSize: Style.font.caption
      focusable: true
      enabled: !!root.agents && root.agents.canMutate && !root.bundleStale
      opacity: enabled ? 1 : 0.5
      onClicked: root.agents.startAgent(root.agentId, root.instance.relay)
    }
    Ui.Button {
      objectName: "buzzRefreshBundle"
      visible: root.bundleStale && !root.readOnly
      text: "Refresh bundle"
      tooltipText: "Replace the harness bundle's launcher scripts with the installed ones"
      fontSize: Style.font.caption
      focusable: true
      enabled: !!root.agents && root.agents.canMutate
      opacity: enabled ? 1 : 0.5
      onClicked: root.agents.refreshBundle(root.harnessState.id)
    }
    Ui.Button {
      objectName: "buzzAgentStop"
      visible: !!root.instance && root.instance.state === "active"
      text: "Stop"
      fontSize: Style.font.caption
      focusable: true
      enabled: !!root.agents && root.agents.canMutate
      opacity: enabled ? 1 : 0.5
      onClicked: root.agents.stopAgent(root.agentId, root.instance.relay)
    }
    Ui.Button {
      objectName: "buzzAgentSignIn"
      visible: !!root.harnessState && root.harnessState.signedIn === false && !root.readOnly
      text: "Sign in to " + (root.harnessState ? root.agents.harnessLabel(root.harnessState.id) : "")
      tooltipText: "Opens a terminal for the harness's own sign-in"
      fontSize: Style.font.caption
      focusable: true
      enabled: !!root.agents && root.agents.canMutate
      opacity: enabled ? 1 : 0.5
      onClicked: root.agents.signIn(root.harnessState.id)
    }
    Ui.Button {
      objectName: "buzzAgentDelete"
      visible: !!root.entry && !root.readOnly
      text: root.deleteArmed ? "Confirm delete" : "Delete"
      tooltipText: "Stops the agent and removes it from every community; its identity stays in the secret store"
      fontSize: Style.font.caption
      focusable: true
      selected: root.deleteArmed
      enabled: !!root.agents && root.agents.canMutate
      opacity: enabled ? 1 : 0.5
      onClicked: root.requestDelete()
    }
  }
}
