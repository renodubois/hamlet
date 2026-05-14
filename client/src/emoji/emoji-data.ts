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
  shortcodes: string[];
  category: EmojiCategory;
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
type EmojiRow = readonly [emoji: string, shortcodes: string, category: CategoryCode];

function splitShortcodes(value: string): string[] {
  return value.length > 0 ? value.split(" ") : [];
}

// Conservative native-emoji subset for better cross-platform coverage. Entries
// intentionally store only the glyph, category, and canonical shortcodes. This
// avoids skin tone variants, flags, recent Unicode additions, and most complex
// ZWJ sequences. Render these as text so each OS supplies its own native emoji
// font/style. Future skin-tone support can add variant metadata alongside these
// base entries.
const EMOJI_ROWS = [
  ["😀", ":grinning:", "smileys"],
  ["😃", ":smiley:", "smileys"],
  ["😄", ":smile:", "smileys"],
  ["😁", ":grin:", "smileys"],
  ["😆", ":laughing: :satisfied:", "smileys"],
  ["😅", ":sweat_smile:", "smileys"],
  ["😂", ":joy:", "smileys"],
  ["🙂", ":slight_smile:", "smileys"],
  ["🙃", ":upside_down:", "smileys"],
  ["😉", ":wink:", "smileys"],
  ["😊", ":blush:", "smileys"],
  ["😇", ":innocent:", "smileys"],
  ["❤️", ":heart:", "smileys"],
  ["💛", ":yellow_heart:", "smileys"],
  ["💚", ":green_heart:", "smileys"],
  ["💙", ":blue_heart:", "smileys"],
  ["💜", ":purple_heart:", "smileys"],
  ["💔", ":broken_heart:", "smileys"],
  ["😍", ":heart_eyes:", "smileys"],
  ["😘", ":kissing_heart:", "smileys"],
  ["😋", ":yum:", "smileys"],
  ["😛", ":stuck_out_tongue:", "smileys"],
  ["😜", ":stuck_out_tongue_winking_eye:", "smileys"],
  ["😎", ":sunglasses:", "smileys"],
  ["🤔", ":thinking:", "smileys"],
  ["😐", ":neutral_face:", "smileys"],
  ["🙄", ":roll_eyes:", "smileys"],
  ["😮", ":open_mouth:", "smileys"],
  ["😴", ":sleeping:", "smileys"],
  ["😢", ":cry:", "smileys"],
  ["😭", ":sob:", "smileys"],
  ["😠", ":angry:", "smileys"],
  ["😱", ":scream:", "smileys"],
  ["😷", ":mask:", "smileys"],
  ["✨", ":sparkles:", "smileys"],
  ["🔥", ":fire:", "travel"],
  ["💯", ":100:", "symbols"],
  ["👋", ":wave:", "people"],
  ["✋", ":raised_hand:", "people"],
  ["👌", ":ok_hand:", "people"],
  ["👍", ":thumbsup: :+1:", "people"],
  ["👎", ":thumbsdown: :-1:", "people"],
  ["👊", ":facepunch: :punch:", "people"],
  ["✊", ":fist:", "people"],
  ["👏", ":clap:", "people"],
  ["🙌", ":raised_hands:", "people"],
  ["🙏", ":pray:", "people"],
  ["💪", ":muscle:", "people"],
  ["🐶", ":dog:", "nature"],
  ["🐱", ":cat:", "nature"],
  ["🐭", ":mouse:", "nature"],
  ["🐹", ":hamster:", "nature"],
  ["🐰", ":rabbit:", "nature"],
  ["🐻", ":bear:", "nature"],
  ["🐼", ":panda_face:", "nature"],
  ["🐯", ":tiger:", "nature"],
  ["🐮", ":cow:", "nature"],
  ["🐷", ":pig:", "nature"],
  ["🐸", ":frog:", "nature"],
  ["🐵", ":monkey_face:", "nature"],
  ["🐝", ":bee: :honeybee:", "nature"],
  ["🐢", ":turtle:", "nature"],
  ["🌲", ":evergreen_tree:", "nature"],
  ["🌴", ":palm_tree:", "nature"],
  ["🌵", ":cactus:", "nature"],
  ["🌹", ":rose:", "nature"],
  ["🌻", ":sunflower:", "nature"],
  ["🌈", ":rainbow:", "nature"],
  ["🍎", ":apple:", "food"],
  ["🍊", ":tangerine: :orange:", "food"],
  ["🍋", ":lemon:", "food"],
  ["🍌", ":banana:", "food"],
  ["🍉", ":watermelon:", "food"],
  ["🍇", ":grapes:", "food"],
  ["🍓", ":strawberry:", "food"],
  ["🍒", ":cherries:", "food"],
  ["🍍", ":pineapple:", "food"],
  ["🍅", ":tomato:", "food"],
  ["🌽", ":corn:", "food"],
  ["🍞", ":bread:", "food"],
  ["🍔", ":hamburger:", "food"],
  ["🍟", ":fries:", "food"],
  ["🍕", ":pizza:", "food"],
  ["🌮", ":taco:", "food"],
  ["🍿", ":popcorn:", "food"],
  ["🍣", ":sushi:", "food"],
  ["🍜", ":ramen:", "food"],
  ["🍦", ":icecream:", "food"],
  ["🍩", ":doughnut:", "food"],
  ["🎂", ":birthday:", "food"],
  ["☕", ":coffee:", "food"],
  ["🍺", ":beer:", "food"],
  ["⚽", ":soccer:", "activities"],
  ["🏀", ":basketball:", "activities"],
  ["🏈", ":football:", "activities"],
  ["⚾", ":baseball:", "activities"],
  ["🎾", ":tennis:", "activities"],
  ["🎮", ":video_game:", "activities"],
  ["🎲", ":game_die:", "activities"],
  ["🎯", ":dart:", "activities"],
  ["🎨", ":art:", "activities"],
  ["🎤", ":microphone:", "activities"],
  ["🎧", ":headphones:", "activities"],
  ["🎸", ":guitar:", "activities"],
  ["☀️", ":sunny:", "travel"],
  ["☁️", ":cloud:", "travel"],
  ["⚡", ":zap:", "travel"],
  ["❄️", ":snowflake:", "travel"],
  ["🌊", ":ocean:", "travel"],
  ["🚗", ":car: :red_car:", "travel"],
  ["🚌", ":bus:", "travel"],
  ["🚲", ":bike:", "travel"],
  ["✈️", ":airplane:", "travel"],
  ["🚀", ":rocket:", "travel"],
  ["🏠", ":house:", "travel"],
  ["🏢", ":office:", "travel"],
  ["🏫", ":school:", "travel"],
  ["⌚", ":watch:", "objects"],
  ["📱", ":iphone:", "objects"],
  ["💻", ":computer:", "objects"],
  ["📷", ":camera:", "objects"],
  ["📺", ":tv:", "objects"],
  ["⏰", ":alarm_clock:", "objects"],
  ["💡", ":bulb:", "objects"],
  ["🔑", ":key:", "objects"],
  ["🔒", ":lock:", "objects"],
  ["🔨", ":hammer:", "objects"],
  ["📎", ":paperclip:", "objects"],
  ["✂️", ":scissors:", "objects"],
  ["📝", ":memo: :pencil:", "objects"],
  ["📚", ":books:", "objects"],
  ["📌", ":pushpin:", "objects"],
  ["✅", ":white_check_mark:", "symbols"],
  ["❌", ":x:", "symbols"],
  ["❓", ":question:", "symbols"],
  ["❗", ":exclamation:", "symbols"],
] as const satisfies readonly EmojiRow[];

export const CONSERVATIVE_EMOJIS: readonly EmojiEntry[] = EMOJI_ROWS.map(
  ([emoji, shortcodes, category]) => ({
    emoji,
    shortcodes: splitShortcodes(shortcodes),
    category: CATEGORY_BY_CODE[category],
  }),
);
