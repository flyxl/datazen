//! Live CSV mapping, review and explicit append inside Settings.
use crate::state_def::AppState;
use crate::ui::field::TextField;
use rune::tok::{self, type_};
use rune::{col, row, text, El};

use super::native_transfer::{button, label};

pub(super) fn view(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let native = &state.native_csv;
    let page = col().key("import-section").w_full().gap(6.0).child(
        text("SQLite CSV Import")
            .font(type_::TITLE)
            .color(theme.text),
    );
    if native.busy() {
        return page.child(label("native-csv-working", "Working on the CSV…", theme));
    }
    if let Some(review) = &native.review {
        let mut page = page
            .child(label(
                "native-csv-review",
                format!(
                    "{} → {} / {}",
                    review.file_name, review.target_name, review.choice.table
                ),
                theme,
            ))
            .child(label(
                "native-csv-count",
                format!("Rows to append: {}", review.row_count),
                theme,
            ));
        for (index, column) in review.columns.iter().enumerate() {
            page = page.child(label(
                &format!("native-csv-column-{index}"),
                format!("{column} → {column}"),
                theme,
            ));
        }
        return page.child(
            row()
                .gap(8.0)
                .child(button("native-csv-append", "Append CSV rows", theme, |s| {
                    s.append_reviewed_csv()
                }))
                .child(button("native-csv-cancel", "Cancel", theme, |s| {
                    s.cancel_csv_review()
                })),
        );
    }
    if let Some(inserted) = native.inserted {
        return page
            .child(label(
                "native-csv-success",
                format!("Imported {inserted} rows."),
                theme,
            ))
            .child(button(
                "native-csv-restart",
                "Import another file",
                theme,
                |s| s.restart_csv_import(),
            ));
    }
    let mut profiles = row()
        .key("native-csv-profiles")
        .w_full()
        .gap(6.0)
        .flex_wrap();
    for profile in state
        .visible_connections()
        .into_iter()
        .filter(|p| p.engine == "sqlite")
    {
        let selected = native.profile.as_ref() == Some(&profile.id);
        let caption = if selected {
            format!("✓ {}", profile.name)
        } else {
            profile.name
        };
        let id = profile.id;
        profiles = profiles.child(button(
            format!("native-csv-profile-{id}"),
            caption,
            theme,
            move |s| s.pick_csv_profile(&id),
        ));
    }
    let mut page = page.child(label(
        "native-csv-scope",
        "Choose an existing SQLite table. UTF-8 CSV headers must match writable columns exactly.",
        theme,
    )).child(profiles)
      .child(row().key("import-step-2").gap(8.0).flex_wrap()
        .child(TextField::new("import-target-input", &state.import_target)
            .height(24.0).width(180.0).on_edit(|s: &mut AppState| &mut s.import_target).build(dark))
        .child(button("import-finish", "Preview CSV", theme, |s| s.import_finish()))
        .child(button("import-back", "Back", theme, |s| s.import_back())));
    if let Some(error) = &state.import_error {
        page = page.child(label("import-error", error.clone(), theme));
    }
    page
}
