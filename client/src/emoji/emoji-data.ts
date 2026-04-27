export type EmojiCategory =
  | "Smileys & Emotion"
  | "People & Body"
  | "Animals & Nature"
  | "Food & Drink"
  | "Travel & Places"
  | "Activities"
  | "Objects"
  | "Symbols";

export interface EmojiEntry {
  emoji: string;
  name: string;
  shortcodes: string[];
  keywords: string[];
  category: EmojiCategory;
  unicodeVersion?: string;
}

const CATEGORY_BY_CODE = {
  smileys: "Smileys & Emotion",
  people: "People & Body",
  nature: "Animals & Nature",
  food: "Food & Drink",
  travel: "Travel & Places",
  activities: "Activities",
  objects: "Objects",
  symbols: "Symbols",
} as const satisfies Record<string, EmojiCategory>;

type CategoryCode = keyof typeof CATEGORY_BY_CODE;
type EmojiRow = readonly [
  emoji: string,
  name: string,
  shortcodes: string,
  keywords: string,
  category: CategoryCode,
];

function splitTerms(value: string): string[] {
  return value.length > 0 ? value.split(" ") : [];
}

// Conservative native-emoji subset for better cross-platform coverage. This
// intentionally avoids skin tone variants, flags, recent Unicode additions,
// and most complex ZWJ sequences. Render these as text so each OS supplies its
// own native emoji font/style. Future skin-tone support can add variant
// metadata alongside these base entries.
const EMOJI_ROWS = [
  ["😀", "grinning face", ":grinning:", "face smile happy", "smileys"],
  ["😃", "grinning face with big eyes", ":smiley:", "face smile happy joy", "smileys"],
  ["😄", "grinning face with smiling eyes", ":smile:", "face smile happy laugh", "smileys"],
  ["😁", "beaming face with smiling eyes", ":grin:", "face smile happy", "smileys"],
  ["😆", "grinning squinting face", ":laughing: :satisfied:", "face laugh happy", "smileys"],
  ["😅", "grinning face with sweat", ":sweat_smile:", "face smile sweat relief", "smileys"],
  ["😂", "face with tears of joy", ":joy:", "face laugh tears funny", "smileys"],
  ["🙂", "slightly smiling face", ":slight_smile:", "face smile", "smileys"],
  ["🙃", "upside-down face", ":upside_down:", "face silly sarcasm", "smileys"],
  ["😉", "winking face", ":wink:", "face wink joke", "smileys"],
  ["😊", "smiling face with smiling eyes", ":blush:", "face smile happy warm", "smileys"],
  ["😇", "smiling face with halo", ":innocent:", "face angel halo", "smileys"],
  ["😍", "smiling face with heart-eyes", ":heart_eyes:", "face love heart crush", "smileys"],
  ["😘", "face blowing a kiss", ":kissing_heart:", "face kiss love", "smileys"],
  ["😋", "face savoring food", ":yum:", "face food delicious", "smileys"],
  ["😛", "face with tongue", ":stuck_out_tongue:", "face tongue playful", "smileys"],
  [
    "😜",
    "winking face with tongue",
    ":stuck_out_tongue_winking_eye:",
    "face tongue wink playful",
    "smileys",
  ],
  ["😎", "smiling face with sunglasses", ":sunglasses:", "face cool sunglasses", "smileys"],
  ["🤔", "thinking face", ":thinking:", "face think question", "smileys"],
  ["😐", "neutral face", ":neutral_face:", "face neutral meh", "smileys"],
  ["🙄", "face with rolling eyes", ":roll_eyes:", "face eyes annoyed", "smileys"],
  ["😮", "face with open mouth", ":open_mouth:", "face surprise wow", "smileys"],
  ["😴", "sleeping face", ":sleeping:", "face sleep tired", "smileys"],
  ["😢", "crying face", ":cry:", "face sad tear", "smileys"],
  ["😭", "loudly crying face", ":sob:", "face sad cry tears", "smileys"],
  ["😠", "angry face", ":angry:", "face mad annoyed", "smileys"],
  ["😱", "face screaming in fear", ":scream:", "face fear scared", "smileys"],
  ["😷", "face with medical mask", ":mask:", "face sick mask", "smileys"],
  ["❤️", "red heart", ":heart:", "love like favorite", "smileys"],
  ["💛", "yellow heart", ":yellow_heart:", "love heart", "smileys"],
  ["💚", "green heart", ":green_heart:", "love heart", "smileys"],
  ["💙", "blue heart", ":blue_heart:", "love heart", "smileys"],
  ["💜", "purple heart", ":purple_heart:", "love heart", "smileys"],
  ["💔", "broken heart", ":broken_heart:", "sad heart breakup", "smileys"],
  ["✨", "sparkles", ":sparkles:", "star shine magic", "smileys"],
  ["🔥", "fire", ":fire:", "hot lit flame", "travel"],
  ["💯", "hundred points", ":100:", "score perfect hundred", "symbols"],
  ["👋", "waving hand", ":wave:", "hand hello goodbye", "people"],
  ["✋", "raised hand", ":raised_hand:", "hand stop high five", "people"],
  ["👌", "OK hand", ":ok_hand:", "hand ok perfect", "people"],
  ["👍", "thumbs up", ":thumbsup: :+1:", "hand thumb approve yes", "people"],
  ["👎", "thumbs down", ":thumbsdown: :-1:", "hand thumb disapprove no", "people"],
  ["👊", "oncoming fist", ":facepunch: :punch:", "hand fist bump", "people"],
  ["✊", "raised fist", ":fist:", "hand fist solidarity", "people"],
  ["👏", "clapping hands", ":clap:", "hand applause praise", "people"],
  ["🙌", "raising hands", ":raised_hands:", "hand celebrate praise", "people"],
  ["🙏", "folded hands", ":pray:", "hand please thanks prayer", "people"],
  ["💪", "flexed biceps", ":muscle:", "strong arm fitness", "people"],
  ["🐶", "dog face", ":dog:", "animal pet puppy", "nature"],
  ["🐱", "cat face", ":cat:", "animal pet kitten", "nature"],
  ["🐭", "mouse face", ":mouse:", "animal rodent", "nature"],
  ["🐹", "hamster face", ":hamster:", "animal pet", "nature"],
  ["🐰", "rabbit face", ":rabbit:", "animal bunny", "nature"],
  ["🐻", "bear", ":bear:", "animal", "nature"],
  ["🐼", "panda", ":panda_face:", "animal bear", "nature"],
  ["🐯", "tiger face", ":tiger:", "animal cat", "nature"],
  ["🐮", "cow face", ":cow:", "animal farm", "nature"],
  ["🐷", "pig face", ":pig:", "animal farm", "nature"],
  ["🐸", "frog", ":frog:", "animal", "nature"],
  ["🐵", "monkey face", ":monkey_face:", "animal", "nature"],
  ["🐝", "honeybee", ":bee: :honeybee:", "animal insect honey", "nature"],
  ["🐢", "turtle", ":turtle:", "animal slow", "nature"],
  ["🌲", "evergreen tree", ":evergreen_tree:", "nature tree forest", "nature"],
  ["🌴", "palm tree", ":palm_tree:", "nature tree tropical", "nature"],
  ["🌵", "cactus", ":cactus:", "nature desert", "nature"],
  ["🌹", "rose", ":rose:", "nature flower love", "nature"],
  ["🌻", "sunflower", ":sunflower:", "nature flower", "nature"],
  ["🌈", "rainbow", ":rainbow:", "nature weather color", "nature"],
  ["🍎", "red apple", ":apple:", "food fruit", "food"],
  ["🍊", "tangerine", ":tangerine: :orange:", "food fruit citrus", "food"],
  ["🍋", "lemon", ":lemon:", "food fruit citrus", "food"],
  ["🍌", "banana", ":banana:", "food fruit", "food"],
  ["🍉", "watermelon", ":watermelon:", "food fruit", "food"],
  ["🍇", "grapes", ":grapes:", "food fruit", "food"],
  ["🍓", "strawberry", ":strawberry:", "food fruit", "food"],
  ["🍒", "cherries", ":cherries:", "food fruit", "food"],
  ["🍍", "pineapple", ":pineapple:", "food fruit tropical", "food"],
  ["🍅", "tomato", ":tomato:", "food vegetable", "food"],
  ["🌽", "ear of corn", ":corn:", "food vegetable", "food"],
  ["🍞", "bread", ":bread:", "food bakery", "food"],
  ["🍔", "hamburger", ":hamburger:", "food burger", "food"],
  ["🍟", "french fries", ":fries:", "food potato", "food"],
  ["🍕", "pizza", ":pizza:", "food slice", "food"],
  ["🌮", "taco", ":taco:", "food", "food"],
  ["🍿", "popcorn", ":popcorn:", "food movie", "food"],
  ["🍣", "sushi", ":sushi:", "food", "food"],
  ["🍜", "steaming bowl", ":ramen:", "food noodles soup", "food"],
  ["🍦", "soft ice cream", ":icecream:", "food dessert sweet", "food"],
  ["🍩", "doughnut", ":doughnut:", "food dessert sweet", "food"],
  ["🎂", "birthday cake", ":birthday:", "food cake party", "food"],
  ["☕", "hot beverage", ":coffee:", "drink coffee tea", "food"],
  ["🍺", "beer mug", ":beer:", "drink beer", "food"],
  ["⚽", "soccer ball", ":soccer:", "sport ball football", "activities"],
  ["🏀", "basketball", ":basketball:", "sport ball", "activities"],
  ["🏈", "american football", ":football:", "sport ball", "activities"],
  ["⚾", "baseball", ":baseball:", "sport ball", "activities"],
  ["🎾", "tennis", ":tennis:", "sport ball", "activities"],
  ["🎮", "video game", ":video_game:", "game controller play", "activities"],
  ["🎲", "game die", ":game_die:", "game dice random", "activities"],
  ["🎯", "bullseye", ":dart:", "game target goal", "activities"],
  ["🎨", "artist palette", ":art:", "art paint creative", "activities"],
  ["🎤", "microphone", ":microphone:", "music sing voice", "activities"],
  ["🎧", "headphone", ":headphones:", "music audio", "activities"],
  ["🎸", "guitar", ":guitar:", "music instrument", "activities"],
  ["☀️", "sun", ":sunny:", "weather day bright", "travel"],
  ["☁️", "cloud", ":cloud:", "weather sky", "travel"],
  ["⚡", "high voltage", ":zap:", "lightning energy weather", "travel"],
  ["❄️", "snowflake", ":snowflake:", "weather cold snow", "travel"],
  ["🌊", "water wave", ":ocean:", "water sea ocean", "travel"],
  ["🚗", "automobile", ":car: :red_car:", "travel vehicle car", "travel"],
  ["🚌", "bus", ":bus:", "travel vehicle", "travel"],
  ["🚲", "bicycle", ":bike:", "travel vehicle bike", "travel"],
  ["✈️", "airplane", ":airplane:", "travel flight plane", "travel"],
  ["🚀", "rocket", ":rocket:", "travel space launch", "travel"],
  ["🏠", "house", ":house:", "home building", "travel"],
  ["🏢", "office building", ":office:", "building work", "travel"],
  ["🏫", "school", ":school:", "building education", "travel"],
  ["⌚", "watch", ":watch:", "time clock", "objects"],
  ["📱", "mobile phone", ":iphone:", "phone mobile device", "objects"],
  ["💻", "laptop", ":computer:", "computer work device", "objects"],
  ["📷", "camera", ":camera:", "photo picture", "objects"],
  ["📺", "television", ":tv:", "screen video", "objects"],
  ["⏰", "alarm clock", ":alarm_clock:", "time clock alarm", "objects"],
  ["💡", "light bulb", ":bulb:", "idea light", "objects"],
  ["🔑", "key", ":key:", "lock password", "objects"],
  ["🔒", "locked", ":lock:", "security private", "objects"],
  ["🔨", "hammer", ":hammer:", "tool build", "objects"],
  ["📎", "paperclip", ":paperclip:", "attachment office", "objects"],
  ["✂️", "scissors", ":scissors:", "cut tool", "objects"],
  ["📝", "memo", ":memo: :pencil:", "note write", "objects"],
  ["📚", "books", ":books:", "read study", "objects"],
  ["📌", "pushpin", ":pushpin:", "pin office", "objects"],
  ["✅", "check mark button", ":white_check_mark:", "yes done complete", "symbols"],
  ["❌", "cross mark", ":x:", "no cancel close", "symbols"],
  ["❓", "red question mark", ":question:", "question help", "symbols"],
  ["❗", "red exclamation mark", ":exclamation:", "alert important", "symbols"],
] as const satisfies readonly EmojiRow[];

export const CONSERVATIVE_EMOJIS: readonly EmojiEntry[] = EMOJI_ROWS.map(
  ([emoji, name, shortcodes, keywords, category]) => ({
    emoji,
    name,
    shortcodes: splitTerms(shortcodes),
    keywords: splitTerms(keywords),
    category: CATEGORY_BY_CODE[category],
  }),
);
