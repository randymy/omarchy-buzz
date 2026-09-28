// Run the exact QML JS module with synthetic public metadata; no desktop/network.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const context = vm.createContext({});
vm.runInContext(fs.readFileSync(require('node:path').join(__dirname, '../plugin/ActivityObserver.js'), 'utf8'), context);
const row = (id, author = 'other', time = 100) => ({id, author, time, edited:false, unavailable:false});
let state = context.fresh();
function step(rows, scope = 'room-a', now = 100) {
  const result = context.observe(state, scope, rows, 'self', now);
  state = result.state;
  return result.notify;
}
assert.equal(step([row('history')]), false, 'initial history is a silent baseline');
assert.equal(step([row('new'), row('history')]), true);
assert.equal(step([row('new'), row('history')]), false, 'status echoes are silent');
assert.equal(step([row('own', 'self'), row('new')]), false);
assert.equal(step([{...row('edit'), edited:true}, row('own', 'self')]), false);
assert.equal(step([{...row('hidden'), unavailable:true}, row('edit')]), false);
assert.equal(step([row('future', 'other', 1000), row('hidden')]), false);
assert.equal(step([row('old', 'other', 1), row('future')]), false);
assert.equal(step([row('disjoint')]), false, 'catch-up gap must not replay notifications');
assert.equal(step([row('next'), row('disjoint')]), true);
assert.equal(step([row('different')], 'room-b'), false);
assert.equal(step([row('different')], 'room-b-new-session'), false);
state = context.fresh();
assert.equal(step([]), false);
assert.equal(step([row('first')]), true, 'new message after an empty baseline');
assert.equal(step([]), false);
assert.equal(step([row('first')]), false, 'removed/reappearing rows do not renotify');
for (let i = 0; i < 1000; i++) {
  step([row('bounded-'+i), row('first')]);
  assert.ok(state.ids.length <= 512);
}
console.log('Activity observer: baseline, scope, gaps, own messages, deduplication and bounds passed');
