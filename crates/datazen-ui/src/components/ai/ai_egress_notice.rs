//! The AI egress notice, ported from
//! `packages/frontend/src/components/ai/AiEgressNotice.tsx` (57 lines,
//! T103 on this segment's ledger, T105 on the tasklist's).
//!
//! The amber strip above the chat input that says what may leave the
//! machine: the host-delivered tool egress summary, the bound database
//! label, the relaxed-strict note, and the attached-context note.
//!
//! ## Key convention
//!
//! React's `data-testid`s become the element keys:
//!
//! | part | key |
//! |---|---|
//! | root | `ai-egress-notice` |
//! | summary line | `ai-egress-summary` |
//! | database line | `ai-egress-database` |
//! | message rows | `ai-egress-message-{index}` (new; React keys by text) |
//!
//! ## Deliberate boundaries
//!
//! * **The settings read is an input.** React reads
//!   `useSettingsStore((s) => s.settings.aiStrictEgress)` inside the
//!   component; the port takes the value
//!   ([`AiEgressNotice::strict_egress`], default true) because the
//!   settings-store task owns the store side, exactly like the chat input
//!   area's VM takes the same flag through `set_egress`.
//! * **`ContextItem` is the minimal kind the notice reads.** React's
//!   `ContextItem` (`types/index.ts:226`) carries id/name/path/database
//!   too; the notice only branches on `kind === 'file' || kind === 'dir'`,
//!   so the port carries kind + id + name and documents the rest away.
//! * **The chat input area keeps its VM placeholder.** Its
//!   `egress_notice` builder renders the same keys and text from
//!   `set_egress`'s precomputed flags; swapping it to this component
//!   needs the items themselves, which the panel wiring (the `AiInput`
//!   track) carries — the placeholder's own docs pin that swap.
//! * **Amber is React-verbatim** (`border-amber-500/30 bg-amber-500/10
//!   text-amber-200/90`): the `ai_amber_*` palette helpers
//!   (`ui/palette.rs`), the same ones the input area's placeholder uses.
//! * **`text-[11px]`** renders at the kit's 12pt `text_xsmall` (no 11pt
//!   step in the kit type scale — the input area documents the same
//!   rule). The summary's `font-medium` is `text_xsmall_medium`.
//! * **Null renders.** `!showRelaxed && !showContext && !showSummary`
//!   renders nothing (React `null`): [`build`] returns `Option<El>`, and
//!   the empty-summary rule is the trimmed summary being empty.

// The production consumer is the chat panel wiring (the input area's
// placeholder swaps when the wiring carries the items); until then the
// component tests are the only callers.
#![allow(dead_code)]

use rune::{col, text, El};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::palette::{ai_amber_border, ai_amber_fill, ai_amber_text};

/// One attached AI context item (`types/index.ts:226`). The notice reads
/// only the kind; id and name travel with it so a wiring can hand its
/// real items over (path/database stay with their consumers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextItem {
    pub kind: ContextKind,
    pub id: String,
    pub name: String,
}

/// `ContextKind` (`types/index.ts:224`): a file, a directory, or a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextKind {
    File,
    Dir,
    Table,
}

/// The notice. Build it with the context items, the host summary and the
/// database label, plus the settings value; `build` returns `None` when
/// React would render nothing.
pub struct AiEgressNotice<'a> {
    context_items: &'a [ContextItem],
    egress_summary: Option<&'a str>,
    database: Option<&'a str>,
    strict_egress: bool,
}

impl<'a> AiEgressNotice<'a> {
    pub fn new(
        context_items: &'a [ContextItem],
        egress_summary: Option<&'a str>,
        database: Option<&'a str>,
    ) -> Self {
        Self {
            context_items,
            egress_summary,
            database,
            strict_egress: true,
        }
    }

    /// The settings store's `aiStrictEgress` (default `true`): only an
    /// explicit `false` relaxes the notice.
    pub fn strict_egress(mut self, strict: bool) -> Self {
        self.strict_egress = strict;
        self
    }

    /// React's `contextFiles` filter (`AiEgressNotice.tsx:16`): the
    /// items whose kind is a file or a directory.
    pub fn context_files(&self) -> Vec<&'a ContextItem> {
        self.context_items
            .iter()
            .filter(|item| matches!(item.kind, ContextKind::File | ContextKind::Dir))
            .collect()
    }

    /// The trimmed host summary (`egressSummary?.trim() ?? ''`).
    fn summary_text(&self) -> Option<&'a str> {
        self.egress_summary.map(str::trim).filter(|s| !s.is_empty())
    }

    /// The trimmed database label, formatted through the locale key
    /// (`t('chat.egressDatabase', { database })`).
    fn database_label(&self) -> Option<String> {
        let database = self.database.map(str::trim)?;
        if database.is_empty() {
            return None;
        }
        Some(t("chat.egressDatabase").replacen("{database}", database, 1))
    }

    /// The relaxed / context messages in React's order
    /// (`AiEgressNotice.tsx:29–35`).
    fn messages(&self) -> Vec<&'static str> {
        let mut messages = Vec::new();
        if !self.strict_egress {
            messages.push(t("chat.egressNoticeRelaxed"));
        }
        if !self.context_files().is_empty() {
            messages.push(t("chat.egressNoticeContext"));
        }
        messages
    }

    /// The notice, or `None` when every condition is off
    /// (`!showRelaxed && !showContext && !showSummary`).
    pub fn build(self, dark: bool) -> Option<El> {
        let summary = self.summary_text();
        let database = self.database_label();
        let messages = self.messages();
        if self.strict_egress && messages.is_empty() && summary.is_none() {
            return None;
        }
        let k = kit(dark);
        let mut notice = col()
            .key("ai-egress-notice")
            .w_full()
            .min_w(0.0)
            .gap(2.0)
            .px(8.0)
            .py(6.0)
            .radius(k.radius.sm)
            .border(1.0, ai_amber_border(dark))
            .fill(ai_amber_fill(dark));
        if let Some(summary) = summary {
            notice = notice.child(
                text(summary)
                    .key("ai-egress-summary")
                    .font(k.ty.text_xsmall_medium)
                    .color(ai_amber_text(dark))
                    .wrap(),
            );
        }
        if let Some(database) = database {
            notice = notice.child(
                text(database)
                    .key("ai-egress-database")
                    .font(k.ty.text_xsmall)
                    .color(k.alpha(ai_amber_text(dark), 0.70))
                    .wrap(),
            );
        }
        for (index, message) in messages.into_iter().enumerate() {
            notice = notice.child(
                text(message)
                    .key(format!("ai-egress-message-{index}"))
                    .font(k.ty.text_xsmall)
                    .color(ai_amber_text(dark))
                    .wrap(),
            );
        }
        Some(notice)
    }
}
