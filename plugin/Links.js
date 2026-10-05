.pragma library

// Link detection for message text. Messages are plain text; this finds the
// http(s) URLs in them so LinkText can make exactly those clickable. It is
// deliberately conservative: a URL that is not clearly a URL is left as text.
//
// Rules, kept in step with the helper's own check (helper/src/links.rs, which
// validates again before anything is launched):
//   - only http:// and https:// (any other scheme is never a link);
//   - at most MAX_URL characters (a longer one is not shortened, it is skipped);
//   - no credentials (`user@`), no backslash, no IPv6 literal, no invisible or
//     direction-changing character, no whitespace;
//   - the host is plain ASCII: it starts with a letter or digit, has no empty
//     label, and uses only letters, digits, `.` and `-`. A host with any
//     non-ASCII character (an international name, a full-width or decomposed
//     lookalike) is shown as text and is never a link, so the host shown is
//     exactly the one the helper launches;
//   - trailing `.,;:!?` and unbalanced `)` or `]` are not part of the link.

var MAX_URL = 2048

// Characters that end a URL: whitespace, markup-ish punctuation, controls, and
// invisible or direction-changing formatting characters.
var URL_RUN = /https?:\/\/[^\s<>"'`\\^{}|\u0000-\u001f\u007f-\u009f\u061c\u180e\u200b-\u200f\u2028\u2029\u202a-\u202e\u2060-\u2064\u2066-\u206f\ufeff\ufff9-\ufffb]+/gi
var TRAILING = ".,;:!?"

function count(text, character) {
  var n = 0
  for (var i = 0; i < text.length; i++) if (text[i] === character) n++
  return n
}

function trimTrailing(url) {
  while (url.length > 0) {
    var last = url[url.length - 1]
    if (TRAILING.indexOf(last) !== -1) url = url.slice(0, -1)
    else if (last === ")" && count(url, ")") > count(url, "(")) url = url.slice(0, -1)
    else if (last === "]" && count(url, "]") > count(url, "[")) url = url.slice(0, -1)
    else break
  }
  return url
}

// The authority part (between `://` and the first `/`, `?` or `#`).
function authority(url) {
  var match = /^https?:\/\/([^\/?#]*)/i.exec(url)
  return match ? match[1] : ""
}

// host and port of an acceptable URL, or null.
function hostParts(url) {
  var auth = authority(url)
  if (auth === "" || auth.indexOf("@") !== -1 || auth.indexOf("%") !== -1 || auth[0] === "[" || /[^\x21-\x7e]/.test(auth)) return null
  var match = /^([^:]+)(?::([0-9]{1,5}))?$/.exec(auth)
  if (!match) return null
  var host = match[1]
  if (match[2] !== undefined && Number(match[2]) > 65535) return null
  // ASCII letters, digits, `.` and `-` only; labels are not empty.
  if (!/^[A-Za-z0-9](?:[A-Za-z0-9.\-]*[A-Za-z0-9])?$/.test(host)) return null
  if (host.indexOf("..") !== -1) return null
  return {host: host, port: match[2] === undefined ? "" : match[2]}
}

// True for a URL this panel would make a link.
function acceptable(url) {
  return typeof url === "string" && url.length <= MAX_URL && /^https?:\/\//i.test(url) && hostParts(url) !== null
    && !/[\s\u0000-\u001f\u007f-\u009f\\\u061c\u180e\u200b-\u200f\u2028\u2029\u202a-\u202e\u2060-\u2064\u2066-\u206f\ufeff\ufff9-\ufffb]/.test(url)
}

// [{start, end, url}] in order; `end` is exclusive. Never overlaps.
function detect(text) {
  var found = []
  if (typeof text !== "string" || text.indexOf("://") === -1) return found
  URL_RUN.lastIndex = 0
  var match
  while ((match = URL_RUN.exec(text)) !== null) {
    var start = match.index
    // "xhttp://" is not a link to http://.
    if (start > 0 && /[A-Za-z0-9+.\-]/.test(text[start - 1])) continue
    if (match[0].length > MAX_URL + 64) continue
    var url = trimTrailing(match[0])
    if (!acceptable(url)) continue
    found.push({start: start, end: start + url.length, url: url})
  }
  return found
}

function escape(text) {
  return String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;").replace(/'/g, "&#39;")
}

// The only rich text this panel produces from message text. Every character
// outside a link is escaped, and a link is `<a href="buzz-link:N">` around its
// escaped text, where N indexes `urls`; the URL itself is never an href. No
// other tag can appear, so nothing in a message can be an image, a style, a
// link of its own or any other markup. `color` must be #rrggbb (it comes from
// the theme, never from a message) or the link keeps the default colour.
function render(text, color) {
  var links = detect(text)
  var style = typeof color === "string" && /^#[0-9a-fA-F]{6}$/.test(color) ? " style=\"color:" + color + "\"" : ""
  var html = ""
  var cursor = 0
  var urls = []
  links.forEach(function(link, index) {
    html += escape(text.slice(cursor, link.start))
    html += "<a href=\"buzz-link:" + index + "\"" + style + ">" + escape(link.url) + "</a>"
    urls.push(link.url)
    cursor = link.end
  })
  html += escape(text.slice(cursor))
  return {html: "<span style=\"white-space: pre-wrap\">" + html + "</span>", urls: urls}
}

// The index N of an href made by render(), or -1 for anything else.
function indexOf(href, count) {
  var match = /^buzz-link:(0|[1-9][0-9]{0,3})$/.exec(typeof href === "string" ? href : "")
  if (!match) return -1
  var index = Number(match[1])
  return index < count ? index : -1
}

// --- Host display -----------------------------------------------------------

// The host a link goes to: the ASCII host exactly as the helper will launch it
// (lower case). A host with any non-ASCII character is never a link, so there
// is no international-name conversion to disagree with the helper's.
function hostOf(url) {
  var parts = acceptable(url) ? hostParts(url) : null
  if (!parts) return {host: "", port: ""}
  return {host: parts.host.toLowerCase(), port: parts.port}
}

// The text of a link's tooltip, before PlainText.tip(): the host first, then
// the whole URL.
function describe(url) {
  var parts = hostOf(url)
  if (parts.host === "") return ""
  return "Opens " + parts.host + (parts.port !== "" ? ":" + parts.port : "") + "\n" + url
}
