// The Settings avatar gallery: rotating (buttons and arrow keys), the category
// filter, shuffle staying in range, previews at the three avatar sizes, Save
// through the existing avatar store, and pasting valid and invalid art into
// Create your own. Synthetic frames only: no helper, relay, keys or network.
// BUZZ_GALLERY_CAPTURE (a .png path) also saves the gallery page there, and
// BUZZ_GALLERY_SHEET a contact sheet of every avatar drawn by the real avatar.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as Buzz
import "plugin/AnsiArt.js" as AnsiArt
import "plugin/AvatarLibrary.js" as AvatarLibrary

ShellRoot {
  id: test
  property int stage: 0
  property int ticks: 0
  property bool layoutWait: false
  readonly property string me: "b".repeat(64)
  readonly property string room: "11111111-1111-4111-8111-111111111111"
  readonly property string capturePath: Quickshell.env("BUZZ_GALLERY_CAPTURE")
  readonly property string sheetPath: Quickshell.env("BUZZ_GALLERY_SHEET")
  Buzz.Service { id: service; autoConnect: false }
  FloatingWindow {
    id: window
    visible: true
    implicitWidth: 760
    implicitHeight: 900
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: avatarsFile; path: Quickshell.env("XDG_STATE_HOME") + "/omarchy-buzz/avatars.json"; blockLoading: true; blockWrites: true; printErrors: false }
  TestCase { id: input; when: false; name: "AvatarGalleryInput" }

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
  function none(name) {
    if (findNamed(view, name, []).some(function(item) { return item.visible })) throw new Error("Unexpected visible " + name)
  }
  function check(condition, message) { if (!condition) throw new Error(message) }
  function settle() {
    test.layoutWait = true
    try { input.waitForRendering(view) } finally { test.layoutWait = false }
  }
  function readStore() {
    var reader = Qt.createQmlObject('import Quickshell.Io\nFileView { blockLoading: true; printErrors: false }', test, "storeReader")
    reader.path = avatarsFile.path
    var text = reader.text()
    reader.destroy()
    return text
  }
  function freshOwnArt() {
    var fresh = Qt.createQmlObject('import "plugin" as Buzz\nBuzz.Service { autoConnect: false }', test, "freshService")
    var art = fresh.agents.avatarArtForKey(test.me)
    fresh.destroy()
    return art
  }
  function frame() {
    return {version: 1, type: service.instanceId === "" ? "hello" : "status", instanceId: "gallery-fixture", generation: 1,
      capabilities: ["connection_status", "room_catalog", "room_history"],
      status: {generation: 1, connection: "authenticated", category: null, identity: test.me, relay: "wss://fixture.example/",
        catalog: {state: "partial", category: "room_catalog_partial", rooms: [{id: test.room, name: "Fixture", description: "",
          kind: "stream", participants: [], hidden: false}]},
        history: {state: "snapshot", roomId: test.room, rows: [{id: "1".repeat(64), author: test.me, time: 100, text: "Synthetic message",
          edited: false, truncated: false, unavailable: false}], hasMore: false, category: "history_completeness_unknown"}}}
  }
  function openSettings() {
    var account = one("buzzAccount")
    input.mouseClick(account, account.width / 2, account.height / 2)
    one("buzzAccountSettings").clicked()
    check(view.settingsOpen, "Settings did not open from the account menu")
    settle()
  }
  function categoryButton(name) {
    var found = findNamed(view, "buzzAvatarGalleryCategory", []).filter(function(item) { return item.categoryName === name })
    check(found.length === 1, "Category button " + JSON.stringify(name) + " missing")
    return found[0]
  }
  function storedOwn() { return service.agents.avatarArtForKey(test.me) }
  function setCustom(text) {
    var field = one("buzzAvatarCustomText")
    field.text = text
    settle()
  }

  function rotateCases(gallery) {
    var total = AvatarLibrary.count()
    check(gallery.total === total && total >= 120 && gallery.position === 0, "Gallery does not start at the first of all avatars")
    check(one("buzzAvatarGalleryCounter").text === "1 / " + total + " · " + AvatarLibrary.ENTRIES[0].category, "Counter wrong: " + one("buzzAvatarGalleryCounter").text)
    check(findNamed(view, "buzzAvatarGalleryCategory", []).length === AvatarLibrary.CATEGORIES.length + 1, "A category button is missing")
    // The previews are the real avatar at three different sizes, showing the entry.
    var bar = one("buzzAvatarGalleryPreviewBar"), messages = one("buzzAvatarGalleryPreviewMessages"), profile = one("buzzAvatarGalleryPreviewProfile")
    check(bar.art === AvatarLibrary.ENTRIES[0].art && messages.art === bar.art && profile.art === bar.art
      && bar.usesArt && messages.usesArt && profile.usesArt, "Previews do not show the first avatar")
    check(bar.pixelSize < messages.pixelSize && messages.pixelSize < profile.pixelSize, "Preview sizes do not grow: " + bar.pixelSize + " " + messages.pixelSize + " " + profile.pixelSize)
    check(!bar.clickable && bar.shownArt === AvatarLibrary.ENTRIES[0].art, "Preview is not drawn as plain art")
    // Next and Previous, wrapping at both ends.
    one("buzzAvatarGalleryNext").clicked()
    check(gallery.position === 1 && one("buzzAvatarGalleryCounter").text.indexOf("2 / " + total) === 0
      && one("buzzAvatarGalleryPreviewBar").art === AvatarLibrary.ENTRIES[1].art, "Next did not show the second avatar")
    one("buzzAvatarGalleryPrevious").clicked()
    one("buzzAvatarGalleryPrevious").clicked()
    check(gallery.position === total - 1 && one("buzzAvatarGalleryCounter").text === total + " / " + total + " · " + AvatarLibrary.ENTRIES[total - 1].category,
      "Previous did not wrap to the last avatar: " + gallery.position)
    one("buzzAvatarGalleryNext").clicked()
    check(gallery.position === 0, "Next did not wrap to the first avatar")
    // Left and Right arrow keys, once the gallery has focus.
    gallery.forceActiveFocus()
    input.keyClick(Qt.Key_Right)
    input.keyClick(Qt.Key_Right)
    check(gallery.position === 2, "Right arrow did not advance: " + gallery.position)
    input.keyClick(Qt.Key_Left)
    check(gallery.position === 1, "Left arrow did not go back: " + gallery.position)
    input.keyClick(Qt.Key_Left)
    input.keyClick(Qt.Key_Left)
    check(gallery.position === total - 1, "Left arrow did not wrap")
    gallery.position = 0
  }

  function filterCases(gallery) {
    var total = AvatarLibrary.count()
    AvatarLibrary.CATEGORIES.forEach(function(name) {
      test.categoryButton(name).clicked()
      var list = AvatarLibrary.indexes(name)
      check(gallery.category === name && gallery.total === list.length && gallery.position === 0 && list.length >= 10,
        "Filter " + name + " wrong: " + gallery.total)
      check(one("buzzAvatarGalleryCounter").text === "1 / " + list.length + " · " + name && gallery.entry.category === name, "Counter wrong in " + name)
      check(test.categoryButton(name).selected && !test.categoryButton("").selected, "Selected category not marked")
      for (var i = 0; i < list.length + 1; i++) {
        one("buzzAvatarGalleryNext").clicked()
        check(gallery.entry.category === name && gallery.position < list.length, "Next left the category " + name)
      }
      check(gallery.position === 1, "Next did not wrap inside " + name)
      for (var s = 0; s < 40; s++) {
        var before = gallery.position
        one("buzzAvatarGalleryShuffle").clicked()
        check(gallery.position >= 0 && gallery.position < list.length && gallery.position !== before && gallery.entry.category === name,
          "Shuffle left the range or stood still in " + name + ": " + before + " -> " + gallery.position)
      }
    })
    // Shuffle across everything covers a wide spread, never leaves the library.
    test.categoryButton("").clicked()
    check(gallery.category === "" && gallery.total === total && gallery.position === 0, "All did not restore the whole library")
    var seen = {}
    for (var k = 0; k < 400; k++) {
      var was = gallery.position
      one("buzzAvatarGalleryShuffle").clicked()
      check(gallery.position >= 0 && gallery.position < total && gallery.position !== was, "Shuffle out of range or repeated: " + gallery.position)
      seen[gallery.position] = true
    }
    check(Object.keys(seen).length > total / 2, "Shuffle covers too little: " + Object.keys(seen).length)
    // The filter keeps a position in range when the list shrinks.
    gallery.position = total - 1
    test.categoryButton("Food").clicked()
    check(gallery.position === 0 && gallery.entry.category === "Food", "A new filter did not restart at its first avatar")
    gallery.setCategory("No such category")
    check(gallery.total === 0 && gallery.entry === null && gallery.counter === "" && !one("buzzAvatarGallerySave").enabled, "Empty filter not handled")
    gallery.setCategory("")
    gallery.position = 0
  }

  function saveCases(gallery) {
    check(test.storedOwn() === "" && one("buzzAvatarGallerySave").enabled && one("buzzAvatarGallerySave").text === "Save", "Save not offered at the start")
    // Save applies the shown avatar through the existing path: the local store, plain art, no brightness.
    one("buzzAvatarGalleryNext").clicked()
    var entry = AvatarLibrary.ENTRIES[1]
    one("buzzAvatarGallerySave").clicked()
    check(test.storedOwn() === entry.art && service.agents.avatarBrightnessForKey(test.me) === 0, "Save did not keep the avatar: " + JSON.stringify(test.storedOwn()))
    var stored = JSON.parse(test.readStore())
    check(Object.keys(stored.avatars).join(",") === test.me && stored.avatars[test.me] === entry.art, "Avatar file holds unexpected data")
    check(test.freshOwnArt() === entry.art, "Saved avatar not restored by a fresh service")
    check(one("buzzAvatarGallerySave").text === "Saved" && !one("buzzAvatarGallerySave").enabled, "Saved state not shown")
    check(one("buzzMyAvatarPreview").art === entry.art && one("buzzMyAvatarPreview").usesArt, "The account avatar is not the saved one")
    one("buzzAvatarGalleryNext").clicked()
    check(one("buzzAvatarGallerySave").text === "Save" && one("buzzAvatarGallerySave").enabled, "Save not offered for another avatar")
    check(test.storedOwn() === entry.art, "Browsing changed the saved avatar")
    // Previous returns to the saved one, which reads as saved again.
    one("buzzAvatarGalleryPrevious").clicked()
    check(one("buzzAvatarGallerySave").text === "Saved", "Saved state lost on return")
    // A saved avatar can be replaced; every entry round-trips through the store unchanged.
    AvatarLibrary.ENTRIES.forEach(function(item, index) {
      check(service.agents.setOwnAvatarArt(test.me, item.art, 1.5) && test.storedOwn() === item.art, "Entry " + index + " not stored as written")
    })
    service.agents.setOwnAvatarArt(test.me, entry.art, 1.5)
  }

  function createCases(gallery) {
    one("buzzAvatarGalleryTabCreate").clicked()
    test.settle()
    check(gallery.mode === "create" && test.none("buzzAvatarGalleryBrowse") === undefined, "Create tab not shown")
    var status = function() { return findNamed(view, "buzzAvatarCustomStatus", [])[0] }
    check(!status().visible && !one("buzzAvatarCustomSave").enabled, "Create not empty at first")
    // Valid art: previewed at the three sizes, saved as typed (trailing blanks and edge blank lines removed).
    var heart = "\n  .-. .-.\n (   V   )\n  \\     /   \n   `. .'\n     V\n\n"
    test.setCustom(heart)
    check(status().visible && status().text === "Looks good: 5 lines of 10 columns." && one("buzzAvatarCustomSave").enabled, "Valid art not accepted: " + status().text)
    var expected = "  .-. .-.\n (   V   )\n  \\     /\n   `. .'\n     V"
    var preview = one("buzzAvatarCustomPreviewProfile")
    check(preview.art === expected && one("buzzAvatarCustomPreviewBar").usesArt && one("buzzAvatarCustomPreviewMessages").usesArt, "Custom previews wrong")
    one("buzzAvatarCustomSave").clicked()
    check(test.storedOwn() === expected && test.freshOwnArt() === expected && one("buzzAvatarCustomSave").text === "Saved" && !one("buzzAvatarCustomSave").enabled,
      "Custom art not saved: " + JSON.stringify(test.storedOwn()))
    // Invalid art says why and cannot be saved; the saved avatar stays.
    var cases = [
      {text: "0123456789abc\nx", message: "Too wide: 13 columns, at most 12."},
      {text: "1\n2\n3\n4\n5\n6\n7\n8", message: "Too tall: 8 lines, at most 6."},
      {text: "ab\tcd", message: "Invalid characters"},
      {text: "x".repeat(262145), message: "Too large: at most 256 KiB."},
      {text: "0123456789abc\n2\n3\n4\n5\n6\n7", message: "Too wide: 13 columns, at most 12.\nToo tall: 7 lines, at most 6."},
      {text: "   \n  ", message: "Nothing to save yet."}
    ]
    cases.forEach(function(item, index) {
      test.setCustom(item.text)
      var shown = status().visible ? status().text : ""
      check(item.message === "" ? shown === "" : shown.indexOf(item.message) === 0, "Case " + index + " message wrong: " + JSON.stringify(shown.slice(0, 80)))
      check(!one("buzzAvatarCustomSave").enabled && test.none("buzzAvatarCustomPreviewBar") === undefined, "Case " + index + " can be saved or previewed")
      one("buzzAvatarCustomSave").clicked()
      check(test.storedOwn() === expected, "Case " + index + " changed the avatar")
    })
    // Errors are plain text: nothing typed is interpreted as markup.
    check(status().textFormat === Text.PlainText && one("buzzAvatarCustomText").textFormat === TextEdit.PlainText, "Not plain text")
    test.setCustom("<b>hi</b> \u0007")
    check(status().text.indexOf("Invalid characters") === 0, "Markup case wrong")
    // Pasted colored art (as from a .ans file) is accepted, kept sanitized and previewed colored at the default brightness.
    var ansi = "\x1b[2J\x1b[31m####\x1b[0m\n\x1b[38;2;10;200;30m@@@@\x1b[0m\n"
    test.setCustom(ansi)
    check(status().text === "Looks good: 2 lines of 4 columns." && one("buzzAvatarCustomPreviewProfile").usesColor, "Colored art not accepted: " + status().text)
    one("buzzAvatarCustomSave").clicked()
    check(test.storedOwn() === AnsiArt.sanitize(ansi) && service.agents.avatarBrightnessForKey(test.me) === 1.5, "Colored art not saved sanitized at 1.5")
    // Edit a copy: the gallery's avatar moves into the box, unchanged and valid.
    service.agents.setOwnAvatarArt(test.me, "", 0)
    one("buzzAvatarGalleryTabGallery").clicked()
    test.settle()
    check(gallery.mode === "gallery", "Gallery tab not shown")
    gallery.position = 3
    one("buzzAvatarGalleryCustomize").clicked()
    test.settle()
    check(gallery.mode === "create" && one("buzzAvatarCustomText").text === AvatarLibrary.ENTRIES[3].art
      && status().text.indexOf("Looks good") === 0, "Edit a copy did not fill the box")
    // Arrow keys in the box belong to the box, not the gallery.
    one("buzzAvatarCustomText").forceActiveFocus()
    input.keyClick(Qt.Key_Right)
    check(gallery.position === 3, "Arrow key in the text box rotated the gallery")
    one("buzzAvatarGalleryTabGallery").clicked()
    test.settle()
    service.agents.setOwnAvatarArt(test.me, "", 0)
  }

  // Every avatar drawn by the real avatar, for a person to look at.
  function contactSheet() {
    var sheet = Qt.createQmlObject('import QtQuick\nimport QtQuick.Layouts\nimport qs.Commons\nimport "plugin" as Buzz\n'
      + 'import "plugin/AvatarLibrary.js" as Lib\n'
      + 'Rectangle { id: sheet; width: 1400; color: Color.popups.background; height: grid.implicitHeight + 24\n'
      + '  Grid { id: grid; x: 12; y: 12; width: parent.width - 24; columns: 7; spacing: 14\n'
      + '    Repeater { model: Lib.ENTRIES; delegate: Column { spacing: 2\n'
      + '      Buzz.BuzzAvatar { art: modelData.art; key: "' + test.me + '"; pixelSize: 15 }\n'
      + '      Text { text: modelData.name; color: Color.foreground; opacity: 0.6; font.pixelSize: 10; font.family: Style.font.family; textFormat: Text.PlainText } } } } }',
      window.contentItem, "contactSheet")
    sheet.visible = false
    return sheet
  }

  Timer {
    interval: 100
    running: true
    repeat: true
    onTriggered: {
      if (test.layoutWait) return
      try {
        test.ticks++
        if (test.ticks > 200) throw new Error("Timed out at stage " + test.stage)
        if (test.stage === 0) {
          service.beginSession()
          if (!service.acceptFrame(JSON.stringify(test.frame()))) throw new Error("Fixture frame rejected")
          test.stage = 1
        } else if (test.stage === 1) {
          if (!service.notificationSettingsDirReady) return
          test.none("buzzAvatarGallery")
          test.openSettings()
          var gallery = test.one("buzzAvatarGallery")
          test.rotateCases(gallery)
          test.filterCases(gallery)
          test.saveCases(gallery)
          test.createCases(gallery)
          check(test.storedOwn() === "", "Test left an avatar behind")
          if (test.capturePath === "" && test.sheetPath === "") {
            console.log("PASS: the avatar gallery rotates with Previous, Next and the arrow keys, wraps at both ends, filters by category, shuffles inside the filtered list, previews the real avatar at bar, message and profile sizes, saves a library entry through the avatar store, and checks pasted art live (valid, too wide, too tall, invalid characters, too large, colored) before saving it")
            Qt.quit()
            return
          }
          // For the picture: a few categories in, Cat-like entry first.
          gallery.setCategory("Animals")
          gallery.position = 0
          test.stage = 2
        } else if (test.stage === 2) {
          test.stage = 3
          test.settle()
          if (test.capturePath === "") return
          view.grabToImage(function(result) {
            if (!result.saveToFile(test.capturePath)) { console.error("Could not save the gallery picture"); Qt.exit(1); return }
            console.log("PASS: gallery picture saved")
            test.stage = 4
          })
        } else if (test.stage === 4) {
          test.stage = 5
          if (test.sheetPath === "") { Qt.quit(); return }
          var sheet = test.contactSheet()
          sheet.visible = true
          test.settle()
          sheet.grabToImage(function(result) {
            if (!result.saveToFile(test.sheetPath)) { console.error("Could not save the contact sheet"); Qt.exit(1); return }
            console.log("PASS: contact sheet saved")
            Qt.quit()
          })
        } else if (test.stage === 3 && test.sheetPath !== "" && test.capturePath === "") {
          test.stage = 4
        }
      } catch (error) {
        console.error(error.message || error)
        Qt.exit(1)
      }
    }
  }
}
