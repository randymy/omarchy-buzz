// Desktop notification wording and policy, following Buzz Desktop's
// formatMessageNotification. Pure presentation of the helper's bounded notice;
// the helper already classified, sanitized and truncated it.
var modes = ["direct", "mentions", "dms", "all", "none"];

// Which kinds each preference lets through. "thread" is a reply in a thread I
// am in (or a broadcast reply); "room" is any other observed room message.
var allowed = {
  direct: ["mention", "dm", "thread"],
  mentions: ["mention", "thread"],
  dms: ["dm"],
  all: ["mention", "dm", "thread", "room"],
  none: []
};

function allows(mode, kind) {
  return Object.prototype.hasOwnProperty.call(allowed, mode) && allowed[mode].indexOf(kind) !== -1;
}

// The Omarchy sender script reads a leading "-" word as an option or --exec.
// Relay text is one argument, so a space keeps it from ever matching a flag.
function arg(text) {
  var value = String(text);
  return value.charAt(0) === "-" ? " " + value : value;
}

// The Omarchy notification server renders the body as styled text (tags, and
// images that would be fetched), so message text is escaped to stay literal.
function escapeBody(text) {
  return String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function plural(count) {
  return count + " new messages";
}

// notice: {kind, count, roomName, sender, snippet}. `dm` is the catalog's view
// of the room. Counts are what this session observed, not unread totals.
function compose(notice, dm, includeText) {
  var sender = notice.sender || "";
  var room = notice.roomName ? "#" + notice.roomName : "";
  var title;
  if (dm) title = sender ? sender + " (direct message)" : notice.roomName ? notice.roomName + " (direct message)" : "Direct message";
  else if (notice.kind === "mention") title = sender ? sender + " mentioned you" + (room ? " in " + room : "") : room ? "Mention in " + room : "Mention";
  else if (notice.kind === "thread") title = sender ? sender + " replied" + (room ? " in " + room : "") : room ? "Reply in " + room : "Reply";
  else title = sender ? sender + (room ? " in " + room : "") : room || "New message";
  var body;
  var fallback = notice.kind === "thread" && !dm ? "New reply" : "New message";
  if (notice.count > 1) {
    body = plural(notice.count);
    if (includeText && notice.snippet) body += ". Latest: " + notice.snippet;
  } else if (includeText && notice.snippet) body = notice.snippet;
  else body = sender ? fallback + " from " + sender : fallback;
  return {title: title, body: escapeBody(body)};
}
