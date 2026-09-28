// Local observed activity hints from the helper's complete monitored-room summary.
// This is not Buzz synchronized unread state; no message data or counts persist.
function fresh() { return {scope: "", rooms: Object.create(null)}; }

function whole(value, maximum) {
  return typeof value === "number" && isFinite(value) && value >= 0
    && value <= maximum && Math.floor(value) === value;
}

function valid(summary) {
  return summary && typeof summary.roomId === "string" && summary.roomId.length > 0
    && summary.roomId.length <= 128
    && whole(summary.epoch, 9007199254740991)
    && whole(summary.observed, 1000000000);
}

function update(state, scope, summaries, visibleRoomId) {
  if (typeof scope !== "string" || scope.length === 0 || !Array.isArray(summaries)
      || summaries.length > 20) return {state:fresh(), notify:false};
  var previous = state && state.scope === scope && state.rooms ? state.rooms : Object.create(null);
  var rooms = Object.create(null);
  var notify = false;
  for (var i = 0; i < summaries.length; i++) {
    var item = summaries[i];
    if (!valid(item) || Object.prototype.hasOwnProperty.call(rooms, item.roomId))
      return {state:{scope:scope, rooms:Object.create(null)}, notify:false};
    var old = previous[item.roomId];
    var baseline = !old || old.epoch !== item.epoch || item.observed < old.latest;
    var seen = baseline ? item.observed : Math.min(old.seen, item.observed);
    if (visibleRoomId === item.roomId) seen = item.observed;
    if (!baseline && item.observed > old.latest && visibleRoomId !== item.roomId) notify = true;
    rooms[item.roomId] = {epoch:item.epoch, latest:item.observed, seen:seen};
  }
  // Missing summaries are no longer monitored (for example after revocation).
  return {state:{scope:scope, rooms:rooms}, notify:notify};
}

function markSeen(state, roomId) {
  if (!state || !state.rooms || !Object.prototype.hasOwnProperty.call(state.rooms, roomId)) return state;
  var rooms = Object.assign(Object.create(null), state.rooms);
  var room = rooms[roomId];
  rooms[roomId] = {epoch:room.epoch, latest:room.latest, seen:room.latest};
  return {scope:state.scope, rooms:rooms};
}

function count(state, roomId) {
  var room = state && state.rooms ? state.rooms[roomId] : null;
  return room ? Math.max(0, Math.min(1000000000, room.latest - room.seen)) : 0;
}

function total(state) {
  var sum = 0;
  if (state && state.rooms) for (var roomId in state.rooms) {
    if (Object.prototype.hasOwnProperty.call(state.rooms, roomId)) sum += count(state, roomId);
    if (sum > 999) return 999;
  }
  return sum;
}

function totalLabel(state) {
  var sum = 0;
  if (state && state.rooms) for (var roomId in state.rooms) {
    if (Object.prototype.hasOwnProperty.call(state.rooms, roomId)) sum += count(state, roomId);
    if (sum > 999) return "999+";
  }
  return String(sum);
}
