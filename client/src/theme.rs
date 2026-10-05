pub(crate) const BACKGROUND: u32 = 0xf6f7fb;
pub(crate) const SIDEBAR: u32 = 0xe9edf5;
pub(crate) const SELECTED: u32 = 0xcddbf5;
pub(crate) const TEXT: u32 = 0x182338;
pub(crate) const MUTED: u32 = 0x586477;

pub(crate) fn channel_icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::Hash
}

pub(crate) fn send_icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::Send
}
