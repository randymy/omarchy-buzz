import QtQuick
import "SampleData.js" as SampleData

Item {
  id: root

  property var shell: null
  property var manifest: null
  // M0 deliberately has no process, socket, configuration reader, or send action.
  // The future bridge will validate and normalize input before replacing this model.
  readonly property var snapshot: SampleData.snapshot()
  readonly property var viewModel: snapshot.payload
  readonly property var rooms: viewModel.rooms
  property string selectedRoomId: "sample-general"
  readonly property var selectedRoom: rooms.find(function(room) { return room.id === root.selectedRoomId }) || null
  readonly property var messages: viewModel.messages.filter(function(message) { return message.roomId === root.selectedRoomId })

  function selectRoom(roomId) {
    if (rooms.some(function(room) { return room.id === roomId })) selectedRoomId = roomId
  }
}
