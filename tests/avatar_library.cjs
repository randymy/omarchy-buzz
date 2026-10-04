// Every built-in avatar passes the real validator and is in its exact stored
// form; the gallery helpers and the paste check behave. QML libraries are
// loaded the way QML runs them: one context per file, imports as globals.
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const assert = require('node:assert/strict');

function load(file, imports = {}) {
  const source = fs.readFileSync(path.join(__dirname, '../plugin', file), 'utf8')
    .split('\n').map((line) => (/^\.(pragma|import)\b/.test(line) ? '' : line)).join('\n');
  const context = vm.createContext({...imports});
  // `var` declarations become context properties; functions too.
  vm.runInContext(source, context);
  return context;
}
const Identicon = load('Identicon.js');
const AnsiArt = load('AnsiArt.js', {Identicon});
const Library = load('AvatarLibrary.js');

const entries = Library.ENTRIES;
const categories = Library.CATEGORIES;
assert.ok(entries.length >= 120, `at least 120 avatars, found ${entries.length}`);
assert.deepEqual([...categories], [
  'Animals', 'Faces and characters', 'Robots and tech', 'Nature', 'Objects',
  'Symbols and abstract', 'Food', 'Space', 'Geometric patterns', 'Retro and game',
]);
for (const category of categories) {
  assert.ok(entries.filter((e) => e.category === category).length >= 10, `${category} has at least ten avatars`);
}

const names = new Set();
const arts = new Set();
for (const entry of entries) {
  const label = `${entry.category} / ${entry.name}`;
  assert.ok(categories.includes(entry.category), `${label}: known category`);
  assert.ok(/^[A-Za-z][A-Za-z0-9 -]{0,23}$/.test(entry.name), `${label}: plain name`);
  assert.ok(!names.has(`${entry.category}/${entry.name}`), `${label}: unique name`);
  names.add(`${entry.category}/${entry.name}`);
  assert.ok(!arts.has(entry.art), `${label}: unique art`);
  arts.add(entry.art);
  // Printable ASCII and newlines only: no escape sequences, tabs or controls.
  assert.ok(/^[\x20-\x7e\n]+$/.test(entry.art), `${label}: printable ASCII`);
  const lines = entry.art.split('\n');
  assert.ok(lines.length <= Library.ROWS, `${label}: at most ${Library.ROWS} lines, has ${lines.length}`);
  for (const line of lines) assert.ok(line.length <= Library.COLUMNS, `${label}: at most ${Library.COLUMNS} columns: ${JSON.stringify(line)}`);
  assert.ok(/\S/.test(entry.art), `${label}: not blank`);
  assert.ok(lines[0].trim() !== '' && lines.at(-1).trim() !== '', `${label}: no blank first or last line`);
  assert.ok(lines.every((line) => line === line.replace(/\s+$/, '')), `${label}: no trailing blanks`);
  assert.ok(lines.some((line) => !line.startsWith(' ')), `${label}: not indented as a whole`);
  // The validator and the stored form leave it exactly as written.
  assert.ok(!AnsiArt.isAnsi(entry.art), `${label}: plain, not grid art`);
  assert.equal(Identicon.normalizeArt(entry.art), entry.art, `${label}: normalizeArt keeps it`);
  assert.equal(AnsiArt.storedArt(entry.art), entry.art, `${label}: storedArt keeps it`);
  const checked = AnsiArt.check(entry.art);
  assert.ok(checked.ok, `${label}: check passes: ${checked.errors}`);
  assert.equal(checked.art, entry.art, `${label}: check keeps it`);
  assert.deepEqual([...checked.errors], []);
}

// Gallery helpers: filtering, wrapping and shuffling stay in range.
assert.equal(Library.count(), entries.length);
assert.equal(Library.indexes('').length, entries.length);
let sum = 0;
for (const category of categories) {
  const list = Library.indexes(category);
  sum += list.length;
  assert.ok(list.every((i) => entries[i].category === category));
}
assert.equal(sum, entries.length, 'every entry is in exactly one category');
assert.equal(Library.indexes('Nope').length, 0);
const food = Library.indexes('Food');
assert.equal(Library.wrap(food, 0, -1), food.length - 1);
assert.equal(Library.wrap(food, food.length - 1, 1), 0);
assert.equal(Library.wrap(food, 3, 1), 4);
assert.equal(Library.wrap(food, 999, 1), 0, 'a stale position is clamped');
assert.equal(Library.wrap([], 0, 1), -1);
for (const length of [1, 2, 3, 14, 140]) {
  const list = Array.from({length}, (_, i) => i);
  for (let position = 0; position < length; position += Math.max(1, Math.floor(length / 5))) {
    for (const unit of [0, 0.25, 0.5, 0.999, 1, -1]) {
      const next = Library.pick(list, position, unit);
      assert.ok(Number.isInteger(next) && next >= 0 && next < length, `pick in range (${length}, ${position}, ${unit}): ${next}`);
      if (length > 1) assert.notEqual(next, position, 'shuffle moves');
    }
  }
}
assert.equal(Library.pick([], 0, 0.5), -1);

// Pasted art: what is accepted and the plain reasons for what is not.
const errors = (text) => [...AnsiArt.check(text).errors];
assert.deepEqual(errors(''), ['Nothing to save yet.']);
assert.deepEqual(errors('   \n\n'), ['Nothing to save yet.']);
assert.deepEqual(errors(null), ['Nothing to save yet.']);
let result = AnsiArt.check('\n\n /\\_/\\\n( o.o )\n > ^ <  \n\n');
assert.ok(result.ok && result.art === ' /\\_/\\\n( o.o )\n > ^ <' && result.rows === 3 && result.cols === 7, 'plain art, with blank edges');
result = AnsiArt.check('a\r\nb\r\n');
assert.ok(result.ok && result.art === 'a\nb', 'CRLF accepted');
assert.deepEqual(errors('0123456789abc'), ['Too wide: 13 columns, at most 12.']);
assert.deepEqual(errors('a\nb\nc\nd\ne\nf\ng'), ['Too tall: 7 lines, at most 6.']);
assert.deepEqual(errors('0123456789abc\n2\n3\n4\n5\n6\n7'), ['Too wide: 13 columns, at most 12.', 'Too tall: 7 lines, at most 6.']);
assert.equal(AnsiArt.check('a'.repeat(12) + '\n' + 'b\n'.repeat(5)).ok, true, 'exactly 6 x 12 is accepted');
assert.match(errors('a\tb')[0], /^Invalid characters/);
assert.match(errors('a\x07b')[0], /^Invalid characters/);
assert.match(errors('a\x1bb')[0], /^Invalid characters/);
assert.match(errors('a‮b')[0], /^Invalid characters/);
assert.deepEqual(errors('x'.repeat(262145)), ['Too large: at most 256 KiB.']);
assert.deepEqual(errors('\x1b[31m'), ['Nothing to save yet.']);
// Colored art keeps its own limits and is stored in the sanitized form.
const colored = ('\x1b[31m' + '#'.repeat(50) + '\x1b[0m\n').repeat(20);
result = AnsiArt.check(colored);
assert.ok(result.ok && result.colored && result.cols === 50 && result.rows === 20 && result.art === AnsiArt.sanitize(colored));
assert.equal(AnsiArt.storedArt(result.art), result.art, 'stored colored art is canonical');
assert.deepEqual(errors('\x1b[31m' + 'x'.repeat(121)), ['Too wide: 121 columns, at most 120.']);
assert.deepEqual(errors('\x1b[31mx\n'.repeat(61)), ['Too tall: 61 lines, at most 60.']);
assert.ok(AnsiArt.check('\x1b[2J\x1b[H\x1b[31mhi\x1b[5C\x1b]0;t\x07there').ok, 'cursor sequences are removed, not refused');
assert.equal(AnsiArt.check('\x1b[2J\x1b[Hhi').art, AnsiArt.sanitize('\x1b[2J\x1b[Hhi'));

// The color row: a distinct palette that stays readable (3:1) on dark and light panels.
const colors = Library.COLORS.map((c) => c.hex);
assert.equal(colors.length, 14);
assert.equal(new Set(colors).size, colors.length, 'palette colors are distinct');
for (const c of colors) assert.ok(AnsiArt.validTint(c), `${c} is a valid stored color`);
for (const back of ['#000000', '#1a1b26', '#2b2b2b', '#ffffff', '#f5f0e6', '#eff1f5', '#808080', '#a0a0a0']) {
  for (const c of colors) {
    const shown = Library.readable(c, back);
    assert.match(shown, /^#[0-9a-f]{6}$/);
    assert.ok(Library.contrast(shown, back) >= 3, `${c} on ${back} -> ${shown}`);
  }
}
// On a mid-gray, colors move toward the side that can reach 3:1 and stay distinct.
assert.ok(new Set(Library.COLORS.map(c => Library.readable(c.hex, '#a0a0a0'))).size >= 10, 'mid-gray collapses the palette');
assert.equal(Library.readable('#ef4444', '#1a1b26'), '#ef4444', 'readable colors are unchanged');
assert.equal(Library.readable('#ef4444', '#ff1a1b26'), '#ef4444', 'a leading alpha is ignored');
assert.notEqual(Library.readable('#fde047', '#ffffff'), '#fde047', 'yellow is darkened on white');
for (const bad of ['red', 'red; x', '#zzzzzz', '#ABCDEF', '#fff', '#1234567', '#ef4444\n', '', null, 7, undefined]) {
  assert.equal(AnsiArt.validTint(bad), false, `${JSON.stringify(bad)} is not a stored color`);
  assert.equal(Library.readable(bad, '#000000'), '');
}
assert.equal(Library.readable('#ef4444', 'bad'), '');

console.log(`PASS: ${entries.length} avatars in ${categories.length} categories, all valid plain ASCII in stored form; gallery helpers and paste checks`);
