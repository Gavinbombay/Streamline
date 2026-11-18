//! Descriptor-pinned directory operations that prevent symlink traversal and
//! pathname TOCTOU races on Unix via `openat`, `fstatat`, `mkdirat`, `renameat`,
//! and `O_NOFOLLOW`.

use std::io::{self, Read};
use std::path::{Component, Path};

#[cfg(unix)]
use std::ffi::{CStr, CString};
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

#[cfg(not(unix))]
use std::fs::{self, OpenOptions};
#[cfg(not(unix))]
use std::io::Write;
#[cfg(not(unix))]
use std::path::PathBuf;

const MAX_TEXT_READ_BYTES: usize = 16 * 1024 * 1024;

/// A pinned directory handle that executes child lookups, reads, and atomic
/// writes relative to an open directory file descriptor on Unix.
#[cfg(unix)]
#[derive(Debug)]
pub struct PinnedDir {
    fd: OwnedFd,
}

/// Non-Unix fallback pinned directory handle backed by a directory path.
#[cfg(not(unix))]
#[derive(Debug)]
pub struct PinnedDir {
    path: PathBuf,
}

impl PinnedDir {
    /// Reads at most `max_bytes` from regular file `name` inside this directory
    /// without following symlinks.
    pub fn read_bytes_nofollow(&self, name: &str, max_bytes: usize) -> io::Result<Vec<u8>> {
        let file = self.open_file_nofollow(name)?;
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
}

#[cfg(unix)]
impl PinnedDir {
    /// Opens an existing non-symlink directory at `path` pinned by file descriptor.
    pub fn open_existing_dir(path: &Path) -> io::Result<Self> {
        let c_path = path_to_cstring_no_trailing(path)?;

        if let Ok(st) = unsafe { lstat_path(&c_path) } {
            if (st.st_mode & libc::S_IFMT) == libc::S_IFLNK {
                return Err(symlink_forbidden_error());
            }
        }

        let fd = unsafe {
            libc::open(
                c_path.as_ptr(),
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
                if let Ok(st) = unsafe { lstat_path(&c_path) } {
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

        Ok(Self { fd: owned })
    }

    /// Opens an existing non-symlink directory at `path`, or creates missing
    /// subdirectories downward from the nearest existing non-symlink ancestor.
    pub fn open_or_create_dir(path: &Path) -> io::Result<Self> {
        if path.as_os_str().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must not be empty",
            ));
        }

        let mut missing_segments: Vec<String> = Vec::new();
        let mut cursor = path;

        let mut current_dir = loop {
            if cursor.as_os_str().is_empty() {
                break Self::open_existing_dir(Path::new("."))?;
            }
            match Self::open_existing_dir(cursor) {
                Ok(dir) => break dir,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    let mut comps = cursor.components();
                    let Some(last) = comps.next_back() else {
                        return Err(err);
                    };
                    match last {
                        Component::Normal(seg) => {
                            let seg_str = seg.to_str().ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidInput,
                                    "path segment is not valid UTF-8",
                                )
                            })?;
                            missing_segments.push(seg_str.to_string());
                            cursor = comps.as_path();
                        }
                        Component::CurDir => {
                            cursor = comps.as_path();
                        }
                        Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "cannot create directory across non-normal path component",
                            ));
                        }
                    }
                }
                Err(err) => return Err(err),
            }
        };

        for seg in missing_segments.into_iter().rev() {
            current_dir = current_dir.open_or_create_subdir(&seg, 0o755)?;
        }

        Ok(current_dir)
    }

    /// Duplicates the underlying pinned directory descriptor into a new handle.
    pub fn try_clone(&self) -> io::Result<Self> {
        let cloned = self.fd.try_clone()?;
        Ok(Self { fd: cloned })
    }

    /// Opens an immediate non-symlink subdirectory `name` relative to `self`.
    pub fn open_subdir(&self, name: &str) -> io::Result<Self> {
        let c_name = validate_single_segment(name)?;

        if let Ok(st) = unsafe { fstatat_nofollow(self.fd.as_raw_fd(), &c_name) } {
            if (st.st_mode & libc::S_IFMT) == libc::S_IFLNK {
                return Err(symlink_forbidden_error());
            }
        }

        let fd = unsafe {
            libc::openat(
                self.fd.as_raw_fd(),
                c_name.as_ptr(),
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
                if let Ok(st) = unsafe { fstatat_nofollow(self.fd.as_raw_fd(), &c_name) } {
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

        Ok(Self { fd: owned })
    }

    /// Opens or atomically creates immediate non-symlink subdirectory `name`
    /// with `mode` relative to `self`.
    pub fn open_or_create_subdir(&self, name: &str, mode: u32) -> io::Result<Self> {
        let c_name = validate_single_segment(name)?;

        match unsafe { fstatat_nofollow(self.fd.as_raw_fd(), &c_name) } {
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
                        (mode & 0o777) as libc::mode_t,
                    )
                };
                if rc < 0 {
                    let mk_err = io::Error::last_os_error();
                    if mk_err.raw_os_error() != Some(libc::EEXIST) {
                        return Err(mk_err);
                    }
                }
            }
            Err(err) => return Err(err),
        }

        self.open_subdir(name)
    }

    /// Lists sorted valid UTF-8 entry names in this directory, excluding `.` and `..`.
    pub fn list_entry_names(&self) -> io::Result<Vec<String>> {
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
            let Ok(name_str) = c_name.to_str() else {
                continue;
            };
            if name_str == "." || name_str == ".." {
                continue;
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

        Ok(std::fs::File::from(owned))
    }

    /// Atomically writes `contents` to `name` in this directory using a temp file
    /// and `renameat`, rejecting symlinks and directories.
    pub fn atomic_write_nofollow(&self, name: &str, contents: &[u8], mode: u32) -> io::Result<()> {
        let c_name = validate_single_segment(name)?;
        check_target_for_atomic_write(self.fd.as_raw_fd(), &c_name)?;

        let tmp_name = format!(".tmp-{}", uuid::Uuid::new_v4());
        let c_tmp = CString::new(tmp_name.as_str()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "temp name contains NUL byte")
        })?;

        let tmp_fd = unsafe {
            libc::openat(
                self.fd.as_raw_fd(),
                c_tmp.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                (mode & 0o777) as libc::c_uint,
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

#[cfg(unix)]
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

#[cfg(unix)]
fn validate_single_segment(name: &str) -> io::Result<CString> {
    validate_segment_str(name)?;
    CString::new(name).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "path segment contains NUL byte",
        )
    })
}

#[cfg(unix)]
fn path_to_cstring_no_trailing(path: &Path) -> io::Result<CString> {
    let raw = path.as_os_str().as_bytes();
    if raw.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must not be empty",
        ));
    }
    let trimmed = trim_trailing_slashes_and_dot(raw);
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must not be empty",
        ));
    }
    CString::new(trimmed)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL byte"))
}

#[cfg(unix)]
fn trim_trailing_slashes_and_dot(bytes: &[u8]) -> &[u8] {
    let mut b = bytes;
    loop {
        if b.len() > 1 && b.ends_with(b"/") {
            b = &b[..b.len() - 1];
            continue;
        }
        if b.len() > 2 && b.ends_with(b"/.") {
            b = &b[..b.len() - 2];
            continue;
        }
        if b == b"/." {
            b = b"/";
            continue;
        }
        break;
    }
    b
}

#[cfg(unix)]
fn is_symlink_errno(errno: Option<i32>) -> bool {
    matches!(errno, Some(libc::ELOOP) | Some(libc::EMLINK))
}

#[cfg(unix)]
unsafe fn lstat_path(c_path: &CStr) -> io::Result<libc::stat> {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::lstat(c_path.as_ptr(), &mut st) == 0 {
        Ok(st)
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
unsafe fn fstat_fd(fd: RawFd) -> io::Result<libc::stat> {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat(fd, &mut st) == 0 {
        Ok(st)
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
unsafe fn fstatat_nofollow(dir_fd: RawFd, c_name: &CStr) -> io::Result<libc::stat> {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstatat(dir_fd, c_name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) == 0 {
        Ok(st)
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
unsafe fn clear_errno() {
    *libc::__errno_location() = 0;
}

#[cfg(all(
    unix,
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
    )
))]
unsafe fn clear_errno() {
    *libc::__error() = 0;
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
    ))
))]
unsafe fn clear_errno() {}

#[cfg(unix)]
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

#[cfg(not(unix))]
impl PinnedDir {
    /// Opens an existing non-symlink directory at `path`.
    pub fn open_existing_dir(path: &Path) -> io::Result<Self> {
        if path.as_os_str().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must not be empty",
            ));
        }
        if path.to_string_lossy().contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path contains NUL byte",
            ));
        }
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() {
            return Err(symlink_forbidden_error());
        }
        if !meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "target is not a directory",
            ));
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Opens an existing non-symlink directory at `path`, or creates missing
    /// subdirectories downward from the nearest existing non-symlink ancestor.
    pub fn open_or_create_dir(path: &Path) -> io::Result<Self> {
        if path.as_os_str().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must not be empty",
            ));
        }
        let mut missing_segments: Vec<String> = Vec::new();
        let mut cursor = path;
        let mut current_dir = loop {
            if cursor.as_os_str().is_empty() {
                break Self::open_existing_dir(Path::new("."))?;
            }
            match Self::open_existing_dir(cursor) {
                Ok(dir) => break dir,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    let mut comps = cursor.components();
                    let Some(last) = comps.next_back() else {
                        return Err(err);
                    };
                    match last {
                        Component::Normal(seg) => {
                            let seg_str = seg.to_str().ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidInput,
                                    "path segment is not valid UTF-8",
                                )
                            })?;
                            missing_segments.push(seg_str.to_string());
                            cursor = comps.as_path();
                        }
                        Component::CurDir => cursor = comps.as_path(),
                        Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "cannot create directory across non-normal path component",
                            ));
                        }
                    }
                }
                Err(err) => return Err(err),
            }
        };
        for seg in missing_segments.into_iter().rev() {
            current_dir = current_dir.open_or_create_subdir(&seg, 0o755)?;
        }
        Ok(current_dir)
    }

    /// Clones this pinned directory handle.
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            path: self.path.clone(),
        })
    }

    /// Opens an immediate non-symlink subdirectory `name` relative to `self`.
    pub fn open_subdir(&self, name: &str) -> io::Result<Self> {
        validate_segment_str(name)?;
        Self::open_existing_dir(&self.path.join(name))
    }

    /// Opens or creates immediate non-symlink subdirectory `name`.
    pub fn open_or_create_subdir(&self, name: &str, _mode: u32) -> io::Result<Self> {
        validate_segment_str(name)?;
        let child = self.path.join(name);
        match fs::symlink_metadata(&child) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(symlink_forbidden_error());
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                if let Err(mk_err) = fs::create_dir(&child) {
                    if mk_err.kind() != io::ErrorKind::AlreadyExists {
                        return Err(mk_err);
                    }
                }
            }
            Err(err) => return Err(err),
        }
        Self::open_existing_dir(&child)
    }

    /// Lists sorted valid UTF-8 entry names in this directory, excluding `.` and `..`.
    pub fn list_entry_names(&self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            let os_name = entry.file_name();
            if let Some(name_str) = os_name.to_str() {
                if name_str != "." && name_str != ".." {
                    names.push(name_str.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// Checks whether `name` in this directory is a regular file (`Ok(true)`) or
    /// a directory (`Ok(false)`), rejecting symlinks and special files.
    pub fn is_regular_file_nofollow(&self, name: &str) -> io::Result<bool> {
        validate_segment_str(name)?;
        let meta = fs::symlink_metadata(self.path.join(name))?;
        if meta.file_type().is_symlink() {
            return Err(symlink_forbidden_error());
        }
        if meta.is_file() {
            Ok(true)
        } else if meta.is_dir() {
            Ok(false)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "symlinks are forbidden",
            ))
        }
    }

    /// Opens an immediate regular file `name` read-only without following symlinks.
    pub fn open_file_nofollow(&self, name: &str) -> io::Result<std::fs::File> {
        validate_segment_str(name)?;
        let target = self.path.join(name);
        let meta = fs::symlink_metadata(&target)?;
        if meta.file_type().is_symlink() {
            return Err(symlink_forbidden_error());
        }
        if !meta.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "target is not a regular file",
            ));
        }
        std::fs::File::open(target)
    }

    /// Atomically writes `contents` to `name` in this directory.
    pub fn atomic_write_nofollow(&self, name: &str, contents: &[u8], _mode: u32) -> io::Result<()> {
        validate_segment_str(name)?;
        let target = self.path.join(name);
        check_target_non_unix(&target)?;

        let tmp_path = self.path.join(format!(".tmp-{}", uuid::Uuid::new_v4()));
        let mut tmp_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        let write_res = (|| -> io::Result<()> {
            tmp_file.write_all(contents)?;
            tmp_file.sync_all()?;
            Ok(())
        })();
        drop(tmp_file);
        if let Err(err) = write_res {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }

        if let Err(err) = check_target_non_unix(&target) {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }
        if let Err(err) = fs::rename(&tmp_path, &target) {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }
        Ok(())
    }
}

#[cfg(not(unix))]
fn check_target_non_unix(target: &Path) -> io::Result<()> {
    match fs::symlink_metadata(target) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                Err(symlink_forbidden_error())
            } else if meta.is_dir() {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "target is a directory",
                ))
            } else if meta.is_file() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "target is not a regular file",
                ))
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
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
    use std::path::PathBuf;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp dir");
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
    fn rejects_invalid_segments() {
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
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_root_subdir_and_file() {
        use std::os::unix::fs::symlink;

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
}
