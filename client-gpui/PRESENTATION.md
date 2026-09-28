# Conversation presentation (#38)

The pane layout and semantic IDs are in `src/main.rs`, `Hamlet::conversation_panes`; theme colors and bundled icon mapping are in its `theme` module. The independent channel and conversation decisions are in `src/conversation.rs`; wire decoding and protected HTTP calls are in `src/http.rs`.

The channel list is in server order, initially selects its first channel, and only requests the newest page for the selected channel. Selecting another channel retains already loaded data for re-selection. There is no send, refresh, or older-page navigation yet. Message rows use Kit `SelectableText` with plain text and line breaks. Sessions and conversations remain memory-only.

`cargo test --locked` exercises the real Kit controls and semantic IDs, including a production-message drag-selection and Ctrl+C preserving line breaks. The initial attempt used a test window without Kit `Root`, which supplies the selection layer and copy action; the test now mounts `Root` just like `main`. The separate #36 feasibility probe also verifies variable-height scrolling and headless copy. **Native Linux selection/copy on a physical desktop was not manually verified** in this environment; headless tests do not prove native clipboard integration. Perform a native manual check before claiming it.
