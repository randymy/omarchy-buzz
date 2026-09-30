// ANSI art avatars: parser, thumbnail and stored form; the colored message
// avatar, profile card and "Set my avatar" against synthetic frames and a
// synthetic fixture file. No helper, relay, keys or network.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as Buzz
import "plugin/AnsiArt.js" as AnsiArt

ShellRoot {
  id: test
  property int stage: 0
  property int ticks: 0
  readonly property string me: "b".repeat(64)
  readonly property string other: "a".repeat(64)
  readonly property string room: "11111111-1111-4111-8111-111111111111"
  readonly property string fixturePath: Quickshell.env("BUZZ_ANSI_FIXTURE")
  readonly property string largePath: Quickshell.env("BUZZ_ANSI_LARGE")
  readonly property string capturePath: Quickshell.env("BUZZ_ANSI_CAPTURE")
  readonly property string captureDir: Quickshell.env("BUZZ_ANSI_CAPTURE_DIR")
  property string fixtureArt: ""
  Buzz.Service { id: service; autoConnect: false }
  FloatingWindow {
    id: window
    visible: true
    implicitWidth: 1000
    implicitHeight: 620
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: fixture; path: test.fixturePath; blockLoading: true; printErrors: false }
  FileView { id: avatarsFile; path: Quickshell.env("XDG_STATE_HOME") + "/omarchy-buzz/avatars.json"; blockLoading: true; blockWrites: true; printErrors: false }
  // Mouse and key events go through the real window, as a pointer and keyboard would.
  TestCase { id: input; when: false; name: "AnsiArtInput" }

  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function one(name) {
    var found = findNamed(view, name, []).filter(function(item) { return item.visible })
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  function cell(grid, r, c) { return grid.cells[r][c] }
  function rowText(grid, r) { return grid.cells[r].map(function(item) { return item.ch }).join("") }
  function freshOwnArt() {
    var fresh = Qt.createQmlObject('import "plugin" as Buzz\nBuzz.Service { autoConnect: false }', test, "freshService")
    var art = fresh.agents.avatarArtForKey(test.me)
    fresh.destroy()
    return art
  }
  // A new reader each time: the file on disk, not a cached copy.
  function readStore() {
    var reader = Qt.createQmlObject('import Quickshell.Io\nFileView { blockLoading: true; printErrors: false }', test, "storeReader")
    reader.path = avatarsFile.path
    var text = reader.text()
    reader.destroy()
    return text
  }
  function row(id, author, time) {
    return {id: id.repeat(64), author: author, time: time, text: "Synthetic message", edited: false, truncated: false, unavailable: false}
  }
  function frame() {
    return {version: 1, type: service.instanceId === "" ? "hello" : "status", instanceId: "ansi-fixture", generation: 1,
      capabilities: ["connection_status", "room_catalog", "room_history"],
      status: {generation: 1, connection: "authenticated", category: null, identity: test.me, relay: "wss://fixture.example/",
        catalog: {state: "partial", category: "room_catalog_partial", rooms: [{id: test.room, name: "Fixture", description: "",
          kind: "stream", participants: [], hidden: false}]},
        history: {state: "snapshot", roomId: test.room, rows: [row("1", test.other, 100), row("2", test.me, 1000),
          row("3", test.me, 1010), row("4", test.other, 2000)], hasMore: false, category: "history_completeness_unknown"}}}
  }
  // Every message row keeps one slot width, whatever the author's avatar is.
  function messageAvatars() {
    var avatars = findNamed(view, "buzzAvatar", []).filter(function(avatar) {
      var message = avatar.parent
      while (message && typeof message.lead !== "boolean") message = message.parent
      return !!message && message.objectName !== "buzzThreadRoot"
    })
    check(avatars.length === 4, "Expected four message avatars, found " + avatars.length)
    var slot = avatars[0].parent.width
    avatars.forEach(function(avatar) {
      check(slot > 0 && avatar.parent.width === slot, "Avatar slot widths differ: " + avatar.parent.width + " vs " + slot)
    })
    return avatars
  }
  function myLeadAvatar() {
    return messageAvatars().filter(function(avatar) { return avatar.key === test.me && avatar.visible })[0]
  }

  function parserCases() {
    var E = "\x1b"
    var g = AnsiArt.parse(E + "[38;2;1;2;3mA" + E + "[38;2;255;128;0mB")
    check(g.rows === 1 && g.cols === 2 && cell(g, 0, 0).ch === "A" && cell(g, 0, 0).fg === "#010203"
      && cell(g, 0, 1).fg === "#ff8000", "Truecolor not parsed")
    g = AnsiArt.parse(E + "[38;5;196mA" + E + "[38;5;244mB" + E + "[38;5;5mC" + E + "[38;5;21mD")
    check(cell(g, 0, 0).fg === "#ff0000" && cell(g, 0, 1).fg === "#808080" && cell(g, 0, 2).fg === AnsiArt.BASIC[5]
      && cell(g, 0, 3).fg === "#0000ff", "256-color not mapped: " + JSON.stringify(g.cells[0]))
    g = AnsiArt.parse(E + "[31mA" + E + "[92mB" + E + "[37mC" + E + "[39mD")
    check(cell(g, 0, 0).fg === AnsiArt.BASIC[1] && cell(g, 0, 1).fg === AnsiArt.BASIC[10]
      && cell(g, 0, 2).fg === AnsiArt.BASIC[7] && cell(g, 0, 3).fg === "", "Basic colors not mapped")
    g = AnsiArt.parse(E + "[31mA" + E + "[0mB" + E + "[31mC" + E + "[mD" + E + "[1mE" + E + "[1;34mF" + E + "[48;2;1;2;3;32mG"
      + E + "[38;2;300;0;0mH")
    check(cell(g, 0, 1).fg === "" && cell(g, 0, 3).fg === "" && cell(g, 0, 4).fg === "" && cell(g, 0, 5).fg === AnsiArt.BASIC[4]
      && cell(g, 0, 6).fg === AnsiArt.BASIC[2] && cell(g, 0, 7).fg === AnsiArt.BASIC[2],
      "Reset, bold, background or an out-of-range color handled wrong: " + JSON.stringify(g.cells[0]))
    // Backgrounds: truecolor, 256-color and basic; 49 and 0 clear them; blanks keep theirs.
    g = AnsiArt.parse(E + "[48;2;10;20;30mA " + E + "[48;5;21;38;2;1;1;1mB" + E + "[49mC" + E + "[44mD" + E + "[0mE" + E + "[48;2;9;9;9;48;2mF")
    check(cell(g, 0, 0).bg === "#0a141e" && cell(g, 0, 1).ch === " " && cell(g, 0, 1).bg === "#0a141e" && cell(g, 0, 1).fg === ""
      && cell(g, 0, 2).bg === "#0000ff" && cell(g, 0, 2).fg === "#010101" && cell(g, 0, 3).bg === null && cell(g, 0, 3).fg === "#010101"
      && cell(g, 0, 4).bg === AnsiArt.BASIC[4] && cell(g, 0, 5).bg === null && cell(g, 0, 5).fg === "" && cell(g, 0, 6).bg === "#090909",
      "Backgrounds handled wrong: " + JSON.stringify(g.cells[0]))
    g = AnsiArt.parse(E + "[2J" + E + "[HK" + E + "[5CL" + E + "[K" + E + "]0;title\x07M" + E + "7N" + E + "[?25lO" + E + "[12;3")
    check(g.rows === 1 && rowText(g, 0) === "KLMNO", "Cursor and other sequences not removed: " + JSON.stringify(rowText(g, 0)))
    g = AnsiArt.parse("a\rb\tc\u0007d\u202ee\u0000f\u009bg\r\nh\n")
    check(g.rows === 2 && rowText(g, 0) === "abcdefg" && rowText(g, 1) === "h      ", "Control characters not dropped: " + JSON.stringify(g.cells))
    check(AnsiArt.parse("X\x1aSAUCE00 trailer\nmore").rows === 1 && rowText(AnsiArt.parse("X\x1aSAUCE00"), 0) === "X", "SAUCE trailer kept")
    g = AnsiArt.parse("ab\nc")
    check(g.cols === 2 && cell(g, 1, 1).ch === " " && cell(g, 1, 1).fg === "", "Rows not padded")
    var tall = [], wide = "y".repeat(130)
    for (var i = 0; i < 70; i++) tall.push("x" + i)
    g = AnsiArt.parse(tall.join("\n") + "\n" + wide)
    check(g.rows === 60 && rowText(g, 59).trim() === "x59", "Rows not clipped at 60: " + g.rows)
    g = AnsiArt.parse(wide + "\n" + wide)
    check(g.rows === 2 && g.cols === 120, "Columns not clipped at 120: " + g.cols)
    g = AnsiArt.parse((E + "[0m").repeat(65534) + "ABCDEFGHIJ")
    check(rowText(g, 0) === "ABCDEFGH", "Input not clipped at 256 KiB: " + rowText(g, 0))
    check(AnsiArt.parse("").rows === 0 && AnsiArt.parse(null).rows === 0 && AnsiArt.sanitize(E + "[31m   \n") === "",
      "Empty art not empty")

    // Thumbnail: the centre cell of each block, or the nearest non-blank one.
    var big = {rows: 50, cols: 100, cells: []}
    for (var r = 0; r < 50; r++) {
      var line = []
      for (var c = 0; c < 100; c++)
        line.push({ch: (r * 7 + c * 3) % 5 === 1 ? " " : "#", fg: "#" + ("0" + (r * 5).toString(16)).slice(-2) + ("0" + (c * 2).toString(16)).slice(-2) + "80"})
      big.cells.push(line)
    }
    var t1 = AnsiArt.thumbnail(big, 6, 12), t2 = AnsiArt.thumbnail(big, 6, 12)
    check(t1.rows === 6 && t1.cols === 12 && t1.cells.length === 6 && t1.cells[5].length === 12
      && JSON.stringify(t1) === JSON.stringify(t2), "Thumbnail not deterministic or wrong shape")
    check(JSON.stringify(cell(t1, 0, 0)) === JSON.stringify(big.cells[4][4]), "Thumbnail does not sample the block centre")
    var holed = AnsiArt.parse("   \n  " + E + "[32m*\n   ")
    var single = AnsiArt.thumbnail(holed, 1, 1)
    check(single.rows === 1 && single.cols === 1 && cell(single, 0, 0).ch === "*" && cell(single, 0, 0).fg === AnsiArt.BASIC[2],
      "Blank centre did not choose the nearest non-blank cell")
    check(cell(AnsiArt.thumbnail(AnsiArt.parse("    \n    \n   x"), 2, 2), 0, 0).ch === " ", "Blank block not kept blank")
    check(AnsiArt.thumbnail(AnsiArt.parse("ab\ncd"), 6, 12).cols === 2, "Small art was enlarged")
    var field = AnsiArt.thumbnail(AnsiArt.parse(E + "[48;2;255;213;74m    \n    \n  " + E + "[38;2;1;2;3;48;2;26;27;38m#" + E + "[48;2;255;213;74m "), 2, 2)
    check(cell(field, 0, 0).ch === " " && cell(field, 0, 0).bg === "#ffd54a" && cell(field, 1, 1).ch === "#" && cell(field, 1, 1).bg === "#1a1b26",
      "Thumbnail lost backgrounds: " + JSON.stringify(field.cells))
    check(AnsiArt.blockColor(cell(field, 0, 0), "#eeeeee") === "#ffd54a" && AnsiArt.blockColor(cell(field, 1, 1), "#eeeeee") === "#010203"
      && AnsiArt.blockColor({ch: "x", fg: "", bg: null}, "#eeeeee") === "#eeeeee" && AnsiArt.blockColor({ch: " ", fg: "", bg: null}, "#eeeeee") === "",
      "Block colors wrong")

    // Plain text and the stored form. Color carries across a newline, as in a terminal.
    check(AnsiArt.toArt(AnsiArt.parse(E + "[31mab  \n" + E + "[0mc\n  \n")) === "ab\nc", "toArt wrong")
    var source = E + "[2J" + E + "[31mab" + E + "[38;5;208mc\r\n" + E + "[1md " + E + "[38;2;9;8;7me\x1aSAUCE"
    var stored = AnsiArt.sanitize(source)
    check(stored === E + "[0m" + E + "[0;38;2;192;57;43mab" + E + "[0;38;2;255;135;0mc" + E + "[0m\n"
      + E + "[0;38;2;255;135;0md " + E + "[0;38;2;9;8;7me" + E + "[0m", "Sanitized form wrong: " + JSON.stringify(stored))
    var backed = AnsiArt.sanitize(E + "[48;5;21m  " + E + "[38;2;5;6;7mx" + E + "[49my\n" + E + "[48;2;1;2;3m ")
    check(backed === E + "[0m" + E + "[0;48;2;0;0;255m  " + E + "[0;38;2;5;6;7;48;2;0;0;255mx" + E + "[0;38;2;5;6;7my" + E + "[0m\n"
      + E + "[0;48;2;1;2;3m " + E + "[0m   " && AnsiArt.sanitize(backed) === backed && AnsiArt.sanitize(E + "[44m  ") !== "",
      "Sanitized backgrounds wrong: " + JSON.stringify(backed))
    check(AnsiArt.sanitize(stored) === stored && JSON.stringify(AnsiArt.parse(stored)) === JSON.stringify(AnsiArt.parse(source)),
      "Sanitizing is not idempotent")
    check(AnsiArt.isAnsi(stored) && !AnsiArt.isAnsi("plain") && AnsiArt.storedArt("0123456789abcdef") === "0123456789ab"
      && AnsiArt.storedArt(source) === stored && AnsiArt.isAnsi(AnsiArt.sanitize("plain\nfile")), "Stored art dispatch wrong")

    // Brightness: auto-levels then a gamma lift, per color with hue kept, clamped.
    var dark = AnsiArt.parse(E + "[38;2;40;20;10mA" + E + "[38;2;20;30;40mB" + E + "[38;2;50;50;45;48;2;10;10;20mC" + E + "[0mD" + E + "[48;2;30;20;10m ")
    function light(hex) { return AnsiArt.lightness(hex) }
    var leveled = AnsiArt.adjust(dark, {brightness: 1})
    check(Math.abs(light(cell(leveled, 0, 2).fg) - 0.95) < 0.02, "Auto-levels did not bring the brightest ink to 0.95: " + light(cell(leveled, 0, 2).fg))
    check(cell(leveled, 0, 3).fg === "" && cell(leveled, 0, 3).bg === null && light(cell(leveled, 0, 2).bg) > light("#0a0a14")
      && light(cell(leveled, 0, 4).bg) > light("#1e140a"), "Theme colors changed or backgrounds not brightened")
    var r0 = parseInt(cell(leveled, 0, 0).fg.slice(1, 3), 16), g0 = parseInt(cell(leveled, 0, 0).fg.slice(3, 5), 16), b0 = parseInt(cell(leveled, 0, 0).fg.slice(5, 7), 16)
    check(r0 > g0 && g0 > b0 && Math.abs(r0 / b0 - 4) < 0.5, "Hue not kept: " + cell(leveled, 0, 0).fg)
    check(JSON.stringify(AnsiArt.adjust(dark, {brightness: 1, levels: false})) === JSON.stringify(dark) && AnsiArt.adjust(dark, {}) === dark,
      "Brightness 1 without levels changed colors")
    var steps = [0.5, 0.75, 1, 1.5, 2, 3].map(function(b) { return AnsiArt.adjust(dark, {brightness: b}) })
    for (var sc = 0; sc < 3; sc++) {
      for (var st = 1; st < steps.length; st++)
        check(light(cell(steps[st], 0, sc).fg) >= light(cell(steps[st - 1], 0, sc).fg) - 0.002, "Brightness not monotonic at cell " + sc)
      check(light(cell(steps[5], 0, sc).fg) > light(cell(steps[0], 0, sc).fg), "Brightness had no effect at cell " + sc)
    }
    var hot = AnsiArt.adjust(AnsiArt.parse(E + "[38;2;255;255;0mY" + E + "[38;2;10;0;0mZ"), {brightness: 3})
    // Yellow would pass 255 and is clamped; a near-black red is lifted many times over, still pure red.
    check(cell(hot, 0, 0).fg === "#ffff00" && /^#[0-9a-f]{2}0000$/.test(cell(hot, 0, 1).fg)
      && parseInt(cell(hot, 0, 1).fg.slice(1, 3), 16) > 200, "Channels not clamped or hue lost: " + JSON.stringify(hot.cells[0]))
    check(AnsiArt.clampBrightness(7) === 3 && AnsiArt.clampBrightness(0.1) === 0.5 && AnsiArt.clampBrightness(1.3) === 1.25
      && AnsiArt.validBrightness(1.5) && !AnsiArt.validBrightness(1.3) && !AnsiArt.validBrightness(0.25) && !AnsiArt.validBrightness("1"),
      "Brightness bounds wrong")
    check(AnsiArt.thumbnailFor(stored, 6, 12, 2) !== AnsiArt.thumbnailFor(stored, 6, 12, 1)
      && JSON.stringify(AnsiArt.thumbnailFor(stored, 6, 12, 0)) === JSON.stringify(AnsiArt.thumbnail(AnsiArt.parse(stored), 6, 12)),
      "Thumbnail does not follow brightness")

    // The fixture file: cursor movement, basic, 256 and truecolor, CRLF and SAUCE.
    test.fixtureArt = AnsiArt.sanitize(fixture.text())
    g = AnsiArt.parse(fixture.text())
    check(g.rows === 8 && g.cols === 16 && cell(g, 0, 4).fg === AnsiArt.BASIC[1] && cell(g, 7, 4).fg === "#ff8700"
      && cell(g, 1, 2).fg === "#b45078" && rowText(g, 3).indexOf("@") !== -1, "Fixture parsed wrong: " + JSON.stringify(rowText(g, 0)))
  }

  function loaderCases(loader) {
    // Each refusal is closed: the stored art is unchanged.
    var before = service.agents.avatarArtForKey(test.me)
    ;["relative/art.ans", "/tmp/../etc/art.ans", "/tmp/art.png", "", "/tmp/a\nb.ans"].forEach(function(path) {
      check(!loader.load(path) && loader.status === "failed" && loader.problem !== "", "Unsafe path accepted: " + JSON.stringify(path))
    })
    check(service.agents.avatarArtForKey(test.me) === before, "Refused path changed the avatar")
  }

  Timer {
    interval: 100
    running: true
    repeat: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 120) throw new Error("Timed out at stage " + test.stage)
        var loader = findNamed(view, "buzzMyAvatarPath", [])[0]
        var myLoader = loader ? loader.parent : null
        if (test.stage === 0) {
          test.parserCases()
          service.beginSession()
          if (!service.acceptFrame(JSON.stringify(test.frame()))) throw new Error("Fixture frame rejected")
          test.stage = 1
        } else if (test.stage === 1) {
          if (!service.notificationSettingsDirReady) return
          var avatars = test.messageAvatars()
          check(avatars.filter(function(avatar) { return avatar.usesArt }).length === 0, "Art shown before any was set")
          check(findNamed(view, "buzzMyAvatarPath", []).filter(function(item) { return item.visible }).length === 0,
            "Avatar path field shown before Set my avatar")
          test.one("buzzSetMyAvatar").clicked()
          myLoader = test.one("buzzMyAvatarPath").parent
          test.loaderCases(myLoader)
          test.one("buzzMyAvatarPath").text = test.fixturePath
          test.one("buzzMyAvatarPathApply").clicked()
          check(myLoader.status === "reading", "Loader did not start reading")
          test.stage = 2
        } else if (test.stage === 2) {
          if (myLoader.status === "reading") return
          check(myLoader.status === "loaded", "Fixture not loaded: " + myLoader.problem)
          check(service.agents.avatarArtForKey(test.me) === test.fixtureArt, "Own avatar not the sanitized fixture")
          var storeText = test.readStore()
          var stored = JSON.parse(storeText)
          check(Object.keys(stored).sort().join(",") === "avatars,version" && Object.keys(stored.avatars).join(",") === test.me
            && JSON.stringify(stored.avatars[test.me]) === JSON.stringify({art: test.fixtureArt, brightness: 1.5})
            && storeText.indexOf(test.fixturePath) === -1, "Avatar file holds unexpected data: " + storeText.slice(0, 120))
          check(service.agents.avatarBrightnessForKey(test.me) === 1.5, "New colored art did not start at brightness 1.5")
          check(test.freshOwnArt() === test.fixtureArt, "Own avatar not restored by a fresh service")
          var mine = test.myLeadAvatar()
          check(mine && mine.usesColor && mine.thumbnail.rows === 6 && mine.thumbnail.cols === 12 && mine.clickable,
            "My message avatar is not the colored thumbnail")
          test.messageAvatars().forEach(function(avatar) {
            if (avatar.key === test.other) check(!avatar.usesArt, "Another author got my avatar")
          })
          var preview = test.one("buzzMyAvatarPreview")
          check(preview.usesColor && preview.brightness === 1.5 && mine.brightness === 1.5, "Sidebar avatar not colored at 1.5")
          // − and + change the stored brightness; both thumbnails follow at once.
          var before = JSON.stringify(mine.thumbnail)
          test.one("buzzMyAvatarBrightnessUp").clicked()
          check(service.agents.avatarBrightnessForKey(test.me) === 1.75 && mine.brightness === 1.75 && preview.brightness === 1.75
            && JSON.stringify(mine.thumbnail) !== before && test.one("buzzMyAvatarBrightness").text === "Brightness 1.75",
            "Brightness + not applied live")
          for (var down = 0; down < 12; down++) test.one("buzzMyAvatarBrightnessDown").clicked()
          check(service.agents.avatarBrightnessForKey(test.me) === 0.5 && !test.one("buzzMyAvatarBrightnessDown").enabled, "Brightness not held at 0.5")
          for (var up = 0; up < 5; up++) test.one("buzzMyAvatarBrightnessUp").clicked()
          check(service.agents.avatarBrightnessForKey(test.me) === 1.75, "Brightness steps wrong")
          check(JSON.parse(test.readStore()).avatars[test.me].brightness === 1.75, "Brightness not saved")
          var fresh = Qt.createQmlObject('import "plugin" as Buzz\nBuzz.Service { autoConnect: false }', test, "freshBrightness")
          var restored = fresh.agents.avatarBrightnessForKey(test.me)
          fresh.destroy()
          check(restored === 1.75, "Brightness not restored by a fresh service: " + restored)
          test.stage = 3
        } else if (test.stage === 3) {
          // A click on the avatar opens the card; Escape closes it.
          var avatar = test.myLeadAvatar()
          input.mouseClick(avatar, avatar.width / 2, avatar.height / 2)
          var card = test.one("buzzAvatarCard")
          var art = test.one("buzzAvatarCardArt")
          var cardView = card.parent
          check(cardView.opened && cardView.cellWidth >= 4 && art.width === 16 * cardView.cellWidth
            && art.height === 8 * cardView.cellWidth * 2 && card.width <= view.width, "Card art not sized to fit")
          check(cardView.brightness === 1.75 && JSON.stringify(cardView.grid) === JSON.stringify(AnsiArt.shownGridFor(test.fixtureArt, 1.75))
            && JSON.stringify(cardView.grid) !== JSON.stringify(AnsiArt.gridFor(test.fixtureArt)), "Card colors are not the adjusted colors")
          check(test.one("buzzAvatarCardName").text === service.messageAuthorName(test.me)
            && test.one("buzzAvatarCardKey").text === "Key " + test.me.slice(0, 12) + "…", "Card name or key wrong")
          input.keyClick(Qt.Key_Escape)
          check(!cardView.opened && !card.visible, "Escape did not close the card")
          check(window.visible, "Escape closed more than the card")
          // Click outside the card closes it; a click on the card does not.
          input.mouseClick(avatar, avatar.width / 2, avatar.height / 2)
          check(cardView.opened, "Card did not reopen")
          input.mouseClick(card, card.width / 2, card.height / 2)
          check(cardView.opened, "Click on the card closed it")
          input.mouseClick(cardView, 4, 4)
          check(!cardView.opened, "Click outside did not close the card")
          // Another author's identicon avatar opens a card too, without colored art.
          var otherAvatar = test.messageAvatars().filter(function(item) { return item.key === test.other && item.visible })[0]
          input.mouseClick(otherAvatar, otherAvatar.width / 2, otherAvatar.height / 2)
          check(cardView.opened && !cardView.grid && findNamed(view, "buzzAvatarCardArt", [])[0].visible === false,
            "Identicon card wrong")
          input.keyClick(Qt.Key_Escape)
          check(!cardView.opened, "Escape did not close the identicon card")
          // An oversized file is refused before it is read; the avatar stays.
          myLoader.load(test.largePath)
          test.stage = 4
        } else if (test.stage === 4) {
          if (myLoader.status === "reading") return
          check(myLoader.status === "failed" && myLoader.problem === "The file is larger than 256 KiB."
            && service.agents.avatarArtForKey(test.me) === test.fixtureArt, "Oversized file not refused: " + myLoader.problem)
          myLoader.load(test.fixturePath.replace(/avatar\.ans$/, "missing.ans"))
          test.stage = 5
        } else if (test.stage === 5) {
          if (myLoader.status === "reading") return
          check(myLoader.status === "failed" && service.agents.avatarArtForKey(test.me) === test.fixtureArt, "Missing file not refused")
          // Damaged stored art is not trusted by a fresh service.
          avatarsFile.setText(JSON.stringify({version: 1, avatars: {[test.me]: "\x1b[31mA"}}) + "\n")
          check(test.freshOwnArt() === "", "Non-canonical stored art was trusted")
          // The earlier plain-string form is still read, shown as stored (brightness 0).
          avatarsFile.setText(JSON.stringify({version: 1, avatars: {[test.me]: test.fixtureArt}}) + "\n")
          check(test.freshOwnArt() === test.fixtureArt, "Canonical stored art not restored")
          var legacy = Qt.createQmlObject('import "plugin" as Buzz\nBuzz.Service { autoConnect: false }', test, "freshLegacy")
          var legacyBrightness = legacy.agents.avatarBrightnessForKey(test.me)
          legacy.destroy()
          check(legacyBrightness === 0, "Plain-string art got a brightness")
          ;[{art: test.fixtureArt, brightness: 1.3}, {art: test.fixtureArt, brightness: 4}, {art: test.fixtureArt, brightness: "2"},
            {art: test.fixtureArt, brightness: 2, extra: 1}, {art: "plain", brightness: 2}, {art: test.fixtureArt}].forEach(function(entry, index) {
            avatarsFile.setText(JSON.stringify({version: 1, avatars: {[test.me]: entry}}) + "\n")
            check(test.freshOwnArt() === "", "Damaged brightness entry " + index + " was trusted")
          })
          // Clear removes it here and on restore.
          test.one("buzzMyAvatarPathClear").clicked()
          check(service.agents.avatarArtForKey(test.me) === "" && !test.myLeadAvatar().usesArt, "Clear kept the avatar")
          check(JSON.stringify(JSON.parse(test.readStore()).avatars) === "{}" && test.freshOwnArt() === "", "Clear not saved")
          test.messageAvatars()
          if (test.capturePath === "") {
            console.log("PASS: ANSI art parses truecolor, 256 and basic colors and resets, drops cursor sequences, controls and SAUCE, clips at 60 x 120 and 256 KiB; keeps backgrounds; thumbnails sample block centres deterministically; brightness auto-levels, is monotonic, clamped, stepped from the sidebar, saved with the art and restored; my avatar loads from a file into the local store, shows colored in the same slot width, restores, clears and fails closed on unsafe paths, oversized or missing files and damaged state; the profile card opens on click and closes on Escape or outside click")
            Qt.quit()
            return
          }
          myLoader.load(test.capturePath)
          test.stage = 6
        } else if (test.stage === 6) {
          // Optional: render real art as my avatar to PNG files for a person to look at.
          if (myLoader.status === "reading") return
          check(myLoader.status === "loaded", "Capture art not loaded: " + myLoader.problem)
          test.stage = 7
          // The panel at twice its size, so the message row thumbnail is legible.
          var lead = test.myLeadAvatar()
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.captureDir + "/ansi-thumb.png")) { console.error("Could not save thumbnail"); Qt.exit(1); return }
            input.mouseClick(lead, lead.width / 2, lead.height / 2)
            test.stage = 8
          }, Qt.size(view.width * 2, view.height * 2))
        } else if (test.stage === 8) {
          // The card at the default brightness (1.5), then as stored in the file.
          test.stage = 9
          check(test.findNamed(view, "buzzAvatarCard", [])[0].parent.brightness === 1.5, "Capture not at brightness 1.5")
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.captureDir + "/ansi-card-bright.png")) { console.error("Could not save card"); Qt.exit(1); return }
            input.keyClick(Qt.Key_Escape)
            view.openAvatarCard(test.me, service.messageAuthorName(test.me), service.agents.avatarArtForKey(test.me), 0)
            test.stage = 10
          })
        } else if (test.stage === 10) {
          test.stage = 11
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.captureDir + "/ansi-card.png")) { console.error("Could not save card"); Qt.exit(1); return }
            console.log("PASS: capture saved")
            Qt.quit()
          })
        }
      } catch (error) {
        console.error(error.message || error)
        Qt.exit(1)
      }
    }
  }
}
