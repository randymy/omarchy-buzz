import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons
import "Identicon.js" as Identicon

// Editor for one agent persona in the middle column. It edits a local draft and
// sends only structured, locally validated requests through the agent service.
ColumnLayout {
  id: root
  property var agents: null
  // Empty for a new agent, otherwise the service's persona id.
  property string agentId: ""
  signal backRequested()
  signal agentChosen(string agentId)

  readonly property bool creating: agentId === ""
  readonly property var entry: agents && !creating ? agents.agent(agentId) : null
  property string draftName: ""
  property string draftDescription: ""
  property string draftInstructions: ""
  property string draftHarness: "codex"
  property string draftModel: ""
  property var draftRooms: []
  property string draftRespondTo: "owner-only"
  property string draftWorkspace: ""
  property bool draftStartAtLogin: false
  // Pasted avatar art: kept on this machine only, never sent to the agent service.
  property string draftArt: ""
  property string pendingCreateArt: ""
  property bool deleteArmed: false
  readonly property alias artPreview: artPreview

  function load() {
    deleteArmed = false
    var source = entry
    draftName = source ? source.name : ""
    draftDescription = source ? source.description : ""
    draftInstructions = source ? source.instructions : ""
    draftHarness = source ? source.harness : "codex"
    draftModel = source ? source.model : ""
    draftRooms = source ? source.rooms.slice() : []
    draftRespondTo = source ? source.respondTo : "owner-only"
    draftWorkspace = source ? source.workspace : ""
    draftStartAtLogin = source ? source.startAtLogin : false
    draftArt = source ? agents.avatarArtFor(source.id) : ""
  }
  onAgentIdChanged: load()
  // Fields follow the draft when it is loaded; typing updates the draft.
  onDraftNameChanged: if (nameField.text !== draftName) nameField.text = draftName
  onDraftDescriptionChanged: if (descriptionField.text !== draftDescription) descriptionField.text = draftDescription
  onDraftInstructionsChanged: if (instructions.text !== draftInstructions) instructions.text = draftInstructions
  onDraftModelChanged: if (modelField.text !== draftModel) modelField.text = draftModel
  onDraftWorkspaceChanged: if (workspaceField.text !== draftWorkspace) workspaceField.text = draftWorkspace
  onDraftArtChanged: if (artField.text !== draftArt) artField.text = draftArt
  Component.onCompleted: load()

  readonly property var draftFields: ({name: draftName, description: draftDescription, instructions: draftInstructions,
    harness: draftHarness, model: draftModel, rooms: draftRooms, respondTo: draftRespondTo, workspace: draftWorkspace})
  // Only what differs from the saved persona is sent on update.
  readonly property var changedFields: {
    if (creating || !entry) return ({})
    var changed = {}
    var fields = draftFields
    Object.keys(fields).forEach(function(key) {
      if (JSON.stringify(fields[key]) !== JSON.stringify(entry[key])) changed[key] = fields[key]
    })
    return changed
  }
  readonly property var saveFields: {
    if (!creating) return changedFields
    var fields = Object.assign({}, draftFields)
    fields.startAtLogin = draftStartAtLogin
    return fields
  }
  readonly property string problem: agents ? agents.fieldsProblem(saveFields, creating) : ""
  readonly property bool serviceChanged: Object.keys(changedFields).length > 0
  readonly property bool artChanged: !creating && !!entry && Identicon.normalizeArt(draftArt) !== agents.avatarArtFor(agentId)
  readonly property bool dirty: creating || serviceChanged || artChanged
  // Art alone is saved locally and needs no agent service request.
  readonly property bool canSave: !!agents && (creating || !!entry)
    && (creating || serviceChanged ? agents.canMutate && problem === "" : artChanged)
  readonly property var harnessState: agents ? agents.harness(entry ? entry.harness : draftHarness) : null

  function save() {
    if (!canSave) return false
    if (creating) {
      pendingCreateArt = Identicon.normalizeArt(draftArt)
      return agents.createAgent(saveFields)
    }
    if (artChanged && !agents.setAvatarArt(agentId, draftArt)) return false
    return serviceChanged ? agents.updateAgent(agentId, saveFields) : true
  }
  function toggleRoom(id) {
    var copy = draftRooms.slice()
    var index = copy.indexOf(id)
    if (index !== -1) copy.splice(index, 1)
    else if (copy.length < 8 && agents.roomChoices.some(function(room) { return room.id === id })) copy.push(id)
    else return false
    draftRooms = copy
    return true
  }
  function requestDelete() {
    if (!entry || !agents.canMutate) return false
    if (!deleteArmed) { deleteArmed = true; disarm.restart(); return true }
    deleteArmed = false
    return agents.deleteAgent(agentId, false)
  }
  Timer { id: disarm; interval: 5000; onTriggered: root.deleteArmed = false }
  Connections {
    target: root.agents
    function onAgentCreated(agentId) {
      if (!root.creating) return
      if (root.pendingCreateArt !== "") root.agents.setAvatarArt(agentId, root.pendingCreateArt)
      root.pendingCreateArt = ""
      root.agentChosen(agentId)
    }
    function onRequestStateChanged() {
      if (!root.agents || root.agents.requestState !== "done") return
      if (root.agents.requestType === "delete_agent" && root.agents.requestAgent === root.agentId) root.backRequested()
      else if (root.agents.requestType === "update_agent" && root.agents.requestAgent === root.agentId) root.load()
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
        + (root.entry.published ? " · published" : "") : "Not saved"
    }
    Ui.Button {
      objectName: "buzzAgentBack"
      text: "Back to rooms"
      fontSize: Style.font.caption
      focusable: true
      onClicked: root.backRequested()
    }
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
        }
        Caption {
          Layout.alignment: Qt.AlignTop
          text: "Kept on this machine only"
        }
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
            text: root.agents.harnessLabel(modelData) + (harnessEntry && harnessEntry.bundle === "missing" ? " · not installed" : "")
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
      Caption {
        text: "Rooms · " + root.draftRooms.length + " of up to 8"
          + (root.agents && root.agents.roomChoices.length === 0 ? " · no verified rooms available" : "")
      }
      Repeater {
        // Verified rooms, then any saved room the helper no longer lists so it can be removed.
        model: {
          if (!root.agents) return []
          var choices = root.agents.roomChoices.map(function(room) { return {id: room.id, label: "# " + room.name, verified: true} })
          root.draftRooms.forEach(function(id) {
            if (!choices.some(function(room) { return room.id === id }))
              choices.push({id: id, label: "Unverified room " + id.slice(0, 8) + "…", verified: false})
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
      Caption { text: "Workspace" }
      Ui.TextField {
        id: workspaceField
        objectName: "buzzAgentWorkspace"
        Layout.fillWidth: true
        verticalPadding: Style.space(4)
        text: root.draftWorkspace
        maximumLength: 4096
        placeholderText: "~/.local/state/omarchy-buzz-room-workspaces/" + (root.creating ? "<agent id>" : root.agentId)
        onTextChanged: if (text !== root.draftWorkspace) root.draftWorkspace = text
      }
      Ui.Button {
        objectName: "buzzAgentStartAtLogin"
        text: "Start at login: " + ((root.entry ? root.entry.startAtLogin : root.draftStartAtLogin) ? "on" : "off")
        tooltipText: root.creating ? "Saved with the new agent" : "Applied now"
        fontSize: Style.font.caption
        focusable: true
        enabled: root.creating || (!!root.agents && root.agents.canMutate)
        opacity: enabled ? 1 : 0.5
        onClicked: {
          if (root.creating) root.draftStartAtLogin = !root.draftStartAtLogin
          else if (root.entry) root.agents.setStartAtLogin(root.agentId, !root.entry.startAtLogin)
        }
      }
    }
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
      if (root.harnessState && root.harnessState.signedIn === null) parts.push(root.agents.harnessLabel(root.harnessState.id) + " sign-in state unknown")
      return parts.join(" · ")
    }
  }
  Flow {
    Layout.fillWidth: true
    spacing: Style.space(6)
    Ui.Button {
      objectName: "buzzAgentSave"
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
      visible: !!root.entry && !root.entry.enrolled
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
      visible: !!root.entry && root.entry.enrolled && root.entry.unit !== "active"
      text: "Start"
      fontSize: Style.font.caption
      focusable: true
      enabled: !!root.agents && root.agents.canMutate
      opacity: enabled ? 1 : 0.5
      onClicked: root.agents.startAgent(root.agentId)
    }
    Ui.Button {
      objectName: "buzzAgentStop"
      visible: !!root.entry && root.entry.unit === "active"
      text: "Stop"
      fontSize: Style.font.caption
      focusable: true
      enabled: !!root.agents && root.agents.canMutate
      opacity: enabled ? 1 : 0.5
      onClicked: root.agents.stopAgent(root.agentId)
    }
    Ui.Button {
      objectName: "buzzAgentSignIn"
      visible: !!root.harnessState && root.harnessState.signedIn === false
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
      visible: !!root.entry
      text: root.deleteArmed ? "Confirm delete" : "Delete"
      tooltipText: "Stops and removes the agent; its identity stays in the secret store"
      fontSize: Style.font.caption
      focusable: true
      selected: root.deleteArmed
      enabled: !!root.agents && root.agents.canMutate
      opacity: enabled ? 1 : 0.5
      onClicked: root.requestDelete()
    }
  }
}
