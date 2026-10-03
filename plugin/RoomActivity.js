// Local observed activity hints from the helper's complete monitored-room summary.
// This is not Buzz synchronized unread state; no message data or counts persist.
// The most summaries a frame carries: one per joined room, up to the helper's
// catalog bound (`catalog::MAX_ROOMS`, `activity::ROOMS`).
var MAX_SUMMARIES = 200;
function fresh() { return {scope: "", rooms: Object.create(null)}; }

function whole(value, maximum) {
  return typeof value === "number" && isFinite(value) && value >= 0
    && value <= maximum && Math.floor(value) === value;
}

function text(value, maximum) {
  return typeof value === "string" && value.length <= maximum;
}

// The helper's bounded, sanitized notification item; null means none yet.
function validNotice(n) {
  return n && whole(n.seq, 9007199254740991) && n.seq >= 1
    && ["mention", "dm", "thread", "room"].indexOf(n.kind) !== -1
    && whole(n.count, 999) && n.count >= 1
    && text(n.roomName, 256) && text(n.sender, 64) && text(n.snippet, 200)
    && typeof n.eventId === "string" && /^[a-f0-9]{64}$/.test(n.eventId)
    && (n.threadRoot === null || (typeof n.threadRoot === "string" && /^[a-f0-9]{64}$/.test(n.threadRoot)));
}

function valid(summary) {
  return summary && typeof summary.roomId === "string" && summary.roomId.length > 0
    && summary.roomId.length <= 128
    && whole(summary.epoch, 9007199254740991)
    && whole(summary.observed, 1000000000)
    && (summary.notice === undefined || summary.notice === null || validNotice(summary.notice));
}

// `notify` is the generic hint for summaries without a notice field (older
// helpers); `notices` are the new ones to announce, at most one per room.
function update(state, scope, summaries, visibleRoomId) {
  if (typeof scope !== "string" || scope.length === 0 || !Array.isArray(summaries)
      || summaries.length > MAX_SUMMARIES) return {state:fresh(), notify:false, notices:[]};
  var previous = state && state.scope === scope && state.rooms ? state.rooms : Object.create(null);
  var rooms = Object.create(null);
  var notify = false;
  var notices = [];
  for (var i = 0; i < summaries.length; i++) {
    var item = summaries[i];
    if (!valid(item) || Object.prototype.hasOwnProperty.call(rooms, item.roomId))
      return {state:{scope:scope, rooms:Object.create(null)}, notify:false, notices:[]};
    var old = previous[item.roomId];
    var baseline = !old || old.epoch !== item.epoch || item.observed < old.latest;
    var seen = baseline ? item.observed : Math.min(old.seen, item.observed);
    if (visibleRoomId === item.roomId) seen = item.observed;
    var noticeSeq = item.notice ? item.notice.seq : 0;
    if (item.notice === undefined) {
      if (!baseline && item.observed > old.latest && visibleRoomId !== item.roomId) notify = true;
    } else if (!baseline && noticeSeq > old.noticeSeq) notices.push({roomId:item.roomId, notice:item.notice});
    rooms[item.roomId] = {epoch:item.epoch, latest:item.observed, seen:seen, noticeSeq:noticeSeq};
  }
  // Missing summaries are no longer monitored (for example after revocation).
  return {state:{scope:scope, rooms:rooms}, notify:notify, notices:notices};
}

function markSeen(state, roomId) {
  if (!state || !state.rooms || !Object.prototype.hasOwnProperty.call(state.rooms, roomId)) return state;
  var rooms = Object.assign(Object.create(null), state.rooms);
  var room = rooms[roomId];
  rooms[roomId] = {epoch:room.epoch, latest:room.latest, seen:room.latest, noticeSeq:room.noticeSeq};
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
