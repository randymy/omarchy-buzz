// Best-effort, session-local hints for a single monitored room, not unread state.
// Input is the helper's already validated, bounded history projection.
function fresh() { return {scope: "", ids: [], previous: [], floor: 0}; }

function observe(state, scope, rows, ownKey, nowSeconds) {
  var ids = rows.map(function(row) { return row.id; });
  var baseline = state.scope !== scope;
  var overlap = state.previous.some(function(id) { return ids.indexOf(id) !== -1; });
  // A disjoint nonempty page could be a catch-up gap, not new activity.
  if (!baseline && state.previous.length && ids.length && !overlap) baseline = true;
  if (baseline || state.ids.length + ids.length > 512) {
    return {state: {scope:scope, ids:ids, previous:ids, floor:nowSeconds - 1}, notify:false};
  }
  var known = state.ids.slice();
  var notify = false;
  for (var i = 0; i < rows.length; i++) {
    var row = rows[i];
    if (known.indexOf(row.id) !== -1) continue;
    known.push(row.id);
    if (row.author !== ownKey && !row.unavailable && !row.edited
        && row.time >= state.floor && row.time <= nowSeconds + 60) notify = true;
  }
  return {state:{scope:scope, ids:known, previous:ids, floor:state.floor}, notify:notify};
}
