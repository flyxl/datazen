pub const NATIVE_CONTEXT_MENU_SURFACE: [&str; 9] = [
    "showNativeContextMenu",
    "hideNativeContextMenu",
    "normalizeNativeMenuItems",
    "nativeEditMenuItems",
    "createNativeContextMenuHandler",
    "ContextMenuPosition",
    "ContextMenuBridge",
    "NativeMenuItemDef",
    "NativeMenuPredefined",
];

pub const DRIVER_SDK_MENU_GAP: [&str; 4] = [
    "showNativeContextMenu bridge",
    "hideNativeContextMenu bridge",
    "createNativeContextMenuHandler binding",
    "contextMenuStore subscription",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexport_surface_is_pinned() {
        assert_eq!(NATIVE_CONTEXT_MENU_SURFACE.len(), 9);
        assert!(NATIVE_CONTEXT_MENU_SURFACE.contains(&"showNativeContextMenu"));
        assert!(NATIVE_CONTEXT_MENU_SURFACE.contains(&"nativeEditMenuItems"));
        assert!(NATIVE_CONTEXT_MENU_SURFACE.contains(&"NativeMenuPredefined"));
    }

    #[test]
    fn driver_sdk_gap_is_pinned() {
        assert_eq!(DRIVER_SDK_MENU_GAP.len(), 4);
        assert!(DRIVER_SDK_MENU_GAP.contains(&"contextMenuStore subscription"));
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        assert_eq!(crate::NATIVE_CONTEXT_MENU_SURFACE.len(), 9);
        assert_eq!(crate::DRIVER_SDK_MENU_GAP.len(), 4);
    }
}
