//! The named-view rules, applied to a repository.
//!
//! Everything that decides *what* a named view list looks like lives here:
//! the name uniqueness inside a scope, the one-default-per-scope rule, the
//! id minting and the order of the list. The host contributes two things only:
//! where the views are stored (the repository) and how a new id is minted.

use std::sync::Arc;

use chrono::Utc;

use super::domain::dto::{NamedView, NamedViewScope};
use super::domain::error::NamedViewError;
use super::domain::repository::{NamedViewIdMinter, NamedViewRepository};
use super::domain::rules::view_names_equal;

/// The views of one scope, or of every scope when `scope` is `None`.
///
/// The order is the one the menu shows: the default view first, then the rest
/// by name.
pub async fn list_named_views(
    repository: &dyn NamedViewRepository,
    scope: Option<&NamedViewScope>,
) -> Result<Vec<NamedView>, NamedViewError> {
    let mut views = repository.load_views().await?;
    if let Some(scope) = scope {
        views.retain(|view| &view.scope == scope);
    }
    sort_views(&mut views);
    Ok(views)
}

/// Save a view and return the record as it was stored.
///
/// An empty `id` asks for a new view: the id is minted and both timestamps
/// are stamped. A known `id` is an update: the creation time is kept, the
/// update time moves.
///
/// A name that is already taken **in the same scope** by **another** view is
/// refused with [`NamedViewError::NameExists`]; the same name in another
/// scope is fine, because the scopes are different tables.
pub async fn save_named_view(
    repository: &dyn NamedViewRepository,
    minter: &dyn NamedViewIdMinter,
    view: NamedView,
) -> Result<NamedView, NamedViewError> {
    let mut view = view;
    view.normalize();
    view.validate().map_err(NamedViewError::Invalid)?;

    let mut views = repository.load_views().await?;
    let now = Utc::now();

    match views.iter().find(|existing| existing.id == view.id) {
        Some(existing) if !view.id.trim().is_empty() => {
            // A rename keeps the identity and the creation time.
            view.created_at = existing.created_at;
        }
        _ => {
            view.created_at = now;
        }
    }
    view.updated_at = Some(now);
    if view.id.trim().is_empty() {
        view.id = minter.mint_id();
    }

    if let Some(conflict) = views.iter().find(|existing| {
        existing.id != view.id
            && existing.scope == view.scope
            && view_names_equal(&existing.name, &view.name)
    }) {
        return Err(NamedViewError::NameExists {
            name: conflict.name.clone(),
        });
    }

    // At most one default per scope: making a view the default clears the
    // flag on the others of the same scope, in the same write.
    if view.is_default {
        for existing in views.iter_mut() {
            if existing.scope == view.scope && existing.id != view.id {
                existing.is_default = false;
            }
        }
    }

    match views.iter_mut().find(|existing| existing.id == view.id) {
        Some(slot) => *slot = view.clone(),
        None => views.push(view.clone()),
    }
    sort_views(&mut views);
    repository.save_views(&views).await?;
    Ok(view)
}

/// Delete a view and return the remaining ones.
///
/// Deleting an id nobody carries is **not** an error: the call is idempotent,
/// so a retry after a lost answer does not fail.
pub async fn delete_named_view(
    repository: &dyn NamedViewRepository,
    id: &str,
) -> Result<Vec<NamedView>, NamedViewError> {
    let mut views = repository.load_views().await?;
    let before = views.len();
    views.retain(|view| view.id != id);
    if views.len() != before {
        sort_views(&mut views);
        repository.save_views(&views).await?;
    }
    Ok(views)
}

/// Host-assembled use-case facade: holds the repository and the id minter so
/// commands do not have to source them separately (the same shape as
/// `SettingsService` / `ConnectionService`).
pub struct NamedViewService {
    repository: Arc<dyn NamedViewRepository>,
    minter: Arc<dyn NamedViewIdMinter>,
}

impl NamedViewService {
    pub fn new(
        repository: Arc<dyn NamedViewRepository>,
        minter: Arc<dyn NamedViewIdMinter>,
    ) -> Self {
        Self { repository, minter }
    }

    /// The views of one scope, or of every scope when `scope` is `None`.
    pub async fn list(
        &self,
        scope: Option<&NamedViewScope>,
    ) -> Result<Vec<NamedView>, NamedViewError> {
        list_named_views(self.repository.as_ref(), scope).await
    }

    /// Save a view and return the record as it was stored.
    pub async fn save(&self, view: NamedView) -> Result<NamedView, NamedViewError> {
        save_named_view(self.repository.as_ref(), self.minter.as_ref(), view).await
    }

    /// Delete a view (idempotent) and return the remaining ones.
    pub async fn delete(&self, id: &str) -> Result<Vec<NamedView>, NamedViewError> {
        delete_named_view(self.repository.as_ref(), id).await
    }
}

/// The order the menu shows: the default first, then by name.
///
/// The name comparison is the same case-insensitive one the uniqueness rule
/// uses, so `Users` and `users` cannot swap places between two renders. The
/// id breaks a tie, so the order is total and stable.
fn sort_views(views: &mut [NamedView]) {
    views.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| {
                super::domain::rules::normalized_name_key(&left.name)
                    .cmp(&super::domain::rules::normalized_name_key(&right.name))
            })
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
}
