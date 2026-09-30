import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons

// Composer for one destination: the room (empty rootId) or the open thread.
ColumnLayout {
  id: root
  property var service: null
  property string rootId: ""
  property string fieldName: "buzzComposer"
  property string placeholder: ""
  property bool showDelivery: false
  property bool pickerExpanded: false
  property int mentionIndex: 0
  property bool mentionDismissed: false
  readonly property alias field: area
  readonly property bool active: !!service && service.replyRootId === rootId
  readonly property string stored: service ? service.drafts[service.selectedRoomId + (rootId ? ":" + rootId : "")] || "" : ""
  readonly property bool usable: !!service && (rootId === "" || service.canReplyTo(rootId))
  readonly property var mentionQuery: {
    if (!service || !active || !area.visible || area.readOnly || service.recipientsState !== "snapshot"
        || service.recipientsRoomId !== service.selectedRoomId || service.recipientPickerLocked) return null
    var before = area.text.slice(0, area.cursorPosition)
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
  signal escaped()
  onMentionMatchesChanged: mentionIndex = 0
  onStoredChanged: if (area.text !== stored) area.text = stored
  function chooseMention(index) {
    if (!mentionOpen || !mentionQuery || index < 0 || index >= mentionMatches.length || !service) return false
    var query = mentionQuery
    var nextCursor = service.insertMention(mentionMatches[index].key, query.start, query.end)
    if (nextCursor < 0) return false
    if (area.text !== stored) area.text = stored
    area.cursorPosition = nextCursor
    area.forceActiveFocus()
    return true
  }
  function submit() { return !!service && service.canSendFor(rootId) && service.submitFor(rootId) }
  spacing: Style.space(4)

  RowLayout {
    Layout.fillWidth: true
    visible: root.showDelivery && !!root.service && root.service.deliveryState !== "idle"
    spacing: Style.space(8)
    Text {
      Layout.fillWidth: true
      text: root.service ? root.service.deliveryLabel : ""
      textFormat: Text.PlainText
      wrapMode: Text.WordWrap
      color: Color.foreground
      opacity: root.service && root.service.deliveryState === "acknowledged" ? 0.6 : 1
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    Ui.Button {
      text: !root.service ? "" : root.service.deliveryState === "unknown"
        ? "Discard uncertain draft" + (root.service.deliveryScopeMismatch ? " from " + root.service.submissionScopeLabel : "")
        : "Start new submission" + (root.service.deliveryScopeMismatch ? " for " + root.service.submissionScopeLabel : "")
      fontSize: Style.font.caption
      focusable: true
      visible: !!root.service && (root.service.deliveryState === "unknown" || root.service.deliveryState === "rejected" || root.service.deliveryCategory === "send_request_reused")
      onClicked: if (root.service) root.service.newDraft(root.service.deliveryState === "rejected" || root.service.deliveryCategory === "send_request_reused")
    }
  }
  Controls.ScrollView {
    visible: root.pickerExpanded && root.active
    Layout.fillWidth: true
    Layout.preferredHeight: Math.min(Style.space(96), people.implicitHeight)
    clip: true
    contentWidth: availableWidth
    Column {
      id: people
      width: parent.width
      Text {
        width: parent.width
        text: (root.service ? root.service.recipientsLabel : "") + " · names are self-asserted"
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: Color.foreground
        opacity: 0.7
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
      Repeater {
        model: root.service && root.pickerExpanded ? root.service.recipientEntries : []
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
  Repeater {
    model: root.service && root.active ? root.service.unavailableRecipients : []
    delegate: RowLayout {
      required property string modelData
      Layout.fillWidth: true
      Text {
        Layout.fillWidth: true
        text: "Mentioned " + modelData.slice(0, 12) + "…" + modelData.slice(-8) + " is not in the current member list"
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
      Ui.Button {
        text: "Remove"
        fontSize: Style.font.caption
        focusable: true
        enabled: root.service && !root.service.recipientPickerLocked
        onClicked: root.service.toggleRecipient(modelData)
      }
    }
  }
  Text {
    objectName: root.fieldName + "Notifies"
    Layout.fillWidth: true
    visible: root.active && !!root.service && root.service.outgoingMentions.length > 0
    text: root.service ? "Notifies: " + root.service.outgoingMentionNames : ""
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    color: Color.foreground
    opacity: 0.7
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }
  ColumnLayout {
    objectName: "buzzMentionSuggestions"
    Layout.fillWidth: true
    visible: root.mentionOpen
    spacing: Style.space(2)
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
        fontSize: Style.font.caption
        verticalPadding: Style.space(3)
        leftAlign: true
        focusable: false
        selected: root.mentionIndex === index
        onClicked: root.chooseMention(index)
      }
    }
  }
  RowLayout {
    Layout.fillWidth: true
    spacing: Style.space(6)
    Controls.ScrollView {
      Layout.fillWidth: true
      Layout.preferredHeight: Math.max(Style.space(36), Math.min(Style.space(132), area.implicitHeight))
      contentWidth: availableWidth
      clip: true
      Controls.TextArea {
        id: area
        objectName: root.fieldName
        placeholderText: root.usable ? root.placeholder : "Replies are not available yet"
        placeholderTextColor: Util.alpha(Color.foreground, 0.5)
        textFormat: TextEdit.PlainText
        wrapMode: TextEdit.Wrap
        text: root.stored
        readOnly: !root.service || !root.usable || root.service.recipientPickerLocked
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.body
        background: Rectangle {
          color: Color.popups.background
          border.color: area.activeFocus ? Color.popups.border : Util.alpha(Color.foreground, 0.25)
          radius: Style.cornerRadius
        }
        onTextChanged: {
          if (text.length > 4096) text = text.slice(0, 4096)
          if (root.service && text !== root.stored && !root.service.updateDraftFor(root.rootId, text)) text = root.stored
          root.mentionDismissed = false
        }
        onCursorPositionChanged: root.mentionDismissed = false
        Keys.onPressed: function(event) {
          var enter = event.key === Qt.Key_Return || event.key === Qt.Key_Enter
          if (root.mentionOpen) {
            if (event.key === Qt.Key_Down) {
              root.mentionIndex = (root.mentionIndex + 1) % root.mentionMatches.length
              event.accepted = true
            } else if (event.key === Qt.Key_Up) {
              root.mentionIndex = (root.mentionIndex + root.mentionMatches.length - 1) % root.mentionMatches.length
              event.accepted = true
            } else if (event.key === Qt.Key_Tab || enter) {
              event.accepted = root.chooseMention(root.mentionIndex)
            } else if (event.key === Qt.Key_Escape) {
              root.mentionDismissed = true
              event.accepted = true
            }
          } else if (enter && !(event.modifiers & Qt.ShiftModifier)) {
            // Enter sends; Shift+Enter keeps its default and adds a line.
            root.submit()
            event.accepted = true
          } else if (event.key === Qt.Key_Escape && root.rootId !== "") {
            root.escaped()
            event.accepted = true
          }
        }
      }
    }
    Ui.Button {
      Layout.alignment: Qt.AlignBottom
      text: "@" + (root.active && root.service && root.service.selectedRecipients.length ? " " + root.service.selectedRecipients.length : "")
      tooltipText: "Choose who this message notifies. Typing @ in the message does the same."
      focusable: true
      selected: root.pickerExpanded && root.active
      visible: !!root.service && root.usable && (root.service.recipientsSupported || root.service.selectedRecipients.length > 0)
      onClicked: {
        if (!root.service.composeScope(root.rootId)) return
        root.pickerExpanded = !root.pickerExpanded
      }
    }
    Ui.Button {
      objectName: root.fieldName + "Send"
      Layout.alignment: Qt.AlignBottom
      text: "Send"
      tooltipText: "Enter sends · Shift+Enter adds a line · 4096 bytes maximum"
      bordered: true
      focusable: true
      enabled: !!root.service && root.service.canSendFor(root.rootId)
      opacity: enabled ? 1 : 0.5
      onClicked: root.submit()
    }
  }
  Text {
    visible: !!root.service && root.service.utf8Size(area.text) > 3600
    text: root.service ? root.service.utf8Size(area.text) + " of 4096 bytes" : ""
    textFormat: Text.PlainText
    color: Color.foreground
    opacity: 0.7
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }
}
