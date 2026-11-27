//! Read only the registry belonging to this adapter. The picker is not a
//! registry, and nothing here can execute, serve, install or activate a package.
use datazen_ui_api::{types::wapp::WappSummary, WappListDelivery, WappListError};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};

type Snapshot = Result<Vec<WappSummary>, WappListError>;

#[derive(Default)]
pub(crate) struct WappListing(Option<(u32, Receiver<Snapshot>)>);

impl WappListing {
    pub(crate) fn begin(&mut self, generation: u32, data_dir: PathBuf) -> bool {
        if self.0.is_some() {
            return false;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("workspace-app-list".into())
            .spawn(move || {
                let _ = sender.send(read_list(&data_dir));
            })
            .is_err()
        {
            return false;
        }
        self.0 = Some((generation, receiver));
        true
    }

    pub(crate) fn poll(&mut self) -> Option<WappListDelivery> {
        let (generation, receiver) = self.0.as_ref()?;
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(WappListError::Unreadable),
        };
        let generation = *generation;
        self.0 = None;
        Some(WappListDelivery { generation, result })
    }
}

#[cfg(not(unix))]
fn read_list(_data_dir: &Path) -> Snapshot {
    // Do not silently fall back to a path-based scan on other platforms.
    Err(WappListError::Unavailable)
}

#[cfg(unix)]
fn read_list(data_dir: &Path) -> Snapshot {
    unix::read_list(data_dir).map_err(|()| WappListError::Unreadable)
}

#[cfg(unix)]
mod unix {
    use super::WappSummary;
    use datazen::workspace_apps::{
        is_valid_wapp_id,
        manager::{LoadedWapp, ENABLED_MARKER_FILE},
        summary::WappSummary as OriginalSummary,
        validate_wapp_dir, MAX_WAPP_FILES, MAX_WAPP_UNCOMPRESSED,
    };
    use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags, Stat};
    use std::{
        ffi::OsStr,
        fs::{self as std_fs, File},
        io::{Read, Write},
        os::fd::OwnedFd,
        path::{Component, Path},
    };

    const MAX_REGISTRY_ENTRIES: usize = 512;
    const MAX_PACKAGES: usize = 64;
    const MAX_TOTAL_NODES: usize = 12_000;
    const MAX_DEPTH: usize = 20;
    const MAX_MANIFEST_OR_SVG: u64 = 256 * 1024;
    const MAX_COPIED_CONTENT: u64 = 8 * 1024 * 1024;
    const DIR: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    // Resolve each configured root component beneath its parent handle. Never
    // follow a symlink (including in the native profile's absolute prefix) or
    // consult a sibling, a Tauri profile, or a picker path.
    fn open_registry(data_dir: &Path) -> Result<Option<OwnedFd>, ()> {
        if data_dir
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(());
        }
        let start = if data_dir.is_absolute() { "/" } else { "." };
        let mut at = fs::open(start, DIR, Mode::empty()).map_err(|_| ())?;
        for part in data_dir
            .components()
            .chain([Component::Normal(OsStr::new("wapps"))])
        {
            let name = match part {
                Component::RootDir | Component::CurDir => continue,
                Component::Normal(name) => name,
                _ => return Err(()),
            };
            at = match fs::openat(&at, name, DIR, Mode::empty()) {
                Ok(child) => child,
                Err(rustix::io::Errno::NOENT) => return Ok(None),
                Err(_) => return Err(()),
            };
        }
        Ok(Some(at))
    }

    pub(super) fn read_list(data_dir: &Path) -> Result<Vec<WappSummary>, ()> {
        let Some(root) = open_registry(data_dir)? else {
            return Ok(Vec::new()); // No creation, including no scratch.
        };
        let mut result = Vec::new();
        let mut remaining_nodes = MAX_TOTAL_NODES;
        let mut remaining_copied_bytes = MAX_COPIED_CONTENT;
        for (index, entry) in Dir::read_from(&root).map_err(|_| ())?.enumerate() {
            if index >= MAX_REGISTRY_ENTRIES {
                return Err(());
            }
            let entry = entry.map_err(|_| ())?;
            let name = entry.file_name().to_str().map_err(|_| ())?;
            // Original staging/backup entries, or names that cannot belong to
            // this registry, aren't installed packages. Package-shaped names
            // must validate; they must not be discarded as if nothing existed.
            if name.starts_with('.') {
                continue;
            }
            if !is_valid_wapp_id(name) {
                inspect_foreign_entry(&root, name)?;
                continue;
            }
            if result.len() >= MAX_PACKAGES {
                return Err(());
            }
            let package = fs::openat(&root, name, DIR, Mode::empty()).map_err(|_| ())?;
            let enabled = match fs::statat(&package, ENABLED_MARKER_FILE, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(stat) if single_regular(&stat) => true,
                Err(rustix::io::Errno::NOENT) => false,
                _ => return Err(()),
            };

            // The shared validator works over paths and reads the manifest and
            // SVG files. Give it a private, link-free snapshot. Other assets
            // are sparse size placeholders: validation only inspects their
            // type/size/extension/presence, never their contents. Nothing from
            // this temporary directory is published or made executable.
            let scratch = tempfile::tempdir().map_err(|_| ())?;
            let mirror = scratch.path().join(name);
            std_fs::create_dir(&mirror).map_err(|_| ())?;
            let mut budget = Budget {
                nodes: &mut remaining_nodes,
                copied_bytes: &mut remaining_copied_bytes,
                files: MAX_WAPP_FILES,
                package_bytes: MAX_WAPP_UNCOMPRESSED,
            };
            mirror_contents(&package, &mirror, 0, &mut budget)?;
            let manifest = validate_wapp_dir(&mirror).map_err(|_| ())?;
            let projection = OriginalSummary::from(&LoadedWapp { manifest, enabled });
            let json = serde_json::to_value(projection).map_err(|_| ())?;
            result.push(serde_json::from_value::<WappSummary>(json).map_err(|_| ())?);
        }
        result.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        Ok(result)
    }

    fn inspect_foreign_entry(root: &OwnedFd, name: &str) -> Result<(), ()> {
        let stat = fs::statat(root, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| ())?;
        match FileType::from_raw_mode(stat.st_mode) {
            FileType::RegularFile => Ok(()),
            FileType::Directory => {
                let child = fs::openat(root, name, DIR, Mode::empty()).map_err(|_| ())?;
                match fs::statat(&child, "manifest.json", AtFlags::SYMLINK_NOFOLLOW) {
                    // A package claiming an invalid directory ID is a corrupt
                    // registry entry, not a successful empty snapshot.
                    Err(rustix::io::Errno::NOENT) => Ok(()),
                    _ => Err(()),
                }
            }
            _ => Err(()),
        }
    }

    struct Budget<'a> {
        nodes: &'a mut usize,
        copied_bytes: &'a mut u64,
        files: usize,
        package_bytes: u64,
    }

    fn mirror_contents(
        directory: &OwnedFd,
        mirror: &Path,
        depth: usize,
        budget: &mut Budget<'_>,
    ) -> Result<(), ()> {
        if depth > MAX_DEPTH {
            return Err(());
        }
        for entry in Dir::read_from(directory).map_err(|_| ())? {
            *budget.nodes = budget.nodes.checked_sub(1).ok_or(())?;
            let entry = entry.map_err(|_| ())?;
            let name = entry.file_name().to_str().map_err(|_| ())?;
            // Same package scanner convention: hidden state is never read.
            if name.starts_with('.') {
                continue;
            }
            let info = fs::statat(directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| ())?;
            let destination = mirror.join(name);
            match FileType::from_raw_mode(info.st_mode) {
                FileType::Directory => {
                    let child = fs::openat(directory, name, DIR, Mode::empty()).map_err(|_| ())?;
                    std_fs::create_dir(&destination).map_err(|_| ())?;
                    mirror_contents(&child, &destination, depth + 1, budget)?;
                }
                FileType::RegularFile if single_regular(&info) => {
                    let bytes: u64 = info.st_size.try_into().map_err(|_| ())?;
                    budget.files = budget.files.checked_sub(1).ok_or(())?;
                    budget.package_bytes = budget.package_bytes.checked_sub(bytes).ok_or(())?;
                    let read_content = (depth == 0 && name == "manifest.json")
                        || destination
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"));
                    let mut output = File::create_new(destination).map_err(|_| ())?;
                    if read_content {
                        if bytes > MAX_MANIFEST_OR_SVG {
                            return Err(());
                        }
                        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC
                            | OFlags::NONBLOCK | OFlags::NOCTTY;
                        let input = fs::openat(directory, name, flags, Mode::empty()).map_err(|_| ())?;
                        let observed = fs::fstat(&input).map_err(|_| ())?;
                        if !single_regular(&observed)
                            || observed.st_dev != info.st_dev
                            || observed.st_ino != info.st_ino
                            || observed.st_size != info.st_size
                        {
                            return Err(());
                        }
                        *budget.copied_bytes = budget.copied_bytes.checked_sub(bytes).ok_or(())?;
                        let mut content = Vec::new();
                        File::from(input)
                            .take(bytes + 1)
                            .read_to_end(&mut content)
                            .map_err(|_| ())?;
                        if content.len() as u64 != bytes {
                            return Err(());
                        }
                        output.write_all(&content).map_err(|_| ())?;
                    } else {
                        output.set_len(bytes).map_err(|_| ())?;
                    }
                }
                // Do not follow symlinks, hard links or special files, even
                // if a package contains other valid files.
                _ => return Err(()),
            }
        }
        Ok(())
    }

    fn single_regular(stat: &Stat) -> bool {
        FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile && stat.st_nlink == 1
    }
}
