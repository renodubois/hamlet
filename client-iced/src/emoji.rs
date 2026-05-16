#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmojiChoice {
    pub symbol: &'static str,
    pub name: &'static str,
    pub keywords: &'static [&'static str],
}

impl EmojiChoice {
    fn matches_terms(&self, terms: &[String]) -> bool {
        terms.iter().all(|term| self.matches_term(term))
    }

    fn matches_term(&self, term: &str) -> bool {
        self.symbol.contains(term)
            || self.name.contains(term)
            || self.keywords.iter().any(|keyword| keyword.contains(term))
    }
}

pub const MAX_VISIBLE_CHOICES: usize = 12;

pub const EMOJI_CATALOG: &[EmojiChoice] = &[
    EmojiChoice {
        symbol: "😀",
        name: "grinning face",
        keywords: &["smile", "happy", "joy"],
    },
    EmojiChoice {
        symbol: "😄",
        name: "smiling face with open mouth",
        keywords: &["smile", "happy", "laugh"],
    },
    EmojiChoice {
        symbol: "😂",
        name: "face with tears of joy",
        keywords: &["laugh", "lol", "cry"],
    },
    EmojiChoice {
        symbol: "🙂",
        name: "slightly smiling face",
        keywords: &["smile", "friendly"],
    },
    EmojiChoice {
        symbol: "😉",
        name: "winking face",
        keywords: &["wink", "playful"],
    },
    EmojiChoice {
        symbol: "😍",
        name: "smiling face with heart eyes",
        keywords: &["love", "heart", "crush"],
    },
    EmojiChoice {
        symbol: "🥳",
        name: "partying face",
        keywords: &["party", "celebrate", "birthday"],
    },
    EmojiChoice {
        symbol: "🤔",
        name: "thinking face",
        keywords: &["think", "hmm", "question"],
    },
    EmojiChoice {
        symbol: "😭",
        name: "loudly crying face",
        keywords: &["cry", "sad", "tears"],
    },
    EmojiChoice {
        symbol: "😎",
        name: "smiling face with sunglasses",
        keywords: &["cool", "sun", "shades"],
    },
    EmojiChoice {
        symbol: "👍",
        name: "thumbs up",
        keywords: &["yes", "approve", "like"],
    },
    EmojiChoice {
        symbol: "👎",
        name: "thumbs down",
        keywords: &["no", "disapprove", "dislike"],
    },
    EmojiChoice {
        symbol: "👏",
        name: "clapping hands",
        keywords: &["clap", "applause", "great"],
    },
    EmojiChoice {
        symbol: "🙏",
        name: "folded hands",
        keywords: &["please", "thanks", "pray"],
    },
    EmojiChoice {
        symbol: "👋",
        name: "waving hand",
        keywords: &["wave", "hello", "goodbye"],
    },
    EmojiChoice {
        symbol: "❤️",
        name: "red heart",
        keywords: &["love", "like", "heart"],
    },
    EmojiChoice {
        symbol: "🔥",
        name: "fire",
        keywords: &["hot", "lit", "flame"],
    },
    EmojiChoice {
        symbol: "✨",
        name: "sparkles",
        keywords: &["sparkle", "magic", "shiny"],
    },
    EmojiChoice {
        symbol: "🎉",
        name: "party popper",
        keywords: &["party", "celebrate", "tada"],
    },
    EmojiChoice {
        symbol: "✅",
        name: "check mark button",
        keywords: &["done", "complete", "yes"],
    },
    EmojiChoice {
        symbol: "❌",
        name: "cross mark",
        keywords: &["x", "no", "cancel"],
    },
    EmojiChoice {
        symbol: "🚀",
        name: "rocket",
        keywords: &["ship", "launch", "fast"],
    },
    EmojiChoice {
        symbol: "👀",
        name: "eyes",
        keywords: &["look", "watch", "see"],
    },
    EmojiChoice {
        symbol: "💯",
        name: "hundred points",
        keywords: &["100", "perfect", "score"],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmojiPickerNavigation {
    Previous,
    Next,
    First,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmojiPickerFocusTarget {
    Composer,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmojiPickerState {
    pub is_open: bool,
    pub query: String,
    pub selected_index: usize,
    pub focus_target: EmojiPickerFocusTarget,
}

impl EmojiPickerState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.query.clear();
        self.selected_index = 0;
        self.focus_target = EmojiPickerFocusTarget::Search;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.query.clear();
        self.selected_index = 0;
        self.focus_target = EmojiPickerFocusTarget::Composer;
    }

    pub fn edit_query(&mut self, query: String) {
        self.query = query;
        self.selected_index = 0;
        self.focus_target = EmojiPickerFocusTarget::Search;
        self.clamp_selection();
    }

    pub fn navigate(&mut self, navigation: EmojiPickerNavigation) {
        let choice_count = self.filtered_choices().len();

        if choice_count == 0 {
            self.selected_index = 0;
            return;
        }

        self.selected_index = match navigation {
            EmojiPickerNavigation::Previous => self
                .selected_index
                .checked_sub(1)
                .unwrap_or(choice_count.saturating_sub(1)),
            EmojiPickerNavigation::Next => {
                if self.selected_index + 1 >= choice_count {
                    0
                } else {
                    self.selected_index + 1
                }
            }
            EmojiPickerNavigation::First => 0,
            EmojiPickerNavigation::Last => choice_count.saturating_sub(1),
        };
        self.focus_target = EmojiPickerFocusTarget::Search;
    }

    pub fn filtered_choices(&self) -> Vec<&'static EmojiChoice> {
        search_emoji(&self.query)
    }

    pub fn selected_choice(&self) -> Option<&'static EmojiChoice> {
        let choices = self.filtered_choices();
        choices.get(self.selected_index).copied()
    }

    fn clamp_selection(&mut self) {
        let choice_count = self.filtered_choices().len();
        if choice_count == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= choice_count {
            self.selected_index = choice_count - 1;
        }
    }
}

impl Default for EmojiPickerState {
    fn default() -> Self {
        Self {
            is_open: false,
            query: String::new(),
            selected_index: 0,
            focus_target: EmojiPickerFocusTarget::Composer,
        }
    }
}

pub fn search_emoji(query: &str) -> Vec<&'static EmojiChoice> {
    let terms = normalized_terms(query);

    EMOJI_CATALOG
        .iter()
        .filter(|choice| terms.is_empty() || choice.matches_terms(&terms))
        .collect()
}

fn normalized_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .filter(|term| !term.is_empty())
        .collect()
}
