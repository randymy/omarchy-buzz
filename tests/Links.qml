// Clickable links against a synthetic stdio helper: nothing is ever opened.
// Only http(s) URLs in message text become links, trailing punctuation is left
// out, the text of a link is the URL as typed, and hostile text (markup, a
// javascript: anchor, entities, a right-to-left override) renders as literal
// text. A click asks the helper for `browser`, a shift-click for `floating`; a
// right-click on a link shows the link menu (Open, Open floating, Copy link,
// Ask agent about this), a right-click elsewhere the message's own actions.
// The tooltip shows the host the link really goes to.
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "plugin" as Buzz
import "plugin/Links.js" as Links
import "plugin/PlainText.js" as PlainText

ShellRoot {
  id: test
  property int step: -1
  property int ticks: 0
  property bool started: false
  readonly property string mixed: "a".repeat(64)
  readonly property string hostile: "b".repeat(64)
  readonly property string plain: "c".repeat(64)
  readonly property string idn: "d".repeat(64)
  readonly property string query: "e".repeat(64)
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  Buzz.Service { id: sample; autoConnect: false; sampleMode: true }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 700
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  function visibleItem(item) {
    for (var p = item; p; p = p.parent) if (!p.visible) return false
    return true
  }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function findMessages(item, found) {
    if (item.actionsOpen !== undefined && item.row && item.row.id) found.push(item)
    for (var i = 0; i < item.children.length; i++) findMessages(item.children[i], found)
    return found
  }
  function message(id) {
    var found = findMessages(view, []).filter(function(item) { return item.row.id === id })
    if (found.length !== 1) throw new Error("Expected one message " + id.slice(0, 1) + ", found " + found.length)
    return found[0]
  }
  function inside(id, name) { return findNamed(message(id), name, []).filter(visibleItem) }
  function one(id, name) {
    var found = inside(id, name)
    if (found.length !== 1) throw new Error("Expected one visible " + name + " on " + id.slice(0, 1) + ", found " + found.length)
    return found[0]
  }
  function check(condition, text) { if (!condition) throw new Error(text) }
  function same(actual, expected, text) {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error(text + ": " + JSON.stringify(actual) + " is not " + JSON.stringify(expected))
  }
  function body(id) { return one(id, "buzzMessageBody") }
  function click(item) { item.clicked() }
  function requests() {
    record.reload()
    return JSON.parse(record.text() || '{"requests":[]}').requests.filter(function(r) { return r.type === "open_link" })
  }
  // Where the middle of a link is, and where plain text is, in the message's own coordinates.
  function pointOf(id, needle) {
    var item = body(id)
    var at = item.shownText.indexOf(needle)
    check(at >= 0, "Text not shown: " + needle)
    var rect = item.positionToRectangle(at + Math.min(2, needle.length - 1))
    return item.mapToItem(message(id), rect.x + 1, rect.y + rect.height / 2)
  }
  function rightClick(id, needle) {
    var point = pointOf(id, needle)
    return message(id).contextAt(point.x, point.y)
  }
  function inert(text) {
    // Whatever the text, the only tags are the wrapper and numbered anchors.
    var html = Links.render(text, "#112233").html
    var tags = html.match(/<[^>]*>/g) || []
    return tags.every(function(tag) {
      return tag === "<span style=\"white-space: pre-wrap\">" || tag === "</span>" || tag === "</a>"
        || /^<a href="buzz-link:[0-9]+" style="color:#112233">$/.test(tag)
    })
  }
  function unitChecks() {
    var rlo = String.fromCharCode(0x202e), zwsp = String.fromCharCode(0x200b), ideo = String.fromCharCode(0x3002)
    var urls = function(text) { return Links.detect(text).map(function(l) { return l.url }) }
    same(urls("see https://a.example/x, ok"), ["https://a.example/x"], "comma")
    same(urls("(https://a.example/x)"), ["https://a.example/x"], "parentheses")
    same(urls("https://a.example/w_(x) and https://a.example/y)."), ["https://a.example/w_(x)", "https://a.example/y"], "balanced parentheses")
    same(urls("really? https://a.example/x?!"), ["https://a.example/x"], "question marks")
    same(urls("HTTP://A.Example:8080/x;"), ["HTTP://A.Example:8080/x"], "scheme case, port, semicolon")
    same(urls("javascript:alert(1) JaVaScRiPt:x file:///etc/passwd data:text/html,x ftp://a.example/ mailto:a@b.example buzz-link:0 vbscript:x //a.example www.a.example"), [], "other schemes")
    same(urls("xhttps://a.example/ 1http://a.example/"), [], "scheme inside a word")
    same(urls("https://user@a.example/ https://u:p@a.example/ https://a.example@b.example/"), [], "credentials")
    same(urls("https:/// https://. https://-a.example/ https://a..example/ https://a.example:99999/ http://[::1]/ https:// x"), [], "bad hosts")
    same(urls("https://a.example/" + rlo + "gpj.exe"), ["https://a.example/"], "override ends a link")
    same(urls("https://a.example/a" + zwsp + "b"), ["https://a.example/a"], "zero width ends a link")
    same(urls("https://a" + ideo + "example/ https://a.example\\evil"), ["https://a.example"], "IDNA dot, backslash")
    same(urls("https://a.example/" + "x".repeat(2048)), [], "over-long")
    check(urls("https://a.example/" + "x".repeat(2000)).length === 1, "A long URL within the cap was refused")
    same(Links.hostOf("https://B" + String.fromCharCode(0xfc) + "cher.Example:81/x").host, "xn--bcher-kva.example", "punycode host")
    same(Links.hostOf("https://Example.COM/x").host, "example.com", "lower-case host")
    var cases = ["<img src=x>", "<a href=\"javascript:alert(1)\">x</a>", "&lt;b&gt;", "&amp;lt;", "\" onclick=\"x", "<style>a{}</style>", "<script>x</script>",
      "<img src=\"https://evil.example/p.png\">", "https://a.example/?q=<b>&x=\"1\"", "'><svg onload=1>", rlo + "x", "</a><a href=\"https://b.example/\">", "&#60;img&#62;",
      "https://a.example/\"><img src=https://b.example/p.png>", "<a href=\"buzz-link:0\">https://a.example/</a>", "<font color=red>x</font>", "<br><hr><table>"]
    cases.forEach(function(text) { check(inert(text), "Markup survived rendering: " + text) })
    // Only a link is ever an anchor, and its href is an index.
    var html = Links.render("<a href=\"https://evil.example/\">x</a> https://a.example/", "#112233").html
    check((html.match(/<a /g) || []).length === 2 && html.indexOf("href=\"http") === -1 && html.indexOf("&lt;a href=\"") === -1 && html.indexOf("&lt;a href=&quot;") !== -1, "A foreign anchor survived")
    check(Links.render("x https://a.example/", "red\"onclick=\"x").html.indexOf("onclick") === -1, "A non-hex colour reached the markup")
    same(Links.indexOf("buzz-link:1", 2), 1, "index")
    same([Links.indexOf("buzz-link:2", 2), Links.indexOf("https://a.example/", 2), Links.indexOf("buzz-link:01", 2), Links.indexOf("buzz-link:-1", 2), Links.indexOf("", 2), Links.indexOf(undefined, 2)], [-1, -1, -1, -1, -1, -1], "foreign hrefs")
    // Sample data and a service that cannot open links make no requests.
    check(!sample.openLink("x", "https://a.example/", "browser"), "Sample mode opened a link")
    check(!service.openLink("x", "javascript:alert(1)", "browser") && !service.openLink("x", "https://a.example/", "shell")
      && !service.openLink("x", "https://a.example/\n", "browser") && !service.openLink("x", "file:///etc/passwd", "floating"), "A bad link or mode was sent")
  }
  readonly property var steps: [
    // 0: connected, rows shown.
    {until: function() { return service.connection === "authenticated" && service.linksSupported && service.historyState === "snapshot" && findMessages(view, []).length >= 5 }},
    // 1: what is a link.
    {run: function() {
      unitChecks()
      var mixedText = "Docs at https://example.com/docs, and (https://example.com/a_(b)). Also see https://omarchy.org!"
      var item = body(test.mixed)
      same(item.urls, ["https://example.com/docs", "https://example.com/a_(b)", "https://omarchy.org"], "links in mixed text")
      check(item.hasLinks && item.shownText === mixedText, "The text shown is not the text sent: " + item.shownText)
      check(item.textFormat === TextEdit.RichText, "A text with links is not rendered with links")
      // Plain text stays plain text and keeps its markup characters.
      var none = body(test.plain)
      check(!none.hasLinks && none.textFormat === TextEdit.PlainText && none.text === "no links here, just <b>markup</b> and &amp; things", "A text without links was altered")
      // The hostile message: two links at most, and every other character literal.
      var bad = body(test.hostile)
      var literal = "<img src=\"https://evil.example/x.png\"> <a href=\"javascript:alert(1)\">click</a> &lt;b&gt;bold&lt;/b&gt; "
        + "javascript:alert(1) file:///etc/passwd data:text/html,hi ftp://ftp.example/x mailto:a@example.com " + String.fromCharCode(0x202e) + "txt.exe https://ok.example/path"
      check(bad.shownText === literal, "Hostile text was not shown literally: " + bad.shownText)
      same(bad.urls, ["https://evil.example/x.png", "https://ok.example/path"], "links in hostile text")
      check(bad.text.indexOf("<img") === -1 && bad.text.indexOf("javascript:alert(1)\">") === -1, "Markup reached the renderer")
      check(!/javascript|file:|data:|ftp:|mailto:/.test(JSON.stringify(bad.urls)), "A non-http scheme became a link")
      // Geometry: a link is found under its own text and not under plain text.
      var item2 = body(test.mixed)
      var rect = item2.positionToRectangle(item2.shownText.indexOf("https://omarchy.org") + 3)
      same(item2.linkIndexAt(rect.x + 1, rect.y + rect.height / 2), 2, "link under the pointer")
      var first = item2.positionToRectangle(2)
      same(item2.linkIndexAt(first.x + 1, first.y + first.height / 2), -1, "plain text under the pointer")
    }, until: function() { return true }},
    // 2: left click, shift-click.
    {run: function() {
      var item = body(test.mixed)
      check(item.choose(0, false) && item.choose(2, true), "Link choice refused")
      check(!item.choose(3, false) && !item.choose(-1, false), "A missing link was chosen")
    }, until: function() { return test.requests().length >= 2 }},
    {run: function() {
      var sent = test.requests()
      same(sent.map(function(r) { return r.mode + " " + r.url }), ["browser https://example.com/docs", "floating https://omarchy.org"], "click requests")
      check(sent.every(function(r) { return service.uuidValue(r.id) && r.generation === 3 && r.instanceId === "links-fixture" }), "Request scope wrong")
      check(inside(test.mixed, "buzzLinkNote").length === 0, "An opened link left a note")
    }, until: function() { return true }},
    // 4: tooltip: the host first, then the whole URL, escaped.
    {run: function() {
      var item = body(test.mixed)
      item.hoveredIndex = 1
      check(item.tipSource === "Opens example.com\nhttps://example.com/a_(b)", "Tooltip wrong: " + item.tipSource)
      item.hoveredIndex = -1
      check(item.tipSource === "", "Tooltip stayed")
      var q = body(test.query)
      q.hoveredIndex = 0
      check(q.tipSource === "Opens example.com\nhttps://example.com/?a=1&b=2", "Query tooltip wrong: " + q.tipSource)
      check(PlainText.tip(q.tipSource).indexOf("&amp;b=2") !== -1 && PlainText.tip("<img src=x>").indexOf("<img") === -1, "Tooltip text is not escaped")
      q.hoveredIndex = -1
      var i = body(test.idn)
      i.hoveredIndex = 0
      check(/^Opens xn--bcher-kva\.example \(international name, shown as punycode\)\nhttps:\/\/b.cher\.example\/x$/.test(i.tipSource), "IDN tooltip wrong: " + i.tipSource)
      i.hoveredIndex = -1
    }, until: function() { return true }},
    // 5: right-click on a link: the link menu, not the message actions.
    {run: function() {
      check(inside(test.mixed, "buzzLinkMenu").length === 0, "Link menu open early")
      same(test.rightClick(test.mixed, "https://example.com/docs"), "link", "right-click on a link")
      check(message(test.mixed).linkMenu === 0 && inside(test.mixed, "buzzLinkMenu").length === 1, "Link menu not shown")
      check(inside(test.mixed, "buzzMessageActions").length === 0, "Message actions shown for a link")
      check(one(test.mixed, "buzzLinkMenuTarget").text === "Opens example.com\nhttps://example.com/docs", "Menu target wrong: " + one(test.mixed, "buzzLinkMenuTarget").text)
      same(["buzzLinkOpen", "buzzLinkOpenFloating", "buzzLinkCopy", "buzzLinkAsk"].map(function(n) { return inside(test.mixed, n).length }), [1, 1, 1, 1], "menu items")
      same([one(test.mixed, "buzzLinkOpen").text, one(test.mixed, "buzzLinkOpenFloating").text, one(test.mixed, "buzzLinkCopy").text, one(test.mixed, "buzzLinkAsk").text],
        ["Open", "Open floating", "Copy link", "Ask agent about this"], "menu labels")
      click(one(test.mixed, "buzzLinkOpen"))
      check(inside(test.mixed, "buzzLinkMenu").length === 0, "Menu stayed open after Open")
    }, until: function() { return test.requests().length >= 3 }},
    {run: function() {
      same(test.rightClick(test.mixed, "https://example.com/a_(b)"), "link", "right-click on the second link")
      check(message(test.mixed).linkMenu === 1, "Wrong link under the menu")
      click(one(test.mixed, "buzzLinkOpenFloating"))
    }, until: function() { return test.requests().length >= 4 }},
    {run: function() {
      test.rightClick(test.mixed, "https://omarchy.org")
      click(one(test.mixed, "buzzLinkCopy"))
      same(body(test.mixed).lastCopied, "https://omarchy.org", "copied link")
      check(test.requests().length === 4, "Copy link asked the helper")
      // Right-click again closes the menu; the menu can also be closed.
      test.rightClick(test.mixed, "https://omarchy.org")
      same(test.rightClick(test.mixed, "https://omarchy.org"), "link", "second right-click")
      check(inside(test.mixed, "buzzLinkMenu").length === 0, "Second right-click did not close the menu")
      test.rightClick(test.mixed, "https://omarchy.org")
      click(one(test.mixed, "buzzLinkMenuClose"))
      check(inside(test.mixed, "buzzLinkMenu").length === 0, "Close left the menu open")
      test.rightClick(test.mixed, "https://omarchy.org")
      click(one(test.mixed, "buzzLinkAsk"))
    }, until: function() { return service.linkNote !== "" && test.requests().length >= 5 }},
    {run: function() {
      var sent = test.requests()
      same(sent.map(function(r) { return r.mode + " " + r.url }), ["browser https://example.com/docs", "floating https://omarchy.org", "browser https://example.com/docs", "floating https://example.com/a_(b)", "agent https://omarchy.org"], "menu requests (note: " + service.linkNote + ")")
      check(/No default agent/.test(one(test.mixed, "buzzLinkNote").text), "A missing default agent was not said plainly: " + service.linkNote)
      check(inside(test.hostile, "buzzLinkNote").length === 0 && inside(test.plain, "buzzLinkNote").length === 0, "The note appeared on another message")
    }, until: function() { return true }},
    // 8: right-click off a link: the message's own actions, as before.
    {run: function() {
      var item = body(test.mixed)
      var off = item.mapToItem(message(test.mixed), 2, item.height / 2)
      var before = message(test.mixed).actionsOpen
      same(message(test.mixed).contextAt(off.x, off.y), "message", "right-click off a link")
      check(message(test.mixed).actionsOpen !== before && message(test.mixed).linkMenu === -1, "Right-click off a link did not toggle the message actions")
      // A message with reactions offers them; the plain text message too.
      var plainPoint = test.pointOf(test.plain, "links")
      same(message(test.plain).contextAt(plainPoint.x, plainPoint.y), "message", "right-click in a message without links")
      check(inside(test.plain, "buzzMessageActions").length === 1 && inside(test.plain, "buzzLinkMenu").length === 0, "Actions did not open on a message without links")
    }, until: function() { return true }},
    {run: function() {
      var sent = test.requests()
      check(sent.length === 5 && sent.every(function(r) { return ["browser", "floating", "agent"].indexOf(r.mode) !== -1 && /^https:\/\//.test(r.url) && Object.keys(r).length === 7 }), "Unexpected requests: " + JSON.stringify(sent))
      console.log("PASS: only http(s) links are links (trailing punctuation and unbalanced parentheses left out, other schemes, credentials and over-long URLs never); the text of a link is the URL as typed; markup, javascript: anchors, entities and a right-to-left override render as literal text and no tag but numbered anchors can appear; click asks for browser and shift-click for floating; right-click on a link shows Open, Open floating, Copy link and Ask agent about this, each sending the right request or copying; right-click elsewhere keeps the message actions; the tooltip shows the host (punycode for international names) and the URL; a missing default agent is said plainly")
      Qt.quit()
    }, until: function() { return true }}
  ]
  property bool ran: false
  Timer {
    interval: 100
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 140) throw new Error("Links timed out at step " + test.step + " " + service.connection + " " + service.linksSupported)
        if (!test.started) { service.retry(); test.started = true; test.step = 0; return }
        var current = test.steps[test.step]
        if (!test.ran) {
          if (current.until && !current.run && !current.until()) return
          if (current.run) current.run()
          test.ran = true
          return
        }
        if (current.until()) { test.step++; test.ran = false }
      } catch (error) { console.error(error.message || error); Qt.exit(1) }
    }
  }
}
