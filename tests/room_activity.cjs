// Exercise the exact QML JavaScript model with synthetic helper summaries.
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const assert = require('node:assert/strict');
const model = vm.createContext({});
vm.runInContext(fs.readFileSync(path.join(__dirname, '../plugin/RoomActivity.js'), 'utf8'), model);

const scope = 'relay|identity|instance|generation';
const entry = (roomId, observed, epoch = 1) => ({roomId, observed, epoch});
let state = model.fresh();
function step(rows, visible = '', nextScope = scope) {
  const result = model.update(state, nextScope, rows, visible);
  state = result.state;
  return result.notify;
}

assert.equal(step([entry('a', 12), entry('b', 4)]), false, 'first counters are a silent baseline');
assert.equal(model.total(state), 0);
assert.equal(step([entry('a', 13), entry('b', 4)]), true);
assert.equal(model.count(state, 'a'), 1);
assert.equal(model.count(state, 'b'), 0, 'unchanged own activity cannot add a badge');
assert.equal(step([entry('a', 13), entry('b', 4)]), false, 'duplicate summary cannot renotify');
assert.equal(step([entry('a', 13), entry('b', 6)]), true, 'other room increments independently');
assert.equal(model.total(state), 3);
state = model.markSeen(state, 'a');
assert.equal(model.count(state, 'a'), 0);
assert.equal(model.count(state, 'b'), 2);
assert.equal(step([entry('a', 14), entry('b', 6)], 'a'), false, 'visible room is seen immediately');
assert.equal(model.count(state, 'a'), 0);
state = model.markSeen(state, 'b');
assert.equal(model.total(state), 0);
assert.equal(step([entry('a', 14), entry('b', 10, 2)]), false, 'epoch change resets that room');
assert.equal(model.count(state, 'b'), 0);
assert.equal(step([entry('a', 14), entry('b', 11, 2)]), true);
assert.equal(step([entry('a', 14)], ''), false, 'missing room is pruned after membership loss');
assert.equal(model.count(state, 'b'), 0);
assert.equal(step([entry('a', 14), entry('b', 20, 2)]), false, 'reappearing room gets a fresh baseline');
assert.equal(step([entry('a', 100)], '', scope + '|reauth'), false, 'scope change clears prior counts');
assert.equal(model.total(state), 0);
assert.equal(step([entry('a', 101)], '', scope + '|reauth'), true);
assert.equal(step([entry('a', 1)], '', scope + '|reauth'), false, 'decreasing counter fails closed');
assert.equal(model.count(state, 'a'), 0);
assert.equal(step([entry('a', 2)], '', scope + '|reauth'), true);
assert.equal(step([entry('a', -1)], '', scope + '|reauth'), false, 'invalid counter resets model');
assert.equal(model.total(state), 0);
assert.equal(step([entry('a', 999999999)], '', scope + '|reauth'), false);
assert.equal(step([entry('a', 1000000000)], '', scope + '|reauth'), true);
assert.equal(step([entry('a', 1000000001)], '', scope + '|reauth'), false, 'counter above bound fails closed');
assert.equal(model.total(state), 0);
assert.equal(step([entry('a', 0), entry('a', 1)]), false, 'duplicate room IDs fail closed');
assert.equal(model.total(state), 0);
assert.equal(step([entry('a', 0)]), false);
assert.equal(step([entry('a', 1000)]), true);
assert.equal(model.total(state), 999);
assert.equal(model.totalLabel(state), '999+');
state = model.markSeen(state, 'a');
assert.equal(model.totalLabel(state), '0');
console.log('Room activity: baselines, scopes, multiroom counts, view, revocation, bounds passed');
