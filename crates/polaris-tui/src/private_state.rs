//! Linux startup: tighten the project state directory to private before first use.
//! The sharing owners require the directory to be private to the user.
use polaris_core::project_mutation::open_directory_nofollow;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

/// Tighten only the pinned, user-owned project state directory. Do not follow
/// aliases, change an ancestor, or relax the intent owner's private-mode check.
pub(super) fn prepare_state(project: &Path, state: &Path) -> Result<(), String> {
    let expected = polaris_core::project::state_dir(project).map_err(|e| e.to_string())?;
    if fs::canonicalize(&expected).map_err(|e| e.to_string())? != state {
        return Err("Project and storage location mismatch".into());
    }
    let fd = open_directory_nofollow(state).map_err(|e| e.to_string())?;
    let m = fd.metadata().map_err(|e| e.to_string())?;
    if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7000 != 0 {
        return Err("Invalid storage owner or mode".into());
    }
    fd.set_permissions(fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    fd.sync_all().map_err(|e| e.to_string())?;
    let fresh = open_directory_nofollow(state)
        .and_then(|f| f.metadata())
        .map_err(|e| e.to_string())?;
    if (fresh.dev(), fresh.ino()) != (m.dev(), m.ino()) || fresh.mode() & 0o7777 != 0o700 {
        return Err("Storage location changed".into());
    }
    Ok(())
}
