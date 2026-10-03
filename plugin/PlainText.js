.pragma library

// Controls.ToolTip renders its text as Text.AutoText and has no textFormat of
// its own, so markup in relay-supplied text (a status, name, description or
// link) would be rendered, and an <img> would fetch a remote host on hover.
// Every tooltip goes through tip(): the text is escaped and wrapped in one
// element, so it is always shown literally and can load nothing.
function tip(value) {
  var text = value === undefined || value === null ? "" : String(value)
  if (text === "")
    return ""
  var escaped = text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;").replace(/'/g, "&#39;")
  return "<span style=\"white-space: pre-wrap\">" + escaped + "</span>"
}
