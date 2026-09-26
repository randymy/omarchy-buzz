import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import qs.Ui as Ui
import qs.Commons

FocusScope {
  id: root
  property var service: null
  signal closeRequested()
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
        text: "DEVELOPMENT PREVIEW · Sample data only\nNo relay connected. Messages and agents below are examples."
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
          text: root.service ? root.service.viewModel.community : "Service unavailable"
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          opacity: 0.7
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }
        Repeater {
          model: root.service ? root.service.rooms : []
          delegate: Ui.Button {
            required property var modelData
            Layout.fillWidth: true
            text: "# " + modelData.name
            leftAlign: true
            focusable: true
            selected: root.service && root.service.selectedRoomId === modelData.id
            onClicked: root.service.selectRoom(modelData.id)
          }
        }
        Item { Layout.fillHeight: true }
        Text {
          Layout.fillWidth: true
          text: "Connection\nNot configured"
          textFormat: Text.PlainText
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
          text: root.service && root.service.selectedRoom ? "# " + root.service.selectedRoom.name : "Buzz is unavailable"
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: Color.foreground
          font.family: Style.font.family
          font.pixelSize: Style.font.body * 1.2
          font.bold: true
        }
        Text {
          Layout.fillWidth: true
          text: root.service && root.service.selectedRoom ? root.service.selectedRoom.description : "Enable the plugin service and reopen this panel."
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: Color.foreground
          opacity: 0.7
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }

        Controls.ScrollView {
          Layout.fillWidth: true
          Layout.fillHeight: true
          clip: true
          contentWidth: availableWidth
          Column {
            width: parent.width
            spacing: Style.space(22)
            Repeater {
              model: root.service ? root.service.messages : []
              delegate: Column {
                required property var modelData
                width: parent.width
                spacing: Style.space(6)
                Text {
                  width: parent.width
                  text: modelData.author + " · " + modelData.role + " · " + modelData.time
                  textFormat: Text.PlainText
                  wrapMode: Text.WordWrap
                  color: Color.accent
                  font.family: Style.font.family
                  font.pixelSize: Style.font.body
                  font.bold: true
                }
                Text {
                  width: parent.width
                  text: modelData.text
                  textFormat: Text.PlainText
                  wrapMode: Text.WordWrap
                  color: Color.foreground
                  font.family: Style.font.family
                  font.pixelSize: Style.font.body
                }
              }
            }
          }
        }

        Rectangle {
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
