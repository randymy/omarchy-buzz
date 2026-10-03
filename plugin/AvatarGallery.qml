import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import qs.Ui as Ui
import qs.Commons
import "AnsiArt.js" as AnsiArt
import "AvatarLibrary.js" as AvatarLibrary

// Avatar gallery for Settings: browse the built-in ASCII avatars (Previous,
// Next, Shuffle, a category filter, Left and Right arrow keys) or paste or
// type your own art, checked live with the same limits the avatar keeps. Each
// choice is previewed with the real avatar at the sizes it appears; Save hands
// the art to the owner (saveRequested), which keeps it on this machine through
// the existing avatar store. Nothing is sent anywhere, and no art is ever run
// or interpreted: it is drawn as plain text by BuzzAvatar.
FocusScope {
  id: root
  objectName: "buzzAvatarGallery"
  // The key the preview avatars are drawn for (a public key, never a secret).
  property string key: ""
  // The art kept now, to tell a saved choice from a new one.
  property string savedArt: ""
  // "gallery" or "create".
  property string mode: "gallery"
  // "" for every avatar, or one of AvatarLibrary.CATEGORIES.
  property string category: ""
  // Position in the filtered list.
  property int position: 0
  property string customText: ""
  // Avatar sizes in the bar, in message rows and on the profile card.
  property real barSize: Math.max(6, Math.round(Style.font.caption * 0.8))
  property real messageSize: Style.font.caption
  property real profileSize: Style.font.body * 2
  signal saveRequested(string art)

  readonly property var list: AvatarLibrary.indexes(category)
  readonly property int total: list.length
  readonly property var entry: total > 0 ? AvatarLibrary.ENTRIES[list[Math.min(position, total - 1)]] : null
  readonly property var custom: AnsiArt.check(customText)
  readonly property string customArt: custom.ok ? custom.art : ""
  readonly property real customBrightness: custom.colored ? AnsiArt.DEFAULT_BRIGHTNESS : 0
  readonly property bool entrySaved: !!entry && AnsiArt.storedArt(entry.art) === savedArt
  readonly property bool customSaved: custom.ok && custom.art === savedArt
  readonly property string counter: !entry ? "" : (Math.min(position, total - 1) + 1) + " / " + total + " · " + entry.category

  implicitWidth: column.implicitWidth
  implicitHeight: column.implicitHeight

  function show(next) { if (next !== position) position = next }
  function step(delta) { show(AvatarLibrary.wrap(list, position, delta)) }
  function previous() { step(-1) }
  function next() { step(1) }
  function shuffle() { show(AvatarLibrary.pick(list, position, Math.random())) }
  function setCategory(name) {
    if (name === category) return
    category = name
    position = 0
  }
  function saveEntry() { if (entry) saveRequested(entry.art) }
  function saveCustom() { if (custom.ok) saveRequested(custom.art) }
  // Copies the shown avatar into the Create tab to change it.
  function customize() {
    if (!entry) return
    customText = entry.art
    mode = "create"
  }
  onCustomTextChanged: if (customField.text !== customText) customField.text = customText
  onModeChanged: if (mode === "gallery") forceActiveFocus()

  Keys.onLeftPressed: function(event) { if (mode === "gallery") { event.accepted = true; previous() } else event.accepted = false }
  Keys.onRightPressed: function(event) { if (mode === "gallery") { event.accepted = true; next() } else event.accepted = false }

  // The same avatar at the three sizes it appears, labelled.
  component Previews: RowLayout {
    id: previews
    property string art: ""
    property real brightness: 0
    property string objectPrefix: ""
    spacing: Style.space(16)
    Repeater {
      model: [{label: "Bar", size: root.barSize}, {label: "Messages", size: root.messageSize}, {label: "Profile", size: root.profileSize}]
      delegate: ColumnLayout {
        required property var modelData
        spacing: Style.space(2)
        Layout.alignment: Qt.AlignBottom
        BuzzAvatar {
          objectName: previews.objectPrefix + modelData.label
          Layout.alignment: Qt.AlignHCenter
          key: root.key
          name: "Preview"
          art: previews.art
          brightness: previews.brightness
          pixelSize: modelData.size
        }
        Text {
          Layout.alignment: Qt.AlignHCenter
          text: modelData.label
          textFormat: Text.PlainText
          color: Color.foreground
          opacity: 0.6
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
      }
    }
  }
  component Note: Text {
    Layout.fillWidth: true
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    color: Color.foreground
    opacity: 0.7
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }

  ColumnLayout {
    id: column
    anchors.left: parent.left
    anchors.right: parent.right
    spacing: Style.space(6)

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(4)
      Ui.Button {
        objectName: "buzzAvatarGalleryTabGallery"
        text: "Gallery"
        fontSize: Style.font.caption
        focusable: true
        selected: root.mode === "gallery"
        onClicked: root.mode = "gallery"
      }
      Ui.Button {
        objectName: "buzzAvatarGalleryTabCreate"
        text: "Create your own"
        fontSize: Style.font.caption
        focusable: true
        selected: root.mode === "create"
        onClicked: root.mode = "create"
      }
    }

    // Gallery: browse, preview and save a built-in avatar.
    ColumnLayout {
      objectName: "buzzAvatarGalleryBrowse"
      visible: root.mode === "gallery"
      Layout.fillWidth: true
      spacing: Style.space(6)
      Flow {
        Layout.fillWidth: true
        spacing: Style.space(4)
        Repeater {
          model: [""].concat(AvatarLibrary.CATEGORIES)
          delegate: Ui.Button {
            required property string modelData
            objectName: "buzzAvatarGalleryCategory"
            readonly property string categoryName: modelData
            text: modelData === "" ? "All" : modelData
            fontSize: Style.font.caption
            horizontalPadding: Style.space(6)
            verticalPadding: Style.space(2)
            focusable: true
            selected: root.category === modelData
            onClicked: { root.setCategory(modelData); root.forceActiveFocus() }
          }
        }
      }
      RowLayout {
        Layout.fillWidth: true
        spacing: Style.space(8)
        Previews {
          objectPrefix: "buzzAvatarGalleryPreview"
          art: root.entry ? root.entry.art : ""
        }
        ColumnLayout {
          Layout.fillWidth: true
          spacing: Style.space(2)
          Text {
            objectName: "buzzAvatarGalleryCounter"
            Layout.fillWidth: true
            text: root.counter
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
          }
          Text {
            objectName: "buzzAvatarGalleryName"
            Layout.fillWidth: true
            text: root.entry ? root.entry.name : ""
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Color.foreground
            opacity: 0.7
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }
      }
      RowLayout {
        Layout.fillWidth: true
        spacing: Style.space(4)
        Ui.Button {
          objectName: "buzzAvatarGalleryPrevious"
          text: "◀ Previous"
          tooltipText: "Previous avatar (Left arrow)"
          fontSize: Style.font.caption
          focusable: true
          onClicked: { root.previous(); root.forceActiveFocus() }
        }
        Ui.Button {
          objectName: "buzzAvatarGalleryNext"
          text: "Next ▶"
          tooltipText: "Next avatar (Right arrow)"
          fontSize: Style.font.caption
          focusable: true
          onClicked: { root.next(); root.forceActiveFocus() }
        }
        Ui.Button {
          objectName: "buzzAvatarGalleryShuffle"
          text: "Shuffle"
          tooltipText: "A random avatar from this list"
          fontSize: Style.font.caption
          focusable: true
          onClicked: { root.shuffle(); root.forceActiveFocus() }
        }
        Item { Layout.fillWidth: true }
        Ui.Button {
          objectName: "buzzAvatarGalleryCustomize"
          text: "Edit a copy"
          tooltipText: "Copy this avatar into Create your own"
          fontSize: Style.font.caption
          focusable: true
          onClicked: root.customize()
        }
        Ui.Button {
          objectName: "buzzAvatarGallerySave"
          text: root.entrySaved ? "Saved" : "Save"
          tooltipText: "Use this avatar on this machine"
          fontSize: Style.font.caption
          focusable: true
          enabled: !!root.entry && !root.entrySaved
          opacity: enabled ? 1 : 0.5
          onClicked: { root.saveEntry(); root.forceActiveFocus() }
        }
      }
    }

    // Create: paste or type art, checked as you go.
    ColumnLayout {
      objectName: "buzzAvatarGalleryCreate"
      visible: root.mode === "create"
      Layout.fillWidth: true
      spacing: Style.space(6)
      Note { text: "Paste or type ASCII art: at most 6 lines of 12 columns, or colored .ans art up to 60 rows of 120 columns and 256 KiB. It is kept on this machine only." }
      Controls.ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: customMetrics.height * 7 + customField.topPadding + customField.bottomPadding
        contentWidth: availableWidth
        clip: true
        Controls.TextArea {
          id: customField
          objectName: "buzzAvatarCustomText"
          textFormat: TextEdit.PlainText
          wrapMode: TextEdit.NoWrap
          text: root.customText
          placeholderText: "Paste art here"
          placeholderTextColor: Util.alpha(Color.foreground, 0.5)
          color: Color.foreground
          selectByMouse: true
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
          background: Rectangle {
            color: Color.popups.background
            border.color: customField.activeFocus ? Color.popups.border : Util.alpha(Color.foreground, 0.25)
            radius: Style.cornerRadius
          }
          onTextChanged: if (text !== root.customText) root.customText = text
          TextMetrics { id: customMetrics; font: customField.font; text: "M" }
        }
      }
      Text {
        objectName: "buzzAvatarCustomStatus"
        Layout.fillWidth: true
        text: root.custom.ok ? "Looks good: " + root.custom.rows + " lines of " + root.custom.cols + " columns."
          : root.customText === "" ? "" : root.custom.errors.join("\n")
        visible: text !== ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Color.foreground
        opacity: root.custom.ok ? 0.7 : 1
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
        font.bold: !root.custom.ok
      }
      RowLayout {
        Layout.fillWidth: true
        spacing: Style.space(8)
        Previews {
          visible: root.custom.ok
          objectPrefix: "buzzAvatarCustomPreview"
          art: root.customArt
          brightness: root.customBrightness
        }
        Item { Layout.fillWidth: true }
        Ui.Button {
          objectName: "buzzAvatarCustomSave"
          Layout.alignment: Qt.AlignBottom
          text: root.customSaved ? "Saved" : "Save"
          tooltipText: "Use this art as your avatar on this machine"
          fontSize: Style.font.caption
          focusable: true
          enabled: root.custom.ok && !root.customSaved
          opacity: enabled ? 1 : 0.5
          onClicked: root.saveCustom()
        }
      }
    }
  }
}
