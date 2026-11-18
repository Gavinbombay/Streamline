//! Descriptor-pinned directory operations that prevent symlink traversal and
//! pathname TOCTOU races on Unix via `openat`, `fstatat`, `mkdirat`, `renameat`,
//! and `O_NOFOLLOW`.

#[cfg(not(unix))]
compile_error!(
    "Streamline requires a Unix target for descriptor-pinned O_NOFOLLOW filesystem safety."
);

use std::ffi::{CStr, CString};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};

const MAX_TEXT_READ_BYTES: usize = 16 * 1024 * 1024;
pub const DEFAULT_MAX_DIR_ENTRIES: usize = 16_384;
pub const MAX_ENTRY_NAME_BYTES: usize = 255;

/// A pinned directory handle that executes child lookups, reads, and atomic
/// writes relative to an open directory file descriptor on Unix.
#[derive(Debug)]
pub struct PinnedDir {
    fd: OwnedFd,
}

impl PinnedDir {
    /// Opens an existing non-symlink directory at `path` pinned by file descriptor,
    /// walking every path component from `/` or `.` with `O_NOFOLLOW`.
    pub fn open_existing_dir(path: &Path) -> io::Result<Self> {
        Self::open_existing_dir_internal(path, false)
    }

    /// Opens an existing non-symlink directory at `path` pinned by file descriptor,
    /// enforcing strict no-group-write and no-world-write checks (`(mode & 0o022) == 0`)
    /// on the target directory.
    pub fn open_existing_dir_strict(path: &Path) -> io::Result<Self> {
        Self::open_existing_dir_internal(path, true)
    }

    fn open_existing_dir_internal(path: &Path, strict_target_mode: bool) -> io::Result<Self> {
        let segments = validate_and_collect_segments(path)?;
        let (mut cur, anchor_st) = open_anchor_dir(path.is_absolute())?;
        if segments.is_empty() {
            verify_stat_trust(&anchor_st, false, strict_target_mode)?;
            return Ok(cur);
        }

        for (idx, c_seg) in segments.iter().enumerate() {
            let is_ancestor = idx + 1 < segments.len();
            let strict_mode_for_this_step = !is_ancestor && strict_target_mode;
            cur = open_child_dir_checked(&cur, c_seg, is_ancestor, strict_mode_for_this_step)?;
        }

        Ok(cur)
    }

    /// Opens an existing non-symlink directory at `path`, or creates missing
    /// subdirectories downward while walking every component from `/` or `.`
    /// with `O_NOFOLLOW`.
    pub fn open_or_create_dir(path: &Path) -> io::Result<Self> {
        let segments = validate_and_collect_segments(path)?;
        let (mut cur, anchor_st) = open_anchor_dir(path.is_absolute())?;
        if segments.is_empty() {
            verify_stat_trust(&anchor_st, false, false)?;
            return Ok(cur);
        }

        for (idx, c_seg) in segments.iter().enumerate() {
            let is_ancestor = idx + 1 < segments.len();
            match open_child_dir_checked(&cur, c_seg, is_ancestor, false) {
                Ok(next) => cur = next,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    for rem_seg in &segments[idx..] {
                        cur = cur.open_or_create_subdir_cstr(rem_seg, 0o755)?;
                    }
                    return Ok(cur);
                }
                Err(err) => return Err(err),
            }
        }

        Ok(cur)
    }

    /// Duplicates the underlying pinned directory descriptor into a new handle.
    pub fn try_clone(&self) -> io::Result<Self> {
        let cloned = self.fd.try_clone()?;
        Ok(Self { fd: cloned })
    }

    /// Opens an immediate non-symlink subdirectory `name` relative to `self`.
    pub fn open_subdir(&self, name: &str) -> io::Result<Self> {
        self.open_subdir_internal(name, false)
    }

    /// Opens an immediate non-symlink subdirectory `name` relative to `self`,
    /// enforcing strict no-group-write and no-world-write checks (`(mode & 0o022) == 0`).
    pub fn open_subdir_strict(&self, name: &str) -> io::Result<Self> {
        self.open_subdir_internal(name, true)
    }

    fn open_subdir_internal(
        &self,
        name: &str,
        strict_no_group_or_world_write: bool,
    ) -> io::Result<Self> {
        let c_name = validate_single_segment(name)?;
        open_child_dir_checked(self, &c_name, false, strict_no_group_or_world_write)
    }

    /// Opens or atomically creates immediate non-symlink subdirectory `name`
    /// with `mode` (masked with `0o755` to prevent group/world write bits) relative to `self`.
    pub fn open_or_create_subdir(&self, name: &str, mode: u32) -> io::Result<Self> {
        let c_name = validate_single_segment(name)?;
        self.open_or_create_subdir_cstr(&c_name, mode)
    }

    fn open_or_create_subdir_cstr(&self, c_name: &CStr, mode: u32) -> io::Result<Self> {
        let sanitized_mode = mode & 0o755;
        let mut created = false;

        match unsafe { fstatat_nofollow(self.fd.as_raw_fd(), c_name) } {
            Ok(st) => {
                if (st.st_mode & libc::S_IFMT) == libc::S_IFLNK {
                    return Err(symlink_forbidden_error());
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let rc = unsafe {
                    libc::mkdirat(
                        self.fd.as_raw_fd(),
                        c_name.as_ptr(),
                        sanitized_mode as libc::mode_t,
                    )
                };
                if rc == 0 {
                    created = true;
                } else {
                    let mk_err = io::Error::last_os_error();
                    if mk_err.raw_os_error() != Some(libc::EEXIST) {
                        return Err(mk_err);
                    }
                }
            }
            Err(err) => return Err(err),
        }

        let dir = open_child_dir_checked(self, c_name, false, false)?;
        if created
            && unsafe { libc::fchmod(dir.fd.as_raw_fd(), sanitized_mode as libc::mode_t) } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(dir)
    }

    /// Lists sorted valid UTF-8 entry names in this directory, excluding `.` and `..`,
    /// up to [`DEFAULT_MAX_DIR_ENTRIES`].
    pub fn list_entry_names(&self) -> io::Result<Vec<String>> {
        self.list_entry_names_bounded(DEFAULT_MAX_DIR_ENTRIES)
    }

    /// Lists sorted valid UTF-8 entry names in this directory, excluding `.` and `..`,
    /// returning `ErrorKind::InvalidData` immediately if entries exceed `max_entries`
    /// or `MAX_ENTRY_NAME_BYTES`.
    pub fn list_entry_names_bounded(&self, max_entries: usize) -> io::Result<Vec<String>> {
        let dup_fd = unsafe { libc::fcntl(self.fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
        if dup_fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let dirp = unsafe { libc::fdopendir(dup_fd) };
        if dirp.is_null() {
            let err = io::Error::last_os_error();
            unsafe {
                libc::close(dup_fd);
            }
            return Err(err);
        }

        struct DirGuard(*mut libc::DIR);
        impl Drop for DirGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe {
                        libc::closedir(self.0);
                    }
                }
            }
        }
        let _guard = DirGuard(dirp);
        unsafe {
            libc::rewinddir(dirp);
        }

        let mut names = Vec::new();
        let mut raw_entries: usize = 0;
        let max_raw_entries = max_entries.saturating_add(16);
        loop {
            unsafe {
                clear_errno();
            }
            let entry = unsafe { libc::readdir(dirp) };
            if entry.is_null() {
                if errno_checking_supported() {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error().unwrap_or(0) != 0 {
                        return Err(err);
                    }
                }
                break;
            }
            let c_name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            let raw_bytes = c_name.to_bytes();
            if raw_bytes == b"." || raw_bytes == b".." {
                continue;
            }
            raw_entries = raw_entries.saturating_add(1);
            if raw_entries > max_raw_entries {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("directory exceeds maximum allowed entry count ({max_entries})"),
                ));
            }
            if raw_bytes.len() > MAX_ENTRY_NAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "directory entry name exceeds maximum allowed {MAX_ENTRY_NAME_BYTES} bytes"
                    ),
                ));
            }
            let Ok(name_str) = c_name.to_str() else {
                continue;
            };
            if names.len() >= max_entries {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("directory exceeds maximum allowed entry count ({max_entries})"),
                ));
            }
            names.push(name_str.to_string());
        }

        names.sort();
        Ok(names)
    }

    /// Checks whether `name` in this directory is a regular file (`Ok(true)`) or
    /// a directory (`Ok(false)`), rejecting symlinks and special files.
    pub fn is_regular_file_nofollow(&self, name: &str) -> io::Result<bool> {
        let c_name = validate_single_segment(name)?;
        let st = unsafe { fstatat_nofollow(self.fd.as_raw_fd(), &c_name)? };
        match st.st_mode & libc::S_IFMT {
            libc::S_IFREG => Ok(true),
            libc::S_IFDIR => Ok(false),
            libc::S_IFLNK => Err(symlink_forbidden_error()),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "symlinks are forbidden",
            )),
        }
    }

    /// Opens an immediate regular file `name` read-only without following symlinks.
    pub fn open_file_nofollow(&self, name: &str) -> io::Result<std::fs::File> {
        self.open_file_nofollow_internal(name, false)
    }

    fn open_file_nofollow_internal(
        &self,
        name: &str,
        strict_no_group_or_world_write: bool,
    ) -> io::Result<std::fs::File> {
        let c_name = validate_single_segment(name)?;

        let pre_st = unsafe { fstatat_nofollow(self.fd.as_raw_fd(), &c_name)? };
        match pre_st.st_mode & libc::S_IFMT {
            libc::S_IFREG => {}
            libc::S_IFLNK => return Err(symlink_forbidden_error()),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "target is not a regular file",
                ));
            }
        }

        let fd = unsafe {
            libc::openat(
                self.fd.as_raw_fd(),
                c_name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            let err = io::Error::last_os_error();
            if is_symlink_errno(err.raw_os_error()) {
                return Err(symlink_forbidden_error());
            }
            if let Ok(st) = unsafe { fstatat_nofollow(self.fd.as_raw_fd(), &c_name) } {
                if (st.st_mode & libc::S_IFMT) == libc::S_IFLNK {
                    return Err(symlink_forbidden_error());
                }
            }
            return Err(err);
        }

        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        let st = unsafe { fstat_fd(owned.as_raw_fd())? };
        if (st.st_mode & libc::S_IFMT) != libc::S_IFREG {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "target is not a regular file",
            ));
        }
        verify_stat_trust(&st, false, strict_no_group_or_world_write)?;

        Ok(std::fs::File::from(owned))
    }

    /// Reads at most `max_bytes` from regular file `name` inside this directory
    /// without following symlinks.
    pub fn read_bytes_nofollow(&self, name: &str, max_bytes: usize) -> io::Result<Vec<u8>> {
        self.read_bytes_nofollow_internal(name, max_bytes, false)
    }

    /// Reads at most `max_bytes` from regular file `name` inside this directory
    /// without following symlinks, enforcing strict no-group-write and
    /// no-world-write checks (`(mode & 0o022) == 0`).
    pub fn read_bytes_nofollow_strict(&self, name: &str, max_bytes: usize) -> io::Result<Vec<u8>> {
        self.read_bytes_nofollow_internal(name, max_bytes, true)
    }

    fn read_bytes_nofollow_internal(
        &self,
        name: &str,
        max_bytes: usize,
        strict_no_group_or_world_write: bool,
    ) -> io::Result<Vec<u8>> {
        let file = self.open_file_nofollow_internal(name, strict_no_group_or_world_write)?;
        let meta = file.metadata()?;
        if meta.len() > max_bytes as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file exceeds maximum allowed byte size",
            ));
        }

        let cap = usize::try_from(meta.len()).unwrap_or(0).min(max_bytes);
        let mut buf = Vec::with_capacity(cap);
        let limit = (max_bytes as u64).saturating_add(1);
        file.take(limit).read_to_end(&mut buf)?;
        if buf.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file exceeds maximum allowed byte size",
            ));
        }
        Ok(buf)
    }

    /// Reads up to 16 MiB from regular file `name` as UTF-8 without following
    /// symlinks.
    pub fn read_to_string_nofollow(&self, name: &str) -> io::Result<String> {
        let bytes = self.read_bytes_nofollow(name, MAX_TEXT_READ_BYTES)?;
        String::from_utf8(bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    /// Atomically writes `contents` to `name` in this directory using a temp file
    /// and `renameat`, rejecting symlinks and directories, and masking mode with
    /// `0o755` so group/world write bits are never inherited.
    pub fn atomic_write_nofollow(&self, name: &str, contents: &[u8], mode: u32) -> io::Result<()> {
        let c_name = validate_single_segment(name)?;
        check_target_for_atomic_write(self.fd.as_raw_fd(), &c_name)?;

        let sanitized_mode = mode & 0o755;
        let tmp_name = format!(".tmp-{}", uuid::Uuid::new_v4());
        let c_tmp = CString::new(tmp_name.as_str()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "temp name contains NUL byte")
        })?;

        let tmp_fd = unsafe {
            libc::openat(
                self.fd.as_raw_fd(),
                c_tmp.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                sanitized_mode as libc::c_uint,
            )
        };
        if tmp_fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let owned_tmp = unsafe { OwnedFd::from_raw_fd(tmp_fd) };

        struct TempUnlinkGuard<'a> {
            dir_fd: RawFd,
            c_name: &'a CStr,
            armed: bool,
        }
        impl Drop for TempUnlinkGuard<'_> {
            fn drop(&mut self) {
                if self.armed {
                    unsafe {
                        libc::unlinkat(self.dir_fd, self.c_name.as_ptr(), 0);
                    }
                }
            }
        }
        let mut guard = TempUnlinkGuard {
            dir_fd: self.fd.as_raw_fd(),
            c_name: &c_tmp,
            armed: true,
        };

        let mut tmp_file = std::fs::File::from(owned_tmp);
        tmp_file.write_all(contents)?;
        tmp_file.sync_all()?;
        if unsafe { libc::fchmod(tmp_file.as_raw_fd(), sanitized_mode as libc::mode_t) } != 0 {
            return Err(io::Error::last_os_error());
        }
        drop(tmp_file);

        check_target_for_atomic_write(self.fd.as_raw_fd(), &c_name)?;

        let rc = unsafe {
            libc::renameat(
                self.fd.as_raw_fd(),
                c_tmp.as_ptr(),
                self.fd.as_raw_fd(),
                c_name.as_ptr(),
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        guard.armed = false;

        let _ = unsafe { libc::fsync(self.fd.as_raw_fd()) };
        Ok(())
    }
}

fn open_anchor_dir(is_absolute: bool) -> io::Result<(PinnedDir, libc::stat)> {
    let fd = if is_absolute {
        unsafe {
            libc::open(
                c"/".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        }
    } else {
        unsafe {
            libc::open(
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        }
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let st = unsafe { fstat_fd(owned.as_raw_fd())? };
    if (st.st_mode & libc::S_IFMT) != libc::S_IFDIR {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "target is not a directory",
        ));
    }
    verify_stat_trust(&st, true, false)?;
    Ok((PinnedDir { fd: owned }, st))
}

fn open_child_dir_checked(
    parent: &PinnedDir,
    c_seg: &CStr,
    is_ancestor: bool,
    strict_no_group_or_world_write: bool,
) -> io::Result<PinnedDir> {
    if let Ok(st) = unsafe { fstatat_nofollow(parent.fd.as_raw_fd(), c_seg) } {
        if (st.st_mode & libc::S_IFMT) == libc::S_IFLNK {
            return Err(symlink_forbidden_error());
        }
    }

    let fd = unsafe {
        libc::openat(
            parent.fd.as_raw_fd(),
            c_seg.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let err = io::Error::last_os_error();
        let errno = err.raw_os_error();
        if is_symlink_errno(errno) {
            return Err(symlink_forbidden_error());
        }
        if errno == Some(libc::ENOTDIR) {
            if let Ok(st) = unsafe { fstatat_nofollow(parent.fd.as_raw_fd(), c_seg) } {
                if (st.st_mode & libc::S_IFMT) == libc::S_IFLNK {
                    return Err(symlink_forbidden_error());
                }
            }
        }
        return Err(err);
    }

    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let st = unsafe { fstat_fd(owned.as_raw_fd())? };
    if (st.st_mode & libc::S_IFMT) != libc::S_IFDIR {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "target is not a directory",
        ));
    }
    verify_stat_trust(&st, is_ancestor, strict_no_group_or_world_write)?;

    Ok(PinnedDir { fd: owned })
}

fn verify_stat_trust(
    st: &libc::stat,
    is_ancestor: bool,
    strict_no_group_or_world_write: bool,
) -> io::Result<()> {
    let euid = unsafe { libc::geteuid() };
    let egid = unsafe { libc::getegid() };

    if st.st_uid != euid && st.st_uid != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "untrusted filesystem owner",
        ));
    }

    let mode = st.st_mode as libc::mode_t;
    if strict_no_group_or_world_write {
        if (mode & 0o022) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing group- or world-writable path",
            ));
        }
        return Ok(());
    }

    if (mode & 0o002) != 0 {
        let sticky_root_ancestor =
            is_ancestor && (mode & (libc::S_ISVTX as libc::mode_t)) != 0 && st.st_uid == 0;
        if !sticky_root_ancestor {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing world-writable path",
            ));
        }
    }

    if (mode & 0o020) != 0 {
        let trusted_group_write = if is_ancestor {
            st.st_uid == 0 || (st.st_uid == euid && st.st_gid == egid)
        } else {
            st.st_uid == euid && st.st_gid == egid
        };
        if !trusted_group_write {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing foreign group-writable path",
            ));
        }
    }

    Ok(())
}

fn validate_and_collect_segments(path: &Path) -> io::Result<Vec<CString>> {
    let raw = path.as_os_str().as_bytes();
    if raw.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must not be empty",
        ));
    }
    if raw.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains NUL byte",
        ));
    }

    let mut segments = Vec::new();
    for comp in path.components() {
        match comp {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(seg) => {
                let c_seg = CString::new(seg.as_bytes()).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL byte")
                })?;
                segments.push(c_seg);
            }
            Component::ParentDir | Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "parent directory traversal ('..') is forbidden",
                ));
            }
        }
    }
    Ok(segments)
}

fn check_target_for_atomic_write(dir_fd: RawFd, c_name: &CStr) -> io::Result<()> {
    match unsafe { fstatat_nofollow(dir_fd, c_name) } {
        Ok(st) => match st.st_mode & libc::S_IFMT {
            libc::S_IFLNK => Err(symlink_forbidden_error()),
            libc::S_IFDIR => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "target is a directory",
            )),
            libc::S_IFREG => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "target is not a regular file",
            )),
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn validate_single_segment(name: &str) -> io::Result<CString> {
    validate_segment_str(name)?;
    CString::new(name).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "path segment contains NUL byte",
        )
    })
}

fn is_symlink_errno(errno: Option<i32>) -> bool {
    matches!(errno, Some(libc::ELOOP) | Some(libc::EMLINK))
}

unsafe fn fstat_fd(fd: RawFd) -> io::Result<libc::stat> {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat(fd, &mut st) == 0 {
        Ok(st)
    } else {
        Err(io::Error::last_os_error())
    }
}

unsafe fn fstatat_nofollow(dir_fd: RawFd, c_name: &CStr) -> io::Result<libc::stat> {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstatat(dir_fd, c_name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) == 0 {
        Ok(st)
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
unsafe fn clear_errno() {
    *libc::__errno_location() = 0;
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
))]
unsafe fn clear_errno() {
    *libc::__error() = 0;
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
)))]
unsafe fn clear_errno() {}

fn errno_checking_supported() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
    ))
}

fn validate_segment_str(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid path segment",
        ));
    }
    Ok(())
}

fn symlink_forbidden_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "symlinks are forbidden")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    use std::path::PathBuf;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp dir");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod temp dir");
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn regular_write_read_and_list_work() {
        let tmp = TempDirGuard::new("pinned-fs-regular");
        let nested_path = tmp.path.join("alpha").join("beta");
        let dir = PinnedDir::open_or_create_dir(&nested_path).expect("open or create dir");

        dir.atomic_write_nofollow("zeta.txt", b"hello world\n", 0o644)
            .expect("atomic write zeta");
        dir.atomic_write_nofollow("alpha.txt", b"first\n", 0o644)
            .expect("atomic write alpha");
        dir.atomic_write_nofollow("zeta.txt", b"updated world\n", 0o644)
            .expect("overwrite zeta");

        let sub = dir
            .open_or_create_subdir("child_dir", 0o755)
            .expect("create subdir");
        sub.atomic_write_nofollow("inner.txt", b"inside\n", 0o644)
            .expect("write inner");

        let entries = dir.list_entry_names().expect("list entries");
        assert_eq!(entries, vec!["alpha.txt", "child_dir", "zeta.txt"]);

        assert!(dir
            .is_regular_file_nofollow("alpha.txt")
            .expect("alpha regular"));
        assert!(!dir
            .is_regular_file_nofollow("child_dir")
            .expect("child_dir is dir"));
        let missing_err = dir
            .is_regular_file_nofollow("missing.txt")
            .expect_err("missing must fail");
        assert_eq!(missing_err.kind(), io::ErrorKind::NotFound);

        let text = dir
            .read_to_string_nofollow("zeta.txt")
            .expect("read zeta string");
        assert_eq!(text, "updated world\n");

        let limit_err = dir
            .read_bytes_nofollow("zeta.txt", 4)
            .expect_err("exceeding max_bytes must fail");
        assert_eq!(limit_err.kind(), io::ErrorKind::InvalidInput);

        dir.atomic_write_nofollow("bad_utf8.bin", &[0xff, 0xfe, 0xfd], 0o644)
            .expect("write binary");
        let utf8_err = dir
            .read_to_string_nofollow("bad_utf8.bin")
            .expect_err("invalid utf8 must fail");
        assert_eq!(utf8_err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_invalid_segments_and_parent_traversal() {
        let tmp = TempDirGuard::new("pinned-fs-segments");
        let dir = PinnedDir::open_existing_dir(&tmp.path).expect("open existing");

        for bad in ["", ".", "..", "a/b", "a\\b", "bad\0name"] {
            assert_eq!(
                dir.open_file_nofollow(bad).expect_err("bad segment").kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(
                dir.open_subdir(bad).expect_err("bad subdir segment").kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(
                dir.atomic_write_nofollow(bad, b"x", 0o644)
                    .expect_err("bad write segment")
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }

        let parent_path = tmp.path.join("child").join("..").join("other");
        assert_eq!(
            PinnedDir::open_existing_dir(&parent_path)
                .expect_err("parent traversal open must fail")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            PinnedDir::open_or_create_dir(&parent_path)
                .expect_err("parent traversal create must fail")
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn rejects_symlinked_root_subdir_and_file() {
        let tmp = TempDirGuard::new("pinned-fs-symlinks");
        let real_dir = tmp.path.join("real_dir");
        let outside_dir = tmp.path.join("outside_dir");
        fs::create_dir_all(&real_dir).expect("create real_dir");
        fs::create_dir_all(&outside_dir).expect("create outside_dir");

        let outside_file = outside_dir.join("outside.txt");
        fs::write(&outside_file, b"OUTSIDE_DATA\n").expect("write outside");

        let symlink_root = tmp.path.join("link_root");
        symlink(&outside_dir, &symlink_root).expect("symlink root");

        let root_err = PinnedDir::open_existing_dir(&symlink_root)
            .expect_err("symlinked root must be rejected");
        assert_eq!(root_err.kind(), io::ErrorKind::InvalidInput);
        assert!(root_err.to_string().contains("symlinks are forbidden"));

        let create_root_err = PinnedDir::open_or_create_dir(&symlink_root)
            .expect_err("open_or_create on symlinked root must be rejected");
        assert_eq!(create_root_err.kind(), io::ErrorKind::InvalidInput);
        assert!(create_root_err
            .to_string()
            .contains("symlinks are forbidden"));

        let create_under_link_err = PinnedDir::open_or_create_dir(&symlink_root.join("nested/dir"))
            .expect_err("create under symlinked ancestor must be rejected");
        assert_eq!(create_under_link_err.kind(), io::ErrorKind::InvalidInput);
        assert!(create_under_link_err
            .to_string()
            .contains("symlinks are forbidden"));

        symlink(&outside_dir, real_dir.join("link_subdir")).expect("symlink subdir");
        symlink(&outside_file, real_dir.join("link_file")).expect("symlink file");

        let pinned = PinnedDir::open_existing_dir(&real_dir).expect("open real_dir");

        let subdir_err = pinned
            .open_subdir("link_subdir")
            .expect_err("symlinked subdir must be rejected");
        assert_eq!(subdir_err.kind(), io::ErrorKind::InvalidInput);
        assert!(subdir_err.to_string().contains("symlinks are forbidden"));

        let create_subdir_err = pinned
            .open_or_create_subdir("link_subdir", 0o755)
            .expect_err("open_or_create symlinked subdir must be rejected");
        assert_eq!(create_subdir_err.kind(), io::ErrorKind::InvalidInput);
        assert!(create_subdir_err
            .to_string()
            .contains("symlinks are forbidden"));

        let stat_file_err = pinned
            .is_regular_file_nofollow("link_file")
            .expect_err("symlinked file stat must be rejected");
        assert_eq!(stat_file_err.kind(), io::ErrorKind::InvalidInput);
        assert!(stat_file_err.to_string().contains("symlinks are forbidden"));

        let open_file_err = pinned
            .open_file_nofollow("link_file")
            .expect_err("symlinked file open must be rejected");
        assert_eq!(open_file_err.kind(), io::ErrorKind::InvalidInput);
        assert!(open_file_err.to_string().contains("symlinks are forbidden"));

        let read_file_err = pinned
            .read_bytes_nofollow("link_file", 1024)
            .expect_err("symlinked file read must be rejected");
        assert_eq!(read_file_err.kind(), io::ErrorKind::InvalidInput);
        assert!(read_file_err.to_string().contains("symlinks are forbidden"));

        let write_file_err = pinned
            .atomic_write_nofollow("link_file", b"OVERWRITE", 0o644)
            .expect_err("atomic write over symlink must be rejected");
        assert_eq!(write_file_err.kind(), io::ErrorKind::InvalidInput);
        assert!(write_file_err
            .to_string()
            .contains("symlinks are forbidden"));
        assert_eq!(
            fs::read_to_string(&outside_file).expect("outside untouched"),
            "OUTSIDE_DATA\n"
        );

        let write_dir_err = pinned
            .atomic_write_nofollow("link_subdir", b"OVERWRITE", 0o644)
            .expect_err("atomic write over symlinked dir must be rejected");
        assert_eq!(write_dir_err.kind(), io::ErrorKind::InvalidInput);

        fs::create_dir_all(real_dir.join("real_sub")).expect("create real_sub");
        let write_real_dir_err = pinned
            .atomic_write_nofollow("real_sub", b"OVERWRITE", 0o644)
            .expect_err("atomic write over real dir must be rejected");
        assert_eq!(write_real_dir_err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn rejects_intermediate_symlinked_ancestor_in_open_and_create() {
        let tmp = TempDirGuard::new("pinned-fs-anc-link");
        let real_parent = tmp.path.join("real_parent");
        let real_runs = real_parent.join("runs");
        fs::create_dir_all(&real_runs).expect("create real_parent/runs");

        let anc_link = tmp.path.join("anc_link");
        symlink(&real_parent, &anc_link).expect("create anc_link");

        let through_existing = anc_link.join("runs");
        let open_err = PinnedDir::open_existing_dir(&through_existing)
            .expect_err("open_existing_dir through symlinked ancestor must fail");
        assert_eq!(open_err.kind(), io::ErrorKind::InvalidInput);
        assert!(open_err.to_string().contains("symlinks are forbidden"));

        let create_existing_err = PinnedDir::open_or_create_dir(&through_existing)
            .expect_err("open_or_create_dir through symlinked ancestor must fail");
        assert_eq!(create_existing_err.kind(), io::ErrorKind::InvalidInput);
        assert!(create_existing_err
            .to_string()
            .contains("symlinks are forbidden"));

        let create_nested = anc_link.join("new_nested").join("runs");
        let create_nested_err = PinnedDir::open_or_create_dir(&create_nested)
            .expect_err("open_or_create_dir nested under symlinked ancestor must fail");
        assert_eq!(create_nested_err.kind(), io::ErrorKind::InvalidInput);
        assert!(create_nested_err
            .to_string()
            .contains("symlinks are forbidden"));
        assert!(!real_parent.join("new_nested").exists());
    }

    #[test]
    fn rejects_world_writable_dirs_and_files_and_strips_write_bits_on_create() {
        let tmp = TempDirGuard::new("pinned-fs-world-write");
        let world_dir = tmp.path.join("world_dir");
        let world_child = world_dir.join("child");
        fs::create_dir_all(&world_child).expect("create world_child");
        fs::set_permissions(&world_child, fs::Permissions::from_mode(0o755))
            .expect("chmod world_child");
        fs::set_permissions(&world_dir, fs::Permissions::from_mode(0o777))
            .expect("chmod world_dir");

        let open_world_err = PinnedDir::open_existing_dir(&world_dir)
            .expect_err("world-writable target dir must be rejected");
        assert_eq!(open_world_err.kind(), io::ErrorKind::PermissionDenied);

        let create_world_err = PinnedDir::open_or_create_dir(&world_dir)
            .expect_err("open_or_create on world-writable dir must be rejected");
        assert_eq!(create_world_err.kind(), io::ErrorKind::PermissionDenied);

        let anc_world_err = PinnedDir::open_existing_dir(&world_child)
            .expect_err("non-sticky world-writable ancestor must be rejected");
        assert_eq!(anc_world_err.kind(), io::ErrorKind::PermissionDenied);

        let safe_dir = tmp.path.join("safe_dir");
        fs::create_dir_all(&safe_dir).expect("create safe_dir");
        fs::set_permissions(&safe_dir, fs::Permissions::from_mode(0o755)).expect("chmod safe_dir");

        let world_file = safe_dir.join("world.txt");
        fs::write(&world_file, b"WORLD_WRITABLE\n").expect("write world.txt");
        fs::set_permissions(&world_file, fs::Permissions::from_mode(0o666))
            .expect("chmod world.txt");

        let world_sub = safe_dir.join("world_sub");
        fs::create_dir_all(&world_sub).expect("create world_sub");
        fs::set_permissions(&world_sub, fs::Permissions::from_mode(0o777))
            .expect("chmod world_sub");

        let pinned = PinnedDir::open_existing_dir(&safe_dir).expect("open safe_dir");
        let file_err = pinned
            .open_file_nofollow("world.txt")
            .expect_err("world-writable file must be rejected");
        assert_eq!(file_err.kind(), io::ErrorKind::PermissionDenied);

        let read_err = pinned
            .read_bytes_nofollow("world.txt", 1024)
            .expect_err("world-writable file read must be rejected");
        assert_eq!(read_err.kind(), io::ErrorKind::PermissionDenied);

        let sub_err = pinned
            .open_subdir("world_sub")
            .expect_err("world-writable subdir must be rejected");
        assert_eq!(sub_err.kind(), io::ErrorKind::PermissionDenied);

        let created_sub = pinned
            .open_or_create_subdir("created_sub", 0o777)
            .expect("create subdir with stripped write bits");
        let sub_mode = fs::metadata(safe_dir.join("created_sub"))
            .expect("created_sub meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(sub_mode & 0o022, 0);

        created_sub
            .atomic_write_nofollow("created.txt", b"safe\n", 0o666)
            .expect("atomic write with stripped write bits");
        let file_mode = fs::metadata(safe_dir.join("created_sub").join("created.txt"))
            .expect("created.txt meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode & 0o022, 0);
    }

    #[test]
    fn strict_methods_reject_group_writable_and_accept_strict_modes() {
        let tmp = TempDirGuard::new("pinned-fs-strict");
        let group_dir = tmp.path.join("group_dir");
        fs::create_dir_all(&group_dir).expect("create group_dir");
        fs::set_permissions(&group_dir, fs::Permissions::from_mode(0o775))
            .expect("chmod group_dir 0775");

        let strict_group_dir_err = PinnedDir::open_existing_dir_strict(&group_dir)
            .expect_err("strict open must reject 0775 group-writable dir");
        assert_eq!(strict_group_dir_err.kind(), io::ErrorKind::PermissionDenied);

        let group_meta = fs::metadata(&group_dir).expect("group_dir metadata");
        if group_meta.gid() == unsafe { libc::getegid() } {
            PinnedDir::open_existing_dir(&group_dir)
                .expect("non-strict open allows user-private-group 0775");
        }

        for dir_mode in [0o700, 0o755] {
            let strict_root = tmp.path.join(format!("strict_{dir_mode:o}"));
            let sub_ok = strict_root.join("sub_ok");
            let sub_group = strict_root.join("sub_group");
            fs::create_dir_all(&sub_ok).expect("create sub_ok");
            fs::create_dir_all(&sub_group).expect("create sub_group");
            fs::set_permissions(&sub_ok, fs::Permissions::from_mode(dir_mode))
                .expect("chmod sub_ok");
            fs::set_permissions(&sub_group, fs::Permissions::from_mode(0o775))
                .expect("chmod sub_group 0775");

            fs::write(strict_root.join("group.js"), b"bad\n").expect("write group.js");
            fs::set_permissions(
                strict_root.join("group.js"),
                fs::Permissions::from_mode(0o664),
            )
            .expect("chmod group.js 0664");

            fs::write(strict_root.join("strict_600.js"), b"ok600\n").expect("write strict_600.js");
            fs::set_permissions(
                strict_root.join("strict_600.js"),
                fs::Permissions::from_mode(0o600),
            )
            .expect("chmod strict_600.js 0600");

            fs::write(strict_root.join("strict_644.js"), b"ok644\n").expect("write strict_644.js");
            fs::set_permissions(
                strict_root.join("strict_644.js"),
                fs::Permissions::from_mode(0o644),
            )
            .expect("chmod strict_644.js 0644");

            fs::set_permissions(&strict_root, fs::Permissions::from_mode(dir_mode))
                .expect("chmod strict_root");

            let dir = PinnedDir::open_existing_dir_strict(&strict_root)
                .expect("strict open must accept 0700/0755");

            let sub_group_err = dir
                .open_subdir_strict("sub_group")
                .expect_err("strict subdir open must reject 0775");
            assert_eq!(sub_group_err.kind(), io::ErrorKind::PermissionDenied);

            dir.open_subdir_strict("sub_ok")
                .expect("strict subdir open must accept 0700/0755");

            let group_file_err = dir
                .read_bytes_nofollow_strict("group.js", 1024)
                .expect_err("strict read must reject 0664");
            assert_eq!(group_file_err.kind(), io::ErrorKind::PermissionDenied);

            assert_eq!(
                dir.read_bytes_nofollow_strict("strict_600.js", 1024)
                    .expect("strict read must accept 0600"),
                b"ok600\n"
            );
            assert_eq!(
                dir.read_bytes_nofollow_strict("strict_644.js", 1024)
                    .expect("strict read must accept 0644"),
                b"ok644\n"
            );
        }
    }

    #[test]
    fn list_entry_names_bounded_stops_before_exceeding_cap() {
        let tmp = TempDirGuard::new("pinned-fs-bounded-list");
        let dir = PinnedDir::open_existing_dir(&tmp.path).expect("open temp dir");

        let empty_sub = dir
            .open_or_create_subdir("empty_sub", 0o755)
            .expect("create empty_sub");
        assert!(empty_sub
            .list_entry_names_bounded(0)
            .expect("empty dir with max_entries=0 succeeds")
            .is_empty());

        for idx in 0..9 {
            dir.atomic_write_nofollow(&format!("item_{idx:02}.txt"), b"x\n", 0o644)
                .expect("write bounded list item");
        }
        // Total entries = 1 subdir (empty_sub) + 9 files = 10 entries.
        let cap_err = dir
            .list_entry_names_bounded(5)
            .expect_err("cap of 5 must fail on 10-entry directory");
        assert_eq!(cap_err.kind(), io::ErrorKind::InvalidData);

        let zero_err = dir
            .list_entry_names_bounded(0)
            .expect_err("cap of 0 must fail on non-empty directory");
        assert_eq!(zero_err.kind(), io::ErrorKind::InvalidData);

        let all = dir
            .list_entry_names_bounded(10)
            .expect("exact cap of 10 must succeed");
        assert_eq!(all.len(), 10);
    }
}
