//! Prefix folders over the keys loaded by the cursor-based Redis scan.
use std::collections::BTreeMap;

use rune::{col, tok::Theme, El};

use super::{control, AppState, KvWorkbench};

#[derive(Default)]
struct Folder {
    children: BTreeMap<String, Folder>,
    keys: Vec<usize>,
}

impl Folder {
    fn insert(&mut self, full: &str, index: usize) {
        let components: Vec<_> = full.split(':').collect();
        let mut folder = self;
        // Cap the actual nesting depth for keys that contain many separators.
        for part in components
            .iter()
            .take(components.len().saturating_sub(1))
            .take(8)
        {
            folder = folder.children.entry((*part).into()).or_default();
        }
        folder.keys.push(index);
    }
}

fn key_button(state: &KvWorkbench, index: usize, depth: usize, theme: Theme) -> El {
    let item = &state.keys[index];
    let key = item.key.clone();
    let selected = state.selected.as_deref() == Some(item.key.as_str());
    control(&format!("native-kv-key-{index}"), &item.key, theme)
        .w_full()
        .px((8 + depth * 12) as f32)
        .focus_selected(selected)
        .fill(if selected {
            theme.card
        } else {
            theme.control_off
        })
        .on_press(move |s: &mut AppState| s.kv_select(key.clone()))
}

fn append(
    mut out: El,
    folder: &Folder,
    prefix: &str,
    depth: usize,
    state: &KvWorkbench,
    theme: Theme,
) -> El {
    for (part, child) in &folder.children {
        let path = format!("{prefix}{part}:");
        let collapsed = state.collapsed_folders.contains(&path);
        let route = path.clone();
        let name = if part.is_empty() {
            "(empty prefix)"
        } else {
            part
        };
        out = out.child(
            control(
                &format!("native-kv-folder-{path}"),
                &format!("{} {name}", if collapsed { "▸" } else { "▾" }),
                theme,
            )
            .w_full()
            .px((8 + depth * 12) as f32)
            .expanded(!collapsed)
            .on_press(move |s: &mut AppState| {
                if let Some(vm) = &mut s.native_kv {
                    if !vm.collapsed_folders.remove(&route) {
                        vm.collapsed_folders.insert(route.clone());
                    }
                }
            }),
        );
        if !collapsed {
            out = append(out, child, &path, depth + 1, state, theme);
        }
    }
    for index in &folder.keys {
        out = out.child(key_button(state, *index, depth, theme));
    }
    out
}

pub(super) fn key_list(state: &KvWorkbench, theme: Theme) -> El {
    let out = col().key("native-kv-keys").w_full().gap(4.0);
    if state.tree_mode {
        let mut folders = Folder::default();
        for (i, item) in state.keys.iter().enumerate() {
            folders.insert(&item.key, i);
        }
        append(out, &folders, "", 0, state, theme)
    } else {
        (0..state.keys.len()).fold(out, |out, i| out.child(key_button(state, i, 0, theme)))
    }
}
