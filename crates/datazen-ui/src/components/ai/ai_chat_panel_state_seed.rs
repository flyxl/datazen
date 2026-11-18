//! Store-sync seeders for [`AiChatPanelState`] — T009-S6.
//!
//! The second `impl` block of the panel state: the `set_*` forwarders
//! the host will use to write the sub-states once the `aiStore` slice
//! (T088) lands. Split out of `ai_chat_panel_state.rs` so both files
//! stay under the 800-line cap — the same split the codebase already
//! uses for `AppState` (`session_state.rs`, `settings_state.rs`, …).
//! The seeders go through the state's `pub(crate)` accessors instead
//! of the private fields, which is what makes the split possible.
//!
//! `append_user_message` stays in the main file: it is private and the
//! send path calls it from there.

// The panel view (S7) is the production consumer; until it lands the
// component tests are the only callers, so the dead-code lint is
// silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use super::ai_chat_header::{AiProfile, AiPromptTemplate};
use super::ai_chat_panel_state::AiChatPanelState;
use super::chat_bubble::{AiChatMessage, AiMemoryReference};

impl AiChatPanelState {
    // -- Store sync: the host writes the panel slice will make once T088
    // lands. Until then tests (and the shell's probe) drive these
    // directly. --

    /// Seed the message list from the store. New arrivals add to the
    /// unread count only while away from the bottom (`useTrackUnread`,
    /// `hooks/useAutoScroll.ts`); the panel itself never calls it in
    /// React as shipped, but the seam is wired here so a store arrival
    /// behaves the same way in the VM.
    pub(crate) fn set_messages(&mut self, messages: Vec<AiChatMessage>) {
        let added = messages
            .len()
            .saturating_sub(self.list_mut().messages().len());
        self.list_mut().set_messages(messages);
        let empty = self.list().messages().is_empty();
        self.header_mut().set_has_messages(!empty);
        if added > 0 {
            self.list_mut().note_new_messages(added as u32);
        }
    }

    /// Seed the streaming flag, mirrored onto the input area. Never
    /// seeds streaming without a session (see the module docs): a
    /// `true` while `ensure_session` has not run is ignored.
    pub(crate) fn set_streaming(&mut self, is_streaming: bool) {
        if is_streaming && !self.list_mut().has_session() {
            return;
        }
        self.list_mut().set_streaming(is_streaming);
        self.area_mut().set_streaming(is_streaming);
    }

    pub(crate) fn set_stream_content(&mut self, content: impl Into<String>) {
        self.list_mut().set_stream_content(content);
    }

    pub(crate) fn set_stream_reasoning(&mut self, reasoning: impl Into<String>) {
        self.list_mut().set_stream_reasoning(reasoning);
    }

    pub(crate) fn set_stream_mcp_tool_name(&mut self, name: Option<String>) {
        self.list_mut().set_stream_mcp_tool_name(name);
    }

    pub(crate) fn set_stream_memory_used(&mut self, memory: Vec<AiMemoryReference>) {
        self.list_mut().set_stream_memory_used(memory);
    }

    pub(crate) fn set_session_memory_used(&mut self, memory: Vec<AiMemoryReference>) {
        self.list_mut().set_session_memory_used(memory);
    }

    pub(crate) fn set_memory_candidate_count(&mut self, count: Option<u32>) {
        self.list_mut().set_memory_candidate_count(count);
    }

    pub(crate) fn dismiss_memory_candidate(&mut self) {
        self.list_mut().dismiss_memory_candidate();
    }

    pub(crate) fn dismiss_remember_notice(&mut self) {
        self.list_mut().dismiss_remember_notice();
    }

    /// S3 D1, fixed at this seam: an empty-string migration error is
    /// falsy in React's `{sessionMigrationError && …}` and renders no
    /// banner, so `""` normalises to `None` here instead of reaching
    /// the header state.
    pub(crate) fn set_session_migration_error(&mut self, error: Option<String>) {
        self.header_mut()
            .set_session_migration_error(error.filter(|value| !value.is_empty()));
    }

    pub(crate) fn set_session_migration_success(&mut self, count: u32) {
        self.header_mut().set_session_migration_success(count);
    }

    /// S3 D1, same seam as the migration error: an empty share error
    /// renders no span in React's `{shareError && …}`.
    pub(crate) fn set_share_error(&mut self, error: Option<String>) {
        self.header_mut()
            .set_share_error(error.filter(|value| !value.is_empty()));
    }

    pub(crate) fn set_can_share(&mut self, can_share: bool) {
        self.header_mut().set_can_share(can_share);
    }

    pub(crate) fn set_sharing(&mut self, sharing: bool) {
        self.header_mut().set_sharing(sharing);
    }

    pub(crate) fn set_templates(&mut self, templates: Vec<AiPromptTemplate>) {
        self.header_mut().set_templates(templates);
    }

    pub(crate) fn set_profiles(&mut self, profiles: Vec<AiProfile>) {
        self.header_mut().set_profiles(profiles);
    }

    pub(crate) fn set_active_profile_id(&mut self, profile_id: impl Into<String>) {
        self.header_mut().set_active_profile_id(profile_id);
    }
}
