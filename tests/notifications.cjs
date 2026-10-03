// Exercise the exact QML JavaScript wording and policy model with synthetic notices.
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const assert = require('node:assert/strict');
const model = vm.createContext({});
vm.runInContext(fs.readFileSync(path.join(__dirname, '../plugin/Notifications.js'), 'utf8'), model);

const n = (extra) => Object.assign({kind: 'room', count: 1, roomName: 'general', sender: 'Alex', snippet: 'hello'}, extra);
const text = (extra, dm = false, body = true) => model.compose(n(extra), dm, body);

assert.deepEqual({...text({})}, {title: 'Alex in #general', body: 'hello'});
assert.deepEqual({...text({kind: 'mention'})}, {title: 'Alex mentioned you in #general', body: 'hello'});
assert.deepEqual({...text({kind: 'thread'})}, {title: 'Alex replied in #general', body: 'hello'});
assert.deepEqual({...text({kind: 'dm', roomName: 'Alex'}, true)}, {title: 'Alex (direct message)', body: 'hello'});
assert.equal(text({kind: 'dm', sender: '', roomName: 'Sam, Kim'}, true).title, 'Sam, Kim (direct message)');
assert.equal(text({kind: 'dm', sender: '', roomName: ''}, true).title, 'Direct message');
assert.equal(text({kind: 'mention', sender: ''}).title, 'Mention in #general');
assert.equal(text({kind: 'thread', sender: '', snippet: ''}).body, 'New reply');
assert.equal(text({sender: '', roomName: '', snippet: ''}).title, 'New message');
// No text: who wrote, never what; bursts keep their observed count.
assert.equal(text({snippet: 'secret'}, false, false).body, 'New message from Alex');
assert.equal(text({snippet: 'secret', count: 3}, false, false).body, '3 new messages');
assert.equal(text({count: 3}).body, '3 new messages. Latest: hello');
// The server renders body markup: text stays literal.
assert.equal(text({snippet: '<img src="http://x/y.png"> & <b>x</b>'}).body, '&lt;img src="http://x/y.png"&gt; &amp; &lt;b&gt;x&lt;/b&gt;');
assert.ok(!/[<>]/.test(text({snippet: '<<img src=x>'}).body));
// Words that look like options never lead an argument.
assert.equal(model.arg('--exec'), ' --exec');
assert.equal(model.arg('-t'), ' -t');
assert.equal(model.arg('Alex'), 'Alex');
// Policy by mode.
const kinds = ['mention', 'dm', 'thread', 'room'];
const pick = (mode) => kinds.filter((k) => model.allows(mode, k)).join(',');
assert.equal(pick('direct'), 'mention,dm,thread');
assert.equal(pick('mentions'), 'mention,thread');
assert.equal(pick('dms'), 'dm');
assert.equal(pick('all'), 'mention,dm,thread,room');
assert.equal(pick('none'), '');
assert.equal(pick('bogus'), '');
assert.equal(pick('__proto__'), '');
console.log('Notifications: titles, bodies, escaping, argument safety and mode policy passed');
