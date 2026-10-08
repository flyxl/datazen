//! Query the linked SQLite runtime rather than assuming a version's default.

pub(super) fn linked_runtime_limit() -> usize {
    let mut handle = std::ptr::null_mut();
    // SAFETY: the NUL-terminated memory name is valid, and SQLite initializes
    // the output pointer even on failure. This private handle is never shared.
    let opened = unsafe { libsqlite3_sys::sqlite3_open(c":memory:".as_ptr(), &mut handle) };
    let limit = if opened == libsqlite3_sys::SQLITE_OK {
        // SAFETY: handle is an open database; -1 queries without changing it.
        unsafe {
            libsqlite3_sys::sqlite3_limit(handle, libsqlite3_sys::SQLITE_LIMIT_VARIABLE_NUMBER, -1)
        }
    } else {
        // A failed probe must not advertise an unproved bulk capacity.
        1
    };
    if !handle.is_null() {
        // SAFETY: close exactly once, including a partially opened error handle.
        unsafe { libsqlite3_sys::sqlite3_close(handle) };
    }
    usize::try_from(limit).unwrap_or(1).max(1)
}
