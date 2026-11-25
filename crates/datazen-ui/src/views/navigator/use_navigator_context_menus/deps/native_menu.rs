//! React `NativeMenuItemDef` (`lib/nativeContextMenu.ts`, defined in
//! `@datazen/driver-sdk`) — what a context menu is made of, generic over the
//! action type so the menu builders stay decoupled from what executing an
//! item means. Removal stage: shard 2's T567 ports the native menu facade
//! itself; this is the def shape only.
//!
//! `action: None` is React's "no handler wired" — the builders only emit
//! items whose handler exists, so `None` here is a modelling courtesy for
//! separators and submenus.

/// One entry of a context menu. Items carry the action selecting them
/// dispatches; separators and submenus carry none.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuNode<A> {
    Item { id: String, label: String, action: Option<A> },
    Separator,
    Submenu { id: String, label: String, items: Vec<MenuNode<A>> },
}

impl<A> MenuNode<A> {
    /// React's `{ kind: 'item', id, label, action }`.
    pub fn item(id: &str, label: &str, action: A) -> Self {
        Self::Item {
            id: id.to_owned(),
            label: label.to_owned(),
            action: Some(action),
        }
    }

    /// React's `item(id, label, action)` helper from
    /// `schemaTreeContextMenu.ts`: `undefined` action returns `null` and the
    /// caller drops it.
    pub fn maybe_item(id: &str, label: &str, action: Option<A>) -> Option<Self> {
        action.map(|a| Self::item(id, label, a))
    }

    pub fn separator() -> Self {
        Self::Separator
    }

    /// The item id, flattened through submenus — the stable identity the
    /// tests (and the host's menu keys) read.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Item { id, .. } => Some(id),
            Self::Submenu { id, .. } => Some(id),
            Self::Separator => None,
        }
    }
}
