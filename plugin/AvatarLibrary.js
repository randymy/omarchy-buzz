.pragma library
// The built-in avatar gallery: original plain ASCII art, each at most 6 lines
// of 12 columns (the size Identicon.normalizeArt keeps), printable ASCII only,
// in the exact stored form (no trailing blanks or blank lines). No escape
// sequences, so every entry is plain art: the bar, message and profile
// avatars draw it as text. Pure data and helpers; nothing is read or sent.
// Each piece is original line art, drawn for this plugin.
//
// tests/avatar_library.cjs checks every entry against the real validator.

var ROWS = 6
var COLUMNS = 12

var CATEGORIES = ["Animals", "Faces and characters", "Robots and tech", "Nature", "Objects", "Symbols and abstract", "Food", "Space", "Geometric patterns", "Retro and game"]

var ENTRIES = []
function add(category, name, rows) { ENTRIES.push({category: category, name: name, art: rows.join("\n")}) }

// Animals
add("Animals", "Cat", [
  " /\\_/\\",
  "( o.o )",
  " > ^ <"])
add("Animals", "Dog", [
  "/^\\    /^\\",
  "|  \\__/  |",
  "| o    o |",
  "\\   ()   /",
  " \\  --  /",
  "  `----'"])
add("Animals", "Owl", [
  "/\\    /\\",
  "|(o)(o)|",
  "\\  \\/  /",
  " )    (",
  "/|\"\"\"\"|\\",
  " ^^  ^^"])
add("Animals", "Fish", [
  "     ,",
  "  ,-' \\  /|",
  "<(  o  )< |",
  "  `-.,/  \\|"])
add("Animals", "Rabbit", [
  "(\\_/)",
  "(o.o)",
  "(> <)"])
add("Animals", "Mouse", [
  " _     _",
  "(_)---(_)",
  " ( o.o )",
  "  \\_^_/",
  "  ~~~~>"])
add("Animals", "Frog", [
  " (o)(o)",
  "/  --  \\",
  " \\____/ )",
  "^^    ^^"])
add("Animals", "Penguin", [
  "   .---.",
  "  / o o \\",
  " (  ,v,  )",
  " /|     |\\",
  "(_|     |_)",
  "   ^   ^"])
add("Animals", "Duck", [
  "  __",
  "<(o )___",
  " ( ._> /",
  "  `---'"])
add("Animals", "Snake", [
  " .---.",
  "( o o )__",
  " \\___   \\",
  " ___)  _/",
  "/  ____/",
  "\\_/"])
add("Animals", "Turtle", [
  "  ___   ,",
  "_/___\\_(o>",
  "\\_____/ ~",
  " U   U"])
add("Animals", "Bear", [
  "(\\_  _/)",
  "(  \\/  )",
  "( o  o )",
  "(  ()  )",
  " \\_--_/"])
add("Animals", "Fox", [
  " /\\      /\\",
  "/  \\____/  \\",
  "\\  o    o  /",
  " \\   /\\   /",
  "  \\_ vv _/",
  "    `--'"])
add("Animals", "Hedgehog", [
  "  ,^^^^^,",
  " /^^^^^^^\\",
  "<^^^^^^o_o>",
  "  ` ` ` `"])

// Faces and characters
add("Faces and characters", "Smile", [
  "  .-\"\"\"-.",
  " / o   o \\",
  "|    <    |",
  " \\  \\_/  /",
  "  `-...-'"])
add("Faces and characters", "Wink", [
  "  .-\"\"\"-.",
  " / o   - \\",
  "|    <    |",
  " \\  \\_/  /",
  "  `-...-'"])
add("Faces and characters", "Cool", [
  "  .-\"\"\"-.",
  " /       \\",
  "| [=] [=] |",
  " \\  \\_/  /",
  "  `-...-'"])
add("Faces and characters", "Surprised", [
  "  .-\"\"\"-.",
  " / O   O \\",
  "|    <    |",
  " \\   O   /",
  "  `-...-'"])
add("Faces and characters", "Ninja", [
  " .-----.",
  "| ##### |",
  "|-(o)(o)-|",
  "| ##### |",
  " `-----'"])
add("Faces and characters", "Wizard", [
  "   /\\",
  "  /**\\",
  " /____\\",
  "( o  o )",
  "( \\__/ )",
  " \\~~~~/"])
add("Faces and characters", "Pirate", [
  "  ______",
  " /######\\",
  "|#(X)_ o |",
  " \\  __  /",
  "  `-vv-'"])
add("Faces and characters", "Knight", [
  " .-^-.",
  "/|===|\\",
  "||o o||",
  "||___||",
  " \\___/"])
add("Faces and characters", "Viking", [
  " \\/    \\/",
  " .-\"\"\"\"-.",
  "/  o  o  \\",
  "| \\====/ |",
  " \\  vv  /",
  "  `----'"])
add("Faces and characters", "Cowboy", [
  "  __,__",
  "_/_____\\_",
  "  .---.",
  " ( o o )",
  "  \\ ~ /",
  "   `-'"])
add("Faces and characters", "Ghost", [
  " .---.",
  "/ o o \\",
  "|  O  |",
  "|     |",
  "/\\/\\/\\/\\"])
add("Faces and characters", "Detective", [
  "  .---.",
  " /#####\\",
  "=========",
  " ( o o )",
  " ( \\_/ )o",
  "  `---'"])
add("Faces and characters", "Chef", [
  " ,-@@-.",
  "(@@@@@@)",
  " |____|",
  "( o  o )",
  " \\ -- /",
  "  `--'"])
add("Faces and characters", "Sprout Kid", [
  "   \\|/",
  "  .-+-.",
  " / o o \\",
  "|   v   |",
  " \\ --- /",
  "  `---'"])
add("Faces and characters", "Sleepy", [
  "  .-\"\"\"-.",
  " / -   - \\ z",
  "|    <    |z",
  " \\   ~   /",
  "  `-...-'"])

// Robots and tech
add("Robots and tech", "Robot", [
  "   |",
  " .-+-.",
  "[ o o ]",
  "[ === ]",
  " '-+-'",
  " _/ \\_"])
add("Robots and tech", "Retro Robot", [
  "  _[]_",
  " |o  o|",
  " |_||_|",
  "=|[::]|=",
  " |____|",
  "  d  b"])
add("Robots and tech", "Chip", [
  "  |||||",
  " .-----.",
  "-| CPU |-",
  "-|     |-",
  " `-----'",
  "  |||||"])
add("Robots and tech", "Laptop", [
  " .-----.",
  " | >_  |",
  " |     |",
  " `-----'",
  "/_______\\"])
add("Robots and tech", "Terminal", [
  "+--------+",
  "| $ _    |",
  "|        |",
  "| > ls   |",
  "+--------+"])
add("Robots and tech", "Floppy Disk", [
  ".------.",
  "|[]  []|",
  "| ____ |",
  "||    ||",
  "`------'"])
add("Robots and tech", "Server", [
  ".--------.",
  "|==  (o) |",
  "'--------'",
  ".--------.",
  "|==  (o) |",
  "'--------'"])
add("Robots and tech", "Battery", [
  ".-----.__",
  "|#### |  |",
  "|#### |  |",
  "`-----'--'"])
add("Robots and tech", "Bug", [
  "\\  ..  /",
  " \\(oo)/",
  "--(  )--",
  " /(__)\\",
  "/  ''  \\"])
add("Robots and tech", "Binary", [
  "0110 1000",
  "1001 0111",
  "0101 1010",
  "1110 0001"])
add("Robots and tech", "Computer Mouse", [
  " .-.-.",
  "/ | | \\",
  "|     |",
  "|     |",
  " \\___/"])
add("Robots and tech", "Router", [
  " \\  |  /",
  ".--------.",
  "|o o o o |",
  "'--------'"])
add("Robots and tech", "Circuit", [
  "o--+--o",
  "   |",
  "o--+--[]",
  "   |",
  "o--+--o"])
add("Robots and tech", "Cursor", [
  "|\\",
  "| \\",
  "|  \\",
  "|___\\",
  "  \\ \\"])
add("Robots and tech", "Keyboard", [
  ".----------.",
  "|[][][][][]|",
  "|[][][][][]|",
  "|[________]|",
  "`----------'"])

// Nature
add("Nature", "Oak Tree", [
  "   .--.",
  " .(    ).",
  "(  ()    )",
  " `(    )'",
  "    ||",
  "   _||_"])
add("Nature", "Pine Tree", [
  "  /\\",
  " /  \\",
  "/____\\",
  " /  \\",
  "/____\\",
  "  ||"])
add("Nature", "Flower", [
  " _ _",
  "( v )",
  "(_ _)__",
  "  |",
  " \\|/",
  "  |"])
add("Nature", "Mountains", [
  "   /\\",
  "  /  \\  /\\",
  " / /\\ \\/  \\",
  "/ /  \\    \\",
  "~~~~~~~~~~~~"])
add("Nature", "Sun", [
  " \\  |  /",
  "  .-\"-.",
  "-(     )-",
  "  `-.-'",
  " /  |  \\"])
add("Nature", "Cloud", [
  "   .--.",
  " .(    ).__",
  "(          )",
  " `-.____.-'",
  "  ' ' ' '"])
add("Nature", "Wave", [
  "  _..._",
  ".'     '.__",
  "~~~~~~~~~~~~",
  "~~~~~~~~~~~~"])
add("Nature", "Cactus", [
  "   _",
  " _| |_",
  "| | | |",
  "|_| | |",
  "  | |_|",
  "  | |"])
add("Nature", "Mushroom", [
  "  .-\"\"-.",
  " /o  o  \\",
  "(   o    )",
  " `-.__.-'",
  "   |  |",
  "   '--'"])
add("Nature", "Rainbow", [
  " .-\"\"\"\"-.",
  "/ .--. \\",
  "| | || |",
  "' ' '' '"])
add("Nature", "Lightning", [
  "  /|",
  " / |",
  "/__|__",
  "  | /",
  "  |/"])
add("Nature", "Snowflake", [
  " \\  |  /",
  "  \\ | /",
  "---\\|/---",
  "---/|\\---",
  "  / | \\",
  " /  |  \\"])
add("Nature", "Palm Island", [
  "  \\\\|//",
  "   \\|/",
  "    |",
  "~.~.|.~.~"])
add("Nature", "Rain Cloud", [
  "   .--.",
  " .(    ).",
  "(________)",
  " / / / /"])

// Objects
add("Objects", "Coffee Cup", [
  "  ) )",
  " ( (",
  ".-----.",
  "|     |]",
  "\\_____/"])
add("Objects", "Lightbulb", [
  " .-\"-.",
  "/     \\",
  "\\  ^  /",
  " `-.-'",
  "  |=|",
  "  `-'"])
add("Objects", "Book", [
  ".------.",
  "|  __  |",
  "| |__| |",
  "|      |",
  "`------'"])
add("Objects", "Clock", [
  "  .---.",
  " / |  \\",
  "|  o-- |",
  " \\     /",
  "  `---'"])
add("Objects", "Umbrella", [
  " _.-\"-._",
  "/_/_|_\\_\\",
  "    |",
  "    |",
  "   _|"])
add("Objects", "Anchor", [
  "    o",
  "  --+--",
  "    |",
  "\\   |   /",
  " `--+--'"])
add("Objects", "Compass", [
  "  .---.",
  " / N   \\",
  "| W + E |",
  " \\  S  /",
  "  `---'"])
add("Objects", "Hourglass", [
  ".-----.",
  " \\   /",
  "  \\ /",
  "  / \\",
  " /...\\",
  "'-----'"])
add("Objects", "Gift", [
  "  _/\\_",
  " (_  _)",
  ".--||--.",
  "|  ||  |",
  "`--||--'"])
add("Objects", "Magnifier", [
  " .--.",
  "/    \\",
  "\\    /",
  " `--'\\",
  "       \\"])
add("Objects", "Bell", [
  "    _",
  "  .' `.",
  " /     \\",
  "/_______\\",
  "   (_)"])
add("Objects", "House", [
  "  /\\",
  " /  \\  []",
  "/____\\ ||",
  "|[]  | ||",
  "|  []|"])
add("Objects", "Camera", [
  "  .--.",
  ".-'  '-.",
  "| (  ) |",
  "| `--' |",
  "`------'"])
add("Objects", "Headphones", [
  " .-\"\"\"\"-.",
  "/        \\",
  "| |    | |",
  "[_]    [_]"])
add("Objects", "Key", [
  " .--.",
  "|  |________",
  "|  |==|==|=",
  " `--'"])

// Symbols and abstract
add("Symbols and abstract", "Heart", [
  " .-. .-.",
  "(   V   )",
  " \\     /",
  "  `. .'",
  "    V"])
add("Symbols and abstract", "Star", [
  "    *",
  "   /_\\",
  "*--- ---*",
  "  \\   /",
  "  /_ _\\"])
add("Symbols and abstract", "Diamond", [
  "  /\\",
  " /  \\",
  "<    >",
  " \\  /",
  "  \\/"])
add("Symbols and abstract", "Check", [
  "        /",
  "       /",
  "\\     /",
  " \\   /",
  "  \\_/"])
add("Symbols and abstract", "Infinity", [
  " .-.  .-.",
  "(   \\/   )",
  " `-'  `-'"])
add("Symbols and abstract", "Crown", [
  "\\  /\\  /",
  " \\/  \\/",
  " |_  _|",
  " |_||_|"])
add("Symbols and abstract", "Arrow", [
  "  /\\",
  " /  \\",
  "/_  _\\",
  "  ||",
  "  ||"])
add("Symbols and abstract", "Plus", [
  "   ||",
  "   ||",
  "==+  +==",
  "==+  +==",
  "   ||"])
add("Symbols and abstract", "Crosshair", [
  "    |",
  "  .-+-.",
  "--+ o +--",
  "  `-+-'",
  "    |"])
add("Symbols and abstract", "Eye", [
  " .-----.",
  "/ .-\"-. \\",
  "\\ `-'-' /",
  " `-----'"])
add("Symbols and abstract", "Hash", [
  " ||  ||",
  "=##==##=",
  " ||  ||",
  "=##==##=",
  " ||  ||"])
add("Symbols and abstract", "Ring", [
  " .-\"\"-.",
  "/  __  \\",
  "| (  ) |",
  "\\  `'  /",
  " `-..-'"])
add("Symbols and abstract", "Sparkle", [
  "   .",
  " . | .",
  "-- * --",
  " ' | '",
  "   '"])
add("Symbols and abstract", "Cross", [
  "\\      /",
  " \\    /",
  "  \\  /",
  "  /  \\",
  " /    \\",
  "/      \\"])

// Food
add("Food", "Burger", [
  "  ,----.",
  " /______\\",
  "~~~~~~~~~~",
  "==========",
  " \\______/"])
add("Food", "Donut", [
  "  .-\"\"-.",
  " / .--. \\",
  "| (    ) |",
  " \\ `--' /",
  "  `-..-'"])
add("Food", "Ice Cream", [
  " .--.",
  "(    )",
  "/\\/\\/\\",
  "\\    /",
  " \\  /",
  "  \\/"])
add("Food", "Apple", [
  "    /",
  " .-'-.",
  "(     )",
  "(     )",
  " `-.-'"])
add("Food", "Banana", [
  "       _",
  "     ,'/",
  "   ,' /",
  " ,'  ,'",
  "`---'"])
add("Food", "Carrot", [
  " \\|/",
  "_\\|/_",
  "\\   /",
  " \\ /",
  "  v"])
add("Food", "Cupcake", [
  "  .--.",
  " (@@@@)",
  ".'----'.",
  "\\ |||| /",
  " \\____/"])
add("Food", "Egg", [
  "  .-.",
  " /   \\",
  "| (o) |",
  " \\   /",
  "  `-'"])
add("Food", "Sushi", [
  ".------.",
  "|  ~~  |",
  "`------'",
  ".------.",
  "| (  ) |",
  "`------'"])
add("Food", "Pineapple", [
  "  \\|/",
  "  \\|/",
  " ,\"\"\"\".",
  "|/\\/\\/\\|",
  "|\\/\\/\\/|",
  " `----'"])
add("Food", "Watermelon", [
  "________",
  "\\ o  o /",
  " \\ o  /",
  "  \\  /",
  "   \\/"])
add("Food", "Cookie", [
  "  .-\"\"-.",
  " / o  o \\",
  "| o   o  |",
  " \\  o   /",
  "  `-..-'"])
add("Food", "Pizza", [
  ".--------.",
  "|o  o  o |",
  " \\  o   /",
  "  \\ o  /",
  "   \\  /",
  "    \\/"])
add("Food", "Strawberry", [
  "  \\|/",
  ",-\"\"\"-.",
  "|.' '.|",
  "| . . |",
  " \\ ' /",
  "  `-'"])

// Space
add("Space", "Rocket", [
  "   /\\",
  "  /  \\",
  " | () |",
  " |    |",
  "/|_/\\_|\\",
  "   ^^"])
add("Space", "Saturn", [
  "     _.-",
  " .--'  .'",
  "(  (O)  )",
  " '.  `--'",
  "  -._"])
add("Space", "Moon", [
  "   _..",
  " .'  _)",
  "(   (",
  " `.  `-.",
  "   `--'"])
add("Space", "UFO", [
  "   .--.",
  " .'_()_`.",
  "(_______)",
  "'-/-/-/-'",
  "  ' ' '"])
add("Space", "Alien", [
  "  .---.",
  " / ___ \\",
  "| (o o) |",
  " \\  v  /",
  "  `-^-'"])
add("Space", "Astronaut", [
  "  .---.",
  " / ,-. \\",
  "| ( o ) |",
  " \\ `-' /",
  " [=====]",
  "  |   |"])
add("Space", "Galaxy", [
  ". *   .  *",
  "  .-@@-.",
  "*(  @@  ).",
  "  `-@@-'  *"])
add("Space", "Planet", [
  "  .-\"\"-.",
  " / ~~~  \\",
  "(  ===   )",
  " \\  ~~  /",
  "  `-..-'"])
add("Space", "Constellation", [
  "*      *",
  "  \\   /",
  "   *-*",
  "  /    \\",
  "*      *"])
add("Space", "Eclipse", [
  " . .-\"\"-. .",
  ".(  ####  ).",
  " ( ###### )",
  "  `-####-'"])
add("Space", "Shooting Star", [
  "~~~~   \\|/",
  "~~~~~--=*=",
  "~~~~   /|\\"])
add("Space", "Starfield", [
  ".  *   .  *",
  "  .  *",
  "*   .    .",
  " .    *  ."])
add("Space", "Observatory", [
  "  .--.",
  " / /\\ \\",
  "/_/__\\_\\",
  "| [  ] |",
  "|_|__|_|"])

// Geometric patterns
add("Geometric patterns", "Checkerboard", [
  "#_#_#_#_#_#_",
  "_#_#_#_#_#_#",
  "#_#_#_#_#_#_",
  "_#_#_#_#_#_#"])
add("Geometric patterns", "Stripes", [
  "\\\\\\\\\\\\\\\\\\\\\\\\",
  "\\\\\\\\\\\\\\\\\\\\\\\\",
  "\\\\\\\\\\\\\\\\\\\\\\\\",
  "\\\\\\\\\\\\\\\\\\\\\\\\"])
add("Geometric patterns", "Zigzag", [
  "/\\/\\/\\/\\/\\/\\",
  "\\/\\/\\/\\/\\/\\/",
  "/\\/\\/\\/\\/\\/\\",
  "\\/\\/\\/\\/\\/\\/"])
add("Geometric patterns", "Diamonds", [
  " /\\  /\\  /\\",
  "<  ><  ><  >",
  " \\/  \\/  \\/",
  " /\\  /\\  /\\",
  "<  ><  ><  >",
  " \\/  \\/  \\/"])
add("Geometric patterns", "Squares", [
  "+----------+",
  "| +------+ |",
  "| | ++++ | |",
  "| +------+ |",
  "+----------+"])
add("Geometric patterns", "Dots", [
  "o . o . o .",
  ". o . o . o",
  "o . o . o .",
  ". o . o . o"])
add("Geometric patterns", "Triangles", [
  "   /\\",
  "  /__\\",
  " /\\  /\\",
  "/__\\/__\\"])
add("Geometric patterns", "Hexagons", [
  " _   _   _",
  "/ \\_/ \\_/ \\",
  "\\_/ \\_/ \\_/",
  "/ \\_/ \\_/ \\",
  "\\_/ \\_/ \\_/"])
add("Geometric patterns", "Crosshatch", [
  "XXXXXXXXXXXX",
  "xxxxxxxxxxxx",
  "XXXXXXXXXXXX",
  "xxxxxxxxxxxx"])
add("Geometric patterns", "Bricks", [
  "|__|__|__|__",
  "__|__|__|__|",
  "|__|__|__|__",
  "__|__|__|__|"])
add("Geometric patterns", "Chevrons", [
  ">>>>>>>>>>>>",
  ">>>>>>>>>>>>",
  "<<<<<<<<<<<<",
  "<<<<<<<<<<<<"])
add("Geometric patterns", "Concentric", [
  " .--------.",
  "/ .------. \\",
  "| | (  ) | |",
  "\\ `------' /",
  " `--------'"])
add("Geometric patterns", "Rings", [
  "oOo oOo oOo",
  "OoO OoO OoO",
  "oOo oOo oOo",
  "OoO OoO OoO"])

// Retro and game
add("Retro and game", "Invader", [
  "  _    _",
  "   \\__/",
  " .-'oo'-.",
  "/_/|__|\\_\\",
  "  /_  _\\"])
add("Retro and game", "Ghost Chaser", [
  " .--.",
  "/ oo \\",
  "|    |",
  "|/\\/\\|"])
add("Retro and game", "Chomper", [
  "  .---.",
  " / o   `.",
  "|    __.'",
  " \\   `-.",
  "  `---'"])
add("Retro and game", "Shield", [
  ".-------.",
  "| \\   / |",
  "|  \\ /  |",
  " \\  |  /",
  "  `---'"])
add("Retro and game", "Pixel Heart", [
  " ## ##",
  "#######",
  "#######",
  " #####",
  "  ###",
  "   #"])
add("Retro and game", "Power Mushroom", [
  "  .-\"\"-.",
  " / O  O \\",
  "(__ __ __)",
  "  |    |",
  "  '----'"])
add("Retro and game", "Coin", [
  "  .---.",
  " / .-. \\",
  "| | $ | |",
  " \\ `-' /",
  "  `---'"])
add("Retro and game", "Treasure Chest", [
  " .------.",
  "/______/|",
  "|  []  ||",
  "|______|/"])
add("Retro and game", "Joystick", [
  "    O",
  "    |",
  " .-----.",
  "|  o o  |",
  " `-----'"])
add("Retro and game", "Dice", [
  ".------.",
  "| o  o |",
  "|  o   |",
  "| o  o |",
  "`------'"])
add("Retro and game", "Potion", [
  "  ||",
  "  ||",
  " /  \\",
  "/ ~~ \\",
  "\\____/"])
add("Retro and game", "Castle", [
  "|_|_|_|_|",
  "|  []   |",
  "|   _   |",
  "|__| |__|"])
add("Retro and game", "Slime", [
  "   .--.",
  "  / oo \\",
  " /      \\",
  "(__~~~~__)"])

function count() { return ENTRIES.length }

// Indexes into ENTRIES for one category, or for all of them when category is "".
function indexes(category) {
  var found = []
  for (var i = 0; i < ENTRIES.length; i++) if (category === "" || ENTRIES[i].category === category) found.push(i)
  return found
}

// The entry at position + delta in a list of indexes, wrapping around; the
// position is clamped first, so a stale position never reaches outside the list.
function wrap(list, position, delta) {
  if (list.length === 0) return -1
  var at = Math.min(list.length - 1, Math.max(0, position))
  return ((at + delta) % list.length + list.length) % list.length
}

// A position in a list other than the current one (when there is another),
// from a number in [0, 1) such as Math.random().
function pick(list, position, unit) {
  if (list.length === 0) return -1
  if (list.length === 1) return 0
  var next = Math.min(list.length - 2, Math.floor(Math.min(0.999999, Math.max(0, unit)) * (list.length - 1)))
  return next >= position ? next + 1 : next
}

// The colors offered for plain art. Default ("") is the theme's own text color.
var COLORS = [
  {name: "Red", hex: "#ef4444"}, {name: "Orange", hex: "#f97316"}, {name: "Amber", hex: "#f59e0b"},
  {name: "Yellow", hex: "#fde047"}, {name: "Lime", hex: "#a3e635"}, {name: "Green", hex: "#22c55e"},
  {name: "Teal", hex: "#14b8a6"}, {name: "Cyan", hex: "#22d3ee"}, {name: "Sky", hex: "#38bdf8"},
  {name: "Blue", hex: "#3b82f6"}, {name: "Indigo", hex: "#6366f1"}, {name: "Violet", hex: "#8b5cf6"},
  {name: "Purple", hex: "#a855f7"}, {name: "Pink", hex: "#ec4899"}]

function channel(hex, at) {
  var v = parseInt(hex.slice(at, at + 2), 16) / 255
  return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
}
function luminance(hex) { return 0.2126 * channel(hex, 1) + 0.7152 * channel(hex, 3) + 0.0722 * channel(hex, 5) }
function contrast(a, b) {
  var x = luminance(a), y = luminance(b)
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05)
}
function hex2(n) { return ("0" + Math.round(n).toString(16)).slice(-2) }
// A #rrggbb color, moved toward black or white (whichever contrasts more with
// the background) until it reaches a 3:1 contrast against it. Colors that already
// read well are returned unchanged. background is a color string: only its
// last six hex digits are used (a leading alpha is ignored). "" is returned
// when either is not a #rrggbb color.
function readable(hex, background) {
  var back = typeof background === "string" ? "#" + background.slice(-6).toLowerCase() : ""
  if (typeof hex !== "string" || !/^#[0-9a-f]{6}$/.test(hex) || !/^#[0-9a-f]{6}$/.test(back)) return ""
  if (contrast(hex, back) >= 3) return hex
  // Toward whichever of black and white contrasts more: on a mid-gray neither
  // luminance side is safe, and white can stay under 3:1.
  var target = contrast("#000000", back) >= contrast("#ffffff", back) ? 0 : 255
  for (var step = 1; step <= 20; step++) {
    var t = step / 20, out = "#"
    for (var at = 1; at <= 5; at += 2) out += hex2(parseInt(hex.slice(at, at + 2), 16) * (1 - t) + target * t)
    if (contrast(out, back) >= 3) return out
  }
  return target === 0 ? "#000000" : "#ffffff"
}
