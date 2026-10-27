//! Small searchable emoji set (paste on run).

use crate::commands::{Command, CommandKind};

/// `(id_suffix, glyph, title, keywords)`.
const EMOJI: &[(&str, &str, &str, &[&str])] = &[
    ("grinning", "😀", "Grinning Face", &["smile", "happy"]),
    ("smiley", "😃", "Smiley", &["smile", "happy"]),
    ("smile", "😄", "Smiling Face", &["happy"]),
    ("grin", "😁", "Beaming Face", &["smile"]),
    ("laughing", "😆", "Laughing", &["lol", "haha"]),
    ("sweat_smile", "😅", "Grinning Sweat", &["relief"]),
    ("rofl", "🤣", "ROFL", &["lol"]),
    ("joy", "😂", "Joy Tears", &["lol", "cry"]),
    ("wink", "😉", "Wink", &[]),
    ("blush", "😊", "Blush", &["smile"]),
    ("innocent", "😇", "Innocent", &["angel"]),
    ("heart_eyes", "😍", "Heart Eyes", &["love"]),
    ("star_struck", "🤩", "Star Struck", &["wow"]),
    ("kissing_heart", "😘", "Blow Kiss", &["love"]),
    ("thinking", "🤔", "Thinking", &["hmm"]),
    ("neutral", "😐", "Neutral", &[]),
    ("expressionless", "😑", "Expressionless", &[]),
    ("smirk", "😏", "Smirk", &[]),
    ("unamused", "😒", "Unamused", &[]),
    ("roll_eyes", "🙄", "Eye Roll", &[]),
    ("grimacing", "😬", "Grimacing", &[]),
    ("relieved", "😌", "Relieved", &[]),
    ("pensive", "😔", "Pensive", &["sad"]),
    ("sleepy", "😪", "Sleepy", &[]),
    ("sleeping", "😴", "Sleeping", &["zzz"]),
    ("mask", "😷", "Mask", &["sick"]),
    ("sick", "🤒", "Thermometer Face", &["ill"]),
    ("hot", "🥵", "Hot Face", &[]),
    ("cold", "🥶", "Cold Face", &[]),
    ("woozy", "🥴", "Woozy", &[]),
    ("dizzy", "😵", "Dizzy", &[]),
    ("exploding", "🤯", "Exploding Head", &["mindblown"]),
    ("cowboy", "🤠", "Cowboy", &[]),
    ("partying", "🥳", "Partying", &["party", "celebrate"]),
    ("sunglasses", "😎", "Sunglasses", &["cool"]),
    ("nerd", "🤓", "Nerd", &[]),
    ("monocle", "🧐", "Monocle", &[]),
    ("confused", "😕", "Confused", &[]),
    ("worried", "😟", "Worried", &[]),
    ("slight_frown", "🙁", "Slight Frown", &["sad"]),
    ("open_mouth", "😮", "Open Mouth", &["wow"]),
    ("hushed", "😯", "Hushed", &[]),
    ("astonished", "😲", "Astonished", &["wow"]),
    ("flushed", "😳", "Flushed", &[]),
    ("pleading", "🥺", "Pleading", &["please"]),
    ("frowning", "😦", "Frowning", &["sad"]),
    ("anguished", "😧", "Anguished", &[]),
    ("fearful", "😨", "Fearful", &["scared"]),
    ("cold_sweat", "😰", "Anxious Sweat", &[]),
    ("cry", "😢", "Cry", &["sad", "tear"]),
    ("sob", "😭", "Sob", &["cry", "sad"]),
    ("scream", "😱", "Scream", &["fear"]),
    ("confounded", "😖", "Confounded", &[]),
    ("persevere", "😣", "Persevere", &[]),
    ("disappointed", "😞", "Disappointed", &["sad"]),
    ("sweat", "😓", "Downcast Sweat", &[]),
    ("weary", "😩", "Weary", &[]),
    ("tired", "😫", "Tired", &[]),
    ("yawning", "🥱", "Yawning", &[]),
    ("triumph", "😤", "Triumph", &["huff"]),
    ("rage", "😡", "Pouting", &["angry", "mad"]),
    ("angry", "😠", "Angry", &["mad"]),
    ("cursing", "🤬", "Cursing", &["angry"]),
    ("smiling_imp", "😈", "Smiling Imp", &["devil"]),
    ("imp", "👿", "Imp", &["devil"]),
    ("skull", "💀", "Skull", &["dead"]),
    ("poop", "💩", "Poop", &["shit"]),
    ("clown", "🤡", "Clown", &[]),
    ("ogre", "👹", "Ogre", &[]),
    ("goblin", "👺", "Goblin", &[]),
    ("ghost", "👻", "Ghost", &[]),
    ("alien", "👽", "Alien", &[]),
    ("space_invader", "👾", "Space Invader", &[]),
    ("robot", "🤖", "Robot", &[]),
    ("cat", "😺", "Grinning Cat", &[]),
    ("thumbsup", "👍", "Thumbs Up", &["ok", "yes", "+1"]),
    ("thumbsdown", "👎", "Thumbs Down", &["no", "-1"]),
    ("ok_hand", "👌", "OK Hand", &["ok"]),
    ("pinch", "🤏", "Pinching Hand", &[]),
    ("v", "✌️", "Victory", &["peace"]),
    ("cross_fingers", "🤞", "Crossed Fingers", &["luck"]),
    ("love_you", "🤟", "Love-You Gesture", &[]),
    ("rock", "🤘", "Rock On", &[]),
    ("call_me", "🤙", "Call Me", &[]),
    ("point_left", "👈", "Point Left", &[]),
    ("point_right", "👉", "Point Right", &[]),
    ("point_up", "👆", "Point Up", &[]),
    ("point_down", "👇", "Point Down", &[]),
    ("raised_hand", "✋", "Raised Hand", &["stop"]),
    ("clap", "👏", "Clap", &["applause"]),
    ("raised_hands", "🙌", "Raised Hands", &["hooray"]),
    ("open_hands", "👐", "Open Hands", &[]),
    ("palms_up", "🤲", "Palms Up", &[]),
    ("handshake", "🤝", "Handshake", &[]),
    ("pray", "🙏", "Folded Hands", &["please", "thanks"]),
    ("writing", "✍️", "Writing Hand", &[]),
    ("nail_care", "💅", "Nail Polish", &[]),
    ("selfie", "🤳", "Selfie", &[]),
    ("muscle", "💪", "Flexed Biceps", &["strong"]),
    ("mechanical_arm", "🦾", "Mechanical Arm", &[]),
    ("leg", "🦵", "Leg", &[]),
    ("foot", "🦶", "Foot", &[]),
    ("ear", "👂", "Ear", &[]),
    ("nose", "👃", "Nose", &[]),
    ("brain", "🧠", "Brain", &[]),
    ("tooth", "🦷", "Tooth", &[]),
    ("bone", "🦴", "Bone", &[]),
    ("eyes", "👀", "Eyes", &["look"]),
    ("eye", "👁️", "Eye", &[]),
    ("tongue", "👅", "Tongue", &[]),
    ("lips", "👄", "Mouth", &[]),
    ("baby", "👶", "Baby", &[]),
    ("child", "🧒", "Child", &[]),
    ("boy", "👦", "Boy", &[]),
    ("girl", "👧", "Girl", &[]),
    ("adult", "🧑", "Person", &[]),
    ("man", "👨", "Man", &[]),
    ("woman", "👩", "Woman", &[]),
    ("older", "🧓", "Older Person", &[]),
    ("fire", "🔥", "Fire", &["lit", "hot"]),
    ("sparkles", "✨", "Sparkles", &["magic"]),
    ("star", "⭐", "Star", &[]),
    ("glowing_star", "🌟", "Glowing Star", &[]),
    ("boom", "💥", "Collision", &["explode"]),
    ("dizzy_symbol", "💫", "Dizzy", &[]),
    ("sweat_drops", "💦", "Sweat Droplets", &[]),
    ("dash", "💨", "Dash", &[]),
    ("hole", "🕳️", "Hole", &[]),
    ("speech", "💬", "Speech Balloon", &[]),
    ("thought", "💭", "Thought Balloon", &[]),
    ("zzz", "💤", "Zzz", &["sleep"]),
    ("wave", "👋", "Wave", &["hi", "hello", "bye"]),
    ("heart", "❤️", "Red Heart", &["love"]),
    ("orange_heart", "🧡", "Orange Heart", &["love"]),
    ("yellow_heart", "💛", "Yellow Heart", &["love"]),
    ("green_heart", "💚", "Green Heart", &["love"]),
    ("blue_heart", "💙", "Blue Heart", &["love"]),
    ("purple_heart", "💜", "Purple Heart", &["love"]),
    ("black_heart", "🖤", "Black Heart", &["love"]),
    ("white_heart", "🤍", "White Heart", &["love"]),
    ("brown_heart", "🤎", "Brown Heart", &["love"]),
    ("broken_heart", "💔", "Broken Heart", &[]),
    ("two_hearts", "💕", "Two Hearts", &["love"]),
    ("revolving_hearts", "💞", "Revolving Hearts", &[]),
    ("heartbeat", "💓", "Beating Heart", &[]),
    ("heartpulse", "💗", "Growing Heart", &[]),
    ("sparkling_heart", "💖", "Sparkling Heart", &[]),
    ("cupid", "💘", "Heart Arrow", &[]),
    ("gift_heart", "💝", "Heart Ribbon", &[]),
    ("heart_decoration", "💟", "Heart Decoration", &[]),
    ("peace", "☮️", "Peace", &[]),
    ("christian_cross", "✝️", "Latin Cross", &[]),
    ("star_david", "✡️", "Star of David", &[]),
    ("om", "🕉️", "Om", &[]),
    ("wheel_dharma", "☸️", "Wheel of Dharma", &[]),
    ("yin_yang", "☯️", "Yin Yang", &[]),
    ("check", "✅", "Check Mark", &["done", "yes"]),
    ("x", "❌", "Cross Mark", &["no", "wrong"]),
    ("warning", "⚠️", "Warning", &[]),
    ("no_entry", "⛔", "No Entry", &[]),
    ("100", "💯", "Hundred", &["perfect"]),
    ("anger", "💢", "Anger Symbol", &[]),
    ("hotsprings", "♨️", "Hot Springs", &[]),
    ("bangbang", "‼️", "Double Exclamation", &[]),
    ("interrobang", "⁉️", "Exclamation Question", &[]),
    ("question", "❓", "Question", &[]),
    ("exclamation", "❗", "Exclamation", &[]),
    ("music", "🎵", "Musical Note", &[]),
    ("notes", "🎶", "Notes", &[]),
    ("atm", "🏧", "ATM", &[]),
    ("wheelchair", "♿", "Wheelchair", &[]),
    ("parking", "🅿️", "Parking", &[]),
    ("wc", "🚾", "Water Closet", &[]),
    ("men", "🚹", "Mens", &[]),
    ("women", "🚺", "Womens", &[]),
    ("restroom", "🚻", "Restroom", &[]),
    ("baby_symbol", "🚼", "Baby Symbol", &[]),
    ("toilet", "🚽", "Toilet", &[]),
    ("shower", "🚿", "Shower", &[]),
    ("bathtub", "🛁", "Bathtub", &[]),
    ("coffee", "☕", "Hot Beverage", &["tea"]),
    ("beer", "🍺", "Beer", &[]),
    ("beers", "🍻", "Clinking Beers", &[]),
    ("wine", "🍷", "Wine Glass", &[]),
    ("cocktail", "🍸", "Cocktail", &[]),
    ("pizza", "🍕", "Pizza", &[]),
    ("burger", "🍔", "Hamburger", &[]),
    ("fries", "🍟", "Fries", &[]),
    ("taco", "🌮", "Taco", &[]),
    ("sushi", "🍣", "Sushi", &[]),
    ("ramen", "🍜", "Ramen", &[]),
    ("cake", "🎂", "Birthday Cake", &["birthday"]),
    ("cookie", "🍪", "Cookie", &[]),
    ("apple", "🍎", "Red Apple", &[]),
    ("banana", "🍌", "Banana", &[]),
    ("grapes", "🍇", "Grapes", &[]),
    ("strawberry", "🍓", "Strawberry", &[]),
    ("avocado", "🥑", "Avocado", &[]),
    ("dog", "🐶", "Dog Face", &[]),
    ("cat_face", "🐱", "Cat Face", &[]),
    ("mouse", "🐭", "Mouse Face", &[]),
    ("hamster", "🐹", "Hamster", &[]),
    ("rabbit", "🐰", "Rabbit Face", &[]),
    ("fox", "🦊", "Fox", &[]),
    ("bear", "🐻", "Bear", &[]),
    ("panda", "🐼", "Panda", &[]),
    ("koala", "🐨", "Koala", &[]),
    ("tiger", "🐯", "Tiger Face", &[]),
    ("lion", "🦁", "Lion", &[]),
    ("cow", "🐮", "Cow Face", &[]),
    ("pig", "🐷", "Pig Face", &[]),
    ("frog", "🐸", "Frog", &[]),
    ("monkey", "🐵", "Monkey Face", &[]),
    ("see_no_evil", "🙈", "See-No-Evil", &[]),
    ("hear_no_evil", "🙉", "Hear-No-Evil", &[]),
    ("speak_no_evil", "🙊", "Speak-No-Evil", &[]),
    ("monkey_face", "🐒", "Monkey", &[]),
    ("chicken", "🐔", "Chicken", &[]),
    ("penguin", "🐧", "Penguin", &[]),
    ("bird", "🐦", "Bird", &[]),
    ("baby_chick", "🐤", "Baby Chick", &[]),
    ("duck", "🦆", "Duck", &[]),
    ("eagle", "🦅", "Eagle", &[]),
    ("owl", "🦉", "Owl", &[]),
    ("bat", "🦇", "Bat", &[]),
    ("wolf", "🐺", "Wolf", &[]),
    ("boar", "🐗", "Boar", &[]),
    ("horse", "🐴", "Horse Face", &[]),
    ("unicorn", "🦄", "Unicorn", &[]),
    ("bee", "🐝", "Honeybee", &[]),
    ("bug", "🐛", "Bug", &[]),
    ("butterfly", "🦋", "Butterfly", &[]),
    ("snail", "🐌", "Snail", &[]),
    ("ladybug", "🐞", "Lady Beetle", &[]),
    ("ant", "🐜", "Ant", &[]),
    ("mosquito", "🦟", "Mosquito", &[]),
    ("cricket", "🦗", "Cricket", &[]),
    ("spider", "🕷️", "Spider", &[]),
    ("scorpion", "🦂", "Scorpion", &[]),
    ("turtle", "🐢", "Turtle", &[]),
    ("snake", "🐍", "Snake", &[]),
    ("lizard", "🦎", "Lizard", &[]),
    ("t_rex", "🦖", "T-Rex", &[]),
    ("sauropod", "🦕", "Sauropod", &[]),
    ("octopus", "🐙", "Octopus", &[]),
    ("squid", "🦑", "Squid", &[]),
    ("shrimp", "🦐", "Shrimp", &[]),
    ("lobster", "🦞", "Lobster", &[]),
    ("crab", "🦀", "Crab", &[]),
    ("blowfish", "🐡", "Blowfish", &[]),
    ("tropical_fish", "🐠", "Tropical Fish", &[]),
    ("fish", "🐟", "Fish", &[]),
    ("dolphin", "🐬", "Dolphin", &[]),
    ("whale", "🐳", "Spouting Whale", &[]),
    ("shark", "🦈", "Shark", &[]),
    ("crocodile", "🐊", "Crocodile", &[]),
    ("tiger2", "🐅", "Tiger", &[]),
    ("leopard", "🐆", "Leopard", &[]),
    ("zebra", "🦓", "Zebra", &[]),
    ("gorilla", "🦍", "Gorilla", &[]),
    ("elephant", "🐘", "Elephant", &[]),
    ("mammoth", "🦣", "Mammoth", &[]),
    ("rhino", "🦏", "Rhinoceros", &[]),
    ("hippo", "🦛", "Hippopotamus", &[]),
    ("camel", "🐪", "Camel", &[]),
    ("giraffe", "🦒", "Giraffe", &[]),
    ("kangaroo", "🦘", "Kangaroo", &[]),
    ("bison", "🦬", "Bison", &[]),
    ("water_buffalo", "🐃", "Water Buffalo", &[]),
    ("ox", "🐂", "Ox", &[]),
    ("cow2", "🐄", "Cow", &[]),
    ("racehorse", "🐎", "Horse", &[]),
    ("pig2", "🐖", "Pig", &[]),
    ("ram", "🐏", "Ram", &[]),
    ("sheep", "🐑", "Ewe", &[]),
    ("llama", "🦙", "Llama", &[]),
    ("goat", "🐐", "Goat", &[]),
    ("deer", "🦌", "Deer", &[]),
    ("dog2", "🐕", "Dog", &[]),
    ("poodle", "🐩", "Poodle", &[]),
    ("guide_dog", "🦮", "Guide Dog", &[]),
    ("service_dog", "🐕‍🦺", "Service Dog", &[]),
    ("cat2", "🐈", "Cat", &[]),
    ("black_cat", "🐈‍⬛", "Black Cat", &[]),
    ("feather", "🪶", "Feather", &[]),
    ("rocket", "🚀", "Rocket", &["ship"]),
    ("airplane", "✈️", "Airplane", &["flight"]),
    ("car", "🚗", "Car", &[]),
    ("taxi", "🚕", "Taxi", &[]),
    ("bus", "🚌", "Bus", &[]),
    ("train", "🚆", "Train", &[]),
    ("bike", "🚲", "Bicycle", &[]),
    ("scooter", "🛴", "Kick Scooter", &[]),
    ("house", "🏠", "House", &["home"]),
    ("office", "🏢", "Office", &[]),
    ("hospital", "🏥", "Hospital", &[]),
    ("school", "🏫", "School", &[]),
    ("church", "⛪", "Church", &[]),
    ("tent", "⛺", "Tent", &[]),
    ("foggy", "🌁", "Foggy", &[]),
    ("night", "🌃", "Night Stars", &[]),
    ("cityscape", "🏙️", "Cityscape", &[]),
    ("sunrise", "🌅", "Sunrise", &[]),
    ("sunset", "🌇", "Sunset", &[]),
    ("bridge", "🌉", "Bridge at Night", &[]),
    ("milky_way", "🌌", "Milky Way", &[]),
    ("carousel", "🎠", "Carousel", &[]),
    ("ferris", "🎡", "Ferris Wheel", &[]),
    ("roller", "🎢", "Roller Coaster", &[]),
    ("barber", "💈", "Barber Pole", &[]),
    ("circus", "🎪", "Circus Tent", &[]),
    ("tada", "🎉", "Party Popper", &["celebrate", "congrats"]),
    ("confetti", "🎊", "Confetti Ball", &["party"]),
    ("balloon", "🎈", "Balloon", &[]),
    ("gift", "🎁", "Wrapped Gift", &["present"]),
    ("trophy", "🏆", "Trophy", &["win"]),
    ("medal", "🏅", "Sports Medal", &[]),
    ("soccer", "⚽", "Soccer", &[]),
    ("basketball", "🏀", "Basketball", &[]),
    ("football", "🏈", "Football", &[]),
    ("baseball", "⚾", "Baseball", &[]),
    ("tennis", "🎾", "Tennis", &[]),
    ("golf", "⛳", "Golf", &[]),
    ("dart", "🎯", "Direct Hit", &["target"]),
    ("game_die", "🎲", "Die", &["dice"]),
    ("puzzle", "🧩", "Puzzle", &[]),
    ("chess", "♟️", "Chess Pawn", &[]),
    ("spades", "♠️", "Spades", &[]),
    ("hearts_suit", "♥️", "Hearts", &[]),
    ("diamonds", "♦️", "Diamonds", &[]),
    ("clubs", "♣️", "Clubs", &[]),
    ("joker", "🃏", "Joker", &[]),
    ("mahjong", "🀄", "Mahjong", &[]),
    ("flower", "🌸", "Cherry Blossom", &[]),
    ("rose", "🌹", "Rose", &[]),
    ("hibiscus", "🌺", "Hibiscus", &[]),
    ("sunflower", "🌻", "Sunflower", &[]),
    ("tulip", "🌷", "Tulip", &[]),
    ("seedling", "🌱", "Seedling", &[]),
    ("evergreen", "🌲", "Evergreen", &[]),
    ("deciduous", "🌳", "Deciduous Tree", &[]),
    ("palm", "🌴", "Palm Tree", &[]),
    ("cactus", "🌵", "Cactus", &[]),
    ("earth", "🌍", "Earth Africa", &["world", "globe"]),
    ("moon", "🌙", "Crescent Moon", &[]),
    ("sun", "☀️", "Sun", &[]),
    ("cloud", "☁️", "Cloud", &[]),
    ("rain", "🌧️", "Cloud Rain", &[]),
    ("snow", "❄️", "Snowflake", &[]),
    ("zap", "⚡", "High Voltage", &["lightning"]),
    ("umbrella", "☔", "Umbrella Rain", &[]),
    ("rainbow", "🌈", "Rainbow", &[]),
    ("ocean", "🌊", "Water Wave", &[]),
    ("money", "💰", "Money Bag", &[]),
    ("dollar", "💵", "Dollar", &[]),
    ("credit_card", "💳", "Credit Card", &[]),
    ("gem", "💎", "Gem Stone", &[]),
    ("wrench", "🔧", "Wrench", &[]),
    ("hammer", "🔨", "Hammer", &[]),
    ("gear", "⚙️", "Gear", &[]),
    ("link", "🔗", "Link", &[]),
    ("paperclip", "📎", "Paperclip", &[]),
    ("scissors", "✂️", "Scissors", &[]),
    ("lock", "🔒", "Locked", &[]),
    ("key", "🔑", "Key", &[]),
    ("bulb", "💡", "Light Bulb", &["idea"]),
    ("flashlight", "🔦", "Flashlight", &[]),
    ("book", "📖", "Open Book", &[]),
    ("books", "📚", "Books", &[]),
    ("memo", "📝", "Memo", &["note"]),
    ("pencil", "✏️", "Pencil", &[]),
    ("briefcase", "💼", "Briefcase", &[]),
    ("folder", "📁", "Folder", &[]),
    ("calendar", "📅", "Calendar", &[]),
    ("chart", "📊", "Bar Chart", &[]),
    ("clipboard", "📋", "Clipboard", &[]),
    ("pushpin", "📌", "Pushpin", &["pin"]),
    ("paperclip2", "🖇️", "Linked Paperclips", &[]),
    ("mag", "🔍", "Magnifying Glass", &["search"]),
    ("bell", "🔔", "Bell", &[]),
    ("megaphone", "📣", "Megaphone", &[]),
    ("hourglass", "⏳", "Hourglass", &[]),
    ("watch", "⌚", "Watch", &[]),
    ("phone", "📱", "Mobile Phone", &[]),
    ("computer", "💻", "Laptop", &[]),
    ("keyboard", "⌨️", "Keyboard", &[]),
    ("tv", "📺", "Television", &[]),
    ("camera", "📷", "Camera", &[]),
    ("movie", "🎬", "Clapper", &[]),
    ("headphones", "🎧", "Headphones", &[]),
    ("battery", "🔋", "Battery", &[]),
    ("electric_plug", "🔌", "Electric Plug", &[]),
];

/// `emoji` / `emo` / `emojis`. Search matches these; OpenCast Keys lists them.
pub const PREFIXES: &[(&str, &str)] = &[("emojis", "Emoji"), ("emoji", "Emoji"), ("emo", "Emoji")];

/// The needle when `query` asks for emoji (`emoji`, `emo smile`), else None.
pub fn needle(query: &str) -> Option<String> {
    let q = query.trim().to_lowercase();
    for (prefix, _) in PREFIXES {
        if q == *prefix {
            return Some(String::new());
        }
        if let Some(rest) = q.strip_prefix(&format!("{prefix} ")) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// Codepoints that take a skin-tone modifier (Unicode Emoji_Modifier_Base).
const MODIFIER_BASES: &[(u32, u32)] = &[
    (0x261D, 0x261D),
    (0x26F9, 0x26F9),
    (0x270A, 0x270D),
    (0x1F385, 0x1F385),
    (0x1F3C2, 0x1F3C4),
    (0x1F3C7, 0x1F3C7),
    (0x1F3CA, 0x1F3CC),
    (0x1F442, 0x1F443),
    (0x1F446, 0x1F450),
    (0x1F466, 0x1F478),
    (0x1F47C, 0x1F47C),
    (0x1F481, 0x1F483),
    (0x1F485, 0x1F487),
    (0x1F48F, 0x1F48F),
    (0x1F491, 0x1F491),
    (0x1F4AA, 0x1F4AA),
    (0x1F574, 0x1F575),
    (0x1F57A, 0x1F57A),
    (0x1F590, 0x1F590),
    (0x1F595, 0x1F596),
    (0x1F645, 0x1F647),
    (0x1F64B, 0x1F64F),
    (0x1F6A3, 0x1F6A3),
    (0x1F6B4, 0x1F6B6),
    (0x1F6C0, 0x1F6C0),
    (0x1F6CC, 0x1F6CC),
    (0x1F90C, 0x1F90C),
    (0x1F90F, 0x1F90F),
    (0x1F918, 0x1F91F),
    (0x1F926, 0x1F926),
    (0x1F930, 0x1F939),
    (0x1F93C, 0x1F93E),
    (0x1F977, 0x1F977),
    (0x1F9B5, 0x1F9B6),
    (0x1F9B8, 0x1F9B9),
    (0x1F9BB, 0x1F9BB),
    (0x1F9CD, 0x1F9CF),
    (0x1F9D1, 0x1F9DD),
    (0x1FAC3, 0x1FAC5),
    (0x1FAF0, 0x1FAF8),
];

/// `light` … `dark` → the Fitzpatrick modifier.
fn tone_modifier(tone: &str) -> Option<char> {
    let cp = match tone.trim().to_lowercase().replace(' ', "-").as_str() {
        "light" => 0x1F3FB,
        "medium-light" => 0x1F3FC,
        "medium" => 0x1F3FD,
        "medium-dark" => 0x1F3FE,
        "dark" => 0x1F3FF,
        _ => return None,
    };
    char::from_u32(cp)
}

/// Apply a skin tone to a single-person emoji (others are returned as is).
pub fn with_tone(glyph: &str, tone: &str) -> String {
    let Some(m) = tone_modifier(tone) else {
        return glyph.to_string();
    };
    let mut chars = glyph.chars().filter(|c| *c != '\u{FE0F}');
    let Some(first) = chars.next() else {
        return glyph.to_string();
    };
    if chars.next().is_some() {
        return glyph.to_string();
    }
    let cp = first as u32;
    if MODIFIER_BASES.iter().any(|(a, b)| (*a..=*b).contains(&cp)) {
        format!("{first}{m}")
    } else {
        glyph.to_string()
    }
}

fn row(id: String, glyph: &str, title: &str, kw: Vec<String>, tone: &str) -> Command {
    let g = with_tone(glyph, tone);
    Command {
        id,
        title: format!("{g}  {title}"),
        subtitle: Some("Emoji · Return to paste".into()),
        icon: Some(g),
        kind: CommandKind::Snippet,
        arguments: vec![],
        hotkey: None,
        keywords: kw,
    }
}

/// Rows when the user is searching emoji (`emoji`, `emo`, or `emoji smile`).
pub fn matching(query: &str, limit: usize) -> Vec<Command> {
    matching_with(query, limit, &[], "")
}

/// Like [`matching`], with recently used emoji ids first and a skin tone.
pub fn matching_with(query: &str, limit: usize, recent: &[String], tone: &str) -> Vec<Command> {
    let Some(needle) = needle(query) else {
        return Vec::new();
    };
    let mut out: Vec<Command> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let words: Vec<&str> = needle.split_whitespace().collect();
    let hit = |hay: &str| words.iter().all(|w| hay.contains(w));
    // Recents first (only when browsing or when they match).
    for id in recent {
        if let Some((g, title)) = lookup(id) {
            if hit(&title.to_lowercase()) || needle.is_empty() {
                seen.insert(g.trim_end_matches('\u{FE0F}').to_string());
                out.push(row(id.clone(), g, &title, vec!["recent".into()], tone));
            }
        }
    }
    for (id, glyph, title, kw) in EMOJI {
        if out.len() >= limit {
            return out;
        }
        let hay = format!("{id} {title} {}", kw.join(" ")).to_lowercase();
        if !needle.is_empty() && !hit(&hay) {
            continue;
        }
        if !seen.insert(glyph.trim_end_matches('\u{FE0F}').to_string()) {
            continue;
        }
        out.push(row(
            format!("emoji:{id}"),
            glyph,
            title,
            kw.iter().map(|s| (*s).into()).collect(),
            tone,
        ));
    }
    if needle.is_empty() {
        return out;
    }
    for (glyph, name) in crate::emoji_data::ALL {
        if out.len() >= limit {
            break;
        }
        if !hit(&name.to_lowercase()) || !seen.insert(glyph.trim_end_matches('\u{FE0F}').to_string()) {
            continue;
        }
        let cp = glyph.chars().next().map(|c| c as u32).unwrap_or(0);
        out.push(row(format!("emoji:u{cp:x}"), glyph, name, vec![], tone));
    }
    out
}

/// (glyph, title) for an emoji id from either table.
fn lookup(id: &str) -> Option<(&'static str, String)> {
    let key = id.strip_prefix("emoji:")?;
    if let Some((_, g, t, _)) = EMOJI.iter().find(|(k, ..)| *k == key) {
        return Some((g, t.to_string()));
    }
    let cp = u32::from_str_radix(key.strip_prefix('u')?, 16).ok()?;
    crate::emoji_data::ALL
        .iter()
        .find(|(g, _)| g.chars().next().map(|c| c as u32) == Some(cp))
        .map(|(g, n)| (*g, n.to_string()))
}

pub fn glyph(id: &str) -> Option<&'static str> {
    lookup(id).map(|(g, _)| g)
}

/// The glyph with the configured skin tone (what Return pastes).
pub fn glyph_with_tone(id: &str, tone: &str) -> Option<String> {
    glyph(id).map(|g| with_tone(g, tone))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_grin() {
        let rows = matching("emoji grin", 10);
        assert!(rows.iter().any(|c| c.id.contains("grin")));
    }

    #[test]
    fn full_table_recents_and_tones() {
        // Not in the curated table, found by Unicode name.
        let rows = matching("emoji pretzel", 10);
        assert!(rows.iter().any(|c| c.title.contains("🥨")), "{rows:?}");
        assert_eq!(glyph(&rows[0].id), Some("🥨"));
        // Recents first when browsing.
        let rows = matching_with("emoji", 30, &["emoji:u1f968".into()], "");
        assert!(rows[0].title.contains("🥨"));
        // Skin tones only on modifier bases.
        assert_eq!(with_tone("👍", "medium"), "👍🏽");
        assert_eq!(with_tone("✌️", "dark"), "✌🏿");
        assert_eq!(with_tone("🥨", "dark"), "🥨");
        assert_eq!(with_tone("👍", ""), "👍");
        assert!(matching("hello", 5).is_empty());
    }
}
