use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::{String, ToString};
use core::ffi::{c_char, c_int};

use axerrno::{LinuxError, LinuxResult};
use axfs::fops::OpenOptions;
use axio::{PollState, SeekFrom};
use axsync::Mutex;

use super::fd_ops::{FileLike, get_file_like};
use crate::{ctypes, utils::char_ptr_to_str};

// For getdents64
use core::ptr;

/// Redirects relative paths to /tmp (ramfs) to work around FAT filesystem issues
fn redirect_path(path: &str) -> String {
    if !path.starts_with('/') {
        alloc::format!("/tmp/{}", path)
    } else {
        path.to_string()
    }
}

pub struct File {
    inner: Mutex<axfs::fops::File>,
}

pub struct Dir {
    inner: Mutex<axfs::fops::Directory>,
    path: String,
}

impl File {
    fn new(inner: axfs::fops::File) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    fn add_to_fd_table(self) -> LinuxResult<c_int> {
        super::fd_ops::add_file_like(Arc::new(self))
    }

    fn from_fd(fd: c_int) -> LinuxResult<Arc<Self>> {
        let f = super::fd_ops::get_file_like(fd)?;
        f.into_any()
            .downcast::<Self>()
            .map_err(|_| LinuxError::EINVAL)
    }
}

impl Dir {
    fn new(inner: axfs::fops::Directory, path: String) -> Self {
        Self {
            inner: Mutex::new(inner),
            path,
        }
    }

    fn add_to_fd_table(self) -> LinuxResult<c_int> {
        super::fd_ops::add_file_like(Arc::new(self))
    }

    fn from_fd(fd: c_int) -> LinuxResult<Arc<Self>> {
        let f = super::fd_ops::get_file_like(fd)?;
        f.into_any()
            .downcast::<Self>()
            .map_err(|_| LinuxError::EINVAL)
    }
}

impl FileLike for File {
    fn read(&self, buf: &mut [u8]) -> LinuxResult<usize> {
        Ok(self.inner.lock().read(buf)?)
    }

    fn write(&self, buf: &[u8]) -> LinuxResult<usize> {
        Ok(self.inner.lock().write(buf)?)
    }

    fn stat(&self) -> LinuxResult<ctypes::stat> {
        let metadata = self.inner.lock().get_attr()?;
        let ty = metadata.file_type() as u8;
        let perm = metadata.perm().bits() as u32;
        let st_mode = ((ty as u32) << 12) | perm;
        Ok(ctypes::stat {
            st_ino: 1,
            st_nlink: 1,
            st_mode,
            st_uid: 1000,
            st_gid: 1000,
            st_size: metadata.size() as _,
            st_blocks: metadata.blocks() as _,
            st_blksize: 512,
            ..Default::default()
        })
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
        self
    }

    fn poll(&self) -> LinuxResult<PollState> {
        Ok(PollState {
            readable: true,
            writable: true,
        })
    }

    fn set_nonblocking(&self, _nonblocking: bool) -> LinuxResult {
        Ok(())
    }
}

impl FileLike for Dir {
    fn read(&self, _buf: &mut [u8]) -> LinuxResult<usize> {
        Err(LinuxError::EISDIR)
    }

    fn write(&self, _buf: &[u8]) -> LinuxResult<usize> {
        Err(LinuxError::EISDIR)
    }

    fn stat(&self) -> LinuxResult<ctypes::stat> {
        // Use the stored path to query metadata
        let metadata = axfs::api::metadata(&self.path)?;
        let ty = if metadata.is_dir() { 4u8 } else { 8u8 };
        let perm = metadata.permissions().mode() as u32;
        let st_mode = ((ty as u32) << 12) | (perm & 0o777);
        Ok(ctypes::stat {
            st_ino: 1,
            st_nlink: if metadata.is_dir() { 2 } else { 1 },
            st_mode,
            st_uid: 1000,
            st_gid: 1000,
            st_size: metadata.len() as _,
            st_blocks: ((metadata.len() + 511) / 512) as i64,
            st_blksize: 512,
            ..Default::default()
        })
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
        self
    }

    fn poll(&self) -> LinuxResult<PollState> {
        Ok(PollState {
            readable: true,
            writable: false,
        })
    }

    fn set_nonblocking(&self, _nonblocking: bool) -> LinuxResult {
        Ok(())
    }
}

/// Convert open flags to [`OpenOptions`].
fn flags_to_options(flags: c_int, _mode: ctypes::mode_t) -> OpenOptions {
    let flags = flags as u32;
    let mut options = OpenOptions::new();
    match flags & 0b11 {
        ctypes::O_RDONLY => options.read(true),
        ctypes::O_WRONLY => options.write(true),
        _ => {
            options.read(true);
            options.write(true);
        }
    };
    if flags & ctypes::O_APPEND != 0 {
        options.append(true);
    }
    if flags & ctypes::O_TRUNC != 0 {
        options.truncate(true);
    }
    if flags & ctypes::O_CREAT != 0 {
        options.create(true);
    }
    if flags & ctypes::O_EXEC != 0 {
        options.create_new(true);
    }
    options
}

/// Open a file by `filename` and insert it into the file descriptor table.
///
/// Return its index in the file table (`fd`). Return `EMFILE` if it already
/// has the maximum number of files open.
pub fn sys_open(filename: *const c_char, flags: c_int, mode: ctypes::mode_t) -> c_int {
    let filename = char_ptr_to_str(filename);
    debug!("sys_open <= {filename:?} {flags:#o} {mode:#o}");
    syscall_body!(sys_open, {
        let orig_path = filename?;
        let path = redirect_path(orig_path);
        let flags_u32 = flags as u32;
        
        // Check if O_DIRECTORY flag is set or if trying to open a directory
        let is_dir_request = (flags_u32 & ctypes::O_DIRECTORY) != 0;
        
        // Try to open as a directory first if O_DIRECTORY is set
        if is_dir_request || (flags_u32 & 0b11) == ctypes::O_RDONLY {
            // Check if path is a directory
            if let Ok(metadata) = axfs::api::metadata(&path) {
                if metadata.is_dir() {
                    let mut options = OpenOptions::new();
                    options.read(true);
                    let dir = axfs::fops::Directory::open_dir(&path, &options)?;
                    return Dir::new(dir, path).add_to_fd_table();
                }
            }
        }
        
        // Open as a regular file
        let options = flags_to_options(flags, mode);
        let file = axfs::fops::File::open(&path, &options)?;
        File::new(file).add_to_fd_table()
    })
}

/// Set the position of the file indicated by `fd`.
///
/// Return its position after seek.
pub fn sys_lseek(fd: c_int, offset: ctypes::off_t, whence: c_int) -> ctypes::off_t {
    debug!("sys_lseek <= {fd} {offset} {whence}");
    syscall_body!(sys_lseek, {
        let pos = match whence {
            0 => SeekFrom::Start(offset as _),
            1 => SeekFrom::Current(offset as _),
            2 => SeekFrom::End(offset as _),
            _ => return Err(LinuxError::EINVAL),
        };
        let off = File::from_fd(fd)?.inner.lock().seek(pos)?;
        Ok(off)
    })
}

/// Get the file metadata by `path` and write into `buf`.
///
/// Return 0 if success.
pub unsafe fn sys_stat(path: *const c_char, buf: *mut ctypes::stat) -> c_int {
    let path = char_ptr_to_str(path);
    debug!("sys_stat <= {:?} {:#x}", path, buf as usize);
    syscall_body!(sys_stat, {
        if buf.is_null() {
            return Err(LinuxError::EFAULT);
        }
        let orig_path = path?;
        let redirected = redirect_path(orig_path);
        let mut options = OpenOptions::new();
        options.read(true);
        let file = axfs::fops::File::open(&redirected, &options)?;
        let st = File::new(file).stat()?;
        debug!("sys_stat: path={redirected:?}, st_mode={:#o}, is_dir={}, size_of_stat={}", 
               st.st_mode, (st.st_mode & 0o170000) == 0o40000, core::mem::size_of::<ctypes::stat>());
        unsafe { *buf = st };
        Ok(0)
    })
}

/// Get file metadata by `fd` and write into `buf`.
///
/// Return 0 if success.
pub unsafe fn sys_fstat(fd: c_int, buf: *mut ctypes::stat) -> c_int {
    debug!("sys_fstat <= {} {:#x}", fd, buf as usize);
    syscall_body!(sys_fstat, {
        if buf.is_null() {
            return Err(LinuxError::EFAULT);
        }

        unsafe { *buf = get_file_like(fd)?.stat()? };
        Ok(0)
    })
}

/// Get the metadata of the symbolic link and write into `buf`.
///
/// Return 0 if success.
pub unsafe fn sys_lstat(path: *const c_char, buf: *mut ctypes::stat) -> ctypes::ssize_t {
    let path = char_ptr_to_str(path);
    debug!("sys_lstat <= {:?} {:#x}", path, buf as usize);
    syscall_body!(sys_lstat, {
        if buf.is_null() {
            return Err(LinuxError::EFAULT);
        }
        unsafe { *buf = Default::default() }; // TODO
        Ok(0)
    })
}

/// Get the path of the current directory.
#[allow(clippy::unnecessary_cast)] // `c_char` is either `i8` or `u8`
pub fn sys_getcwd(buf: *mut c_char, size: usize) -> *mut c_char {
    debug!("sys_getcwd <= {:#x} {}", buf as usize, size);
    syscall_body!(sys_getcwd, {
        if buf.is_null() {
            return Ok(core::ptr::null::<c_char>() as _);
        }
        let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, size as _) };
        let cwd = axfs::api::current_dir()?;
        let cwd = cwd.as_bytes();
        if cwd.len() < size {
            dst[..cwd.len()].copy_from_slice(cwd);
            dst[cwd.len()] = 0;
            Ok(buf)
        } else {
            Err(LinuxError::ERANGE)
        }
    })
}

/// Rename `old` to `new`
/// If new exists, it is first removed.
///
/// Return 0 if the operation succeeds, otherwise return -1.
pub fn sys_rename(old: *const c_char, new: *const c_char) -> c_int {
    syscall_body!(sys_rename, {
        let old_orig = char_ptr_to_str(old)?;
        let new_orig = char_ptr_to_str(new)?;
        let old_path = redirect_path(old_orig);
        let new_path = redirect_path(new_orig);
        debug!("sys_rename <= old: {old_path:?}, new: {new_path:?}");
        axfs::api::rename(&old_path, &new_path)?;
        Ok(0)
    })
}

/// Create a directory
///
/// Return 0 if the operation succeeds, otherwise return -1.
pub fn sys_mkdir(name: *const c_char, mode: u32) -> c_int {
    syscall_body!(sys_mkdir, {
        let orig_path = char_ptr_to_str(name)?;
        let path = redirect_path(orig_path);
        debug!("sys_mkdir <= path: {path:?}, mode: {mode:#o}");
        axfs::api::create_dir(&path)?;
        Ok(0)
    })
}

/// Remove a file
///
/// Return 0 if the operation succeeds, otherwise return -1.
pub fn sys_unlink(name: *const c_char) -> c_int {
    syscall_body!(sys_unlink, {
        let orig_path = char_ptr_to_str(name)?;
        let path = redirect_path(orig_path);
        debug!("sys_unlink <= path: {path:?}");
        axfs::api::remove_file(&path)?;
        Ok(0)
    })
}

/// Read directory entries
/// 
/// Returns the number of bytes read, or -1 on error
pub fn sys_getdents64(fd: c_int, buf: *mut u8, count: usize) -> isize {
    debug!("sys_getdents64 <= fd: {fd}, count: {count}");
    syscall_body!(sys_getdents64, {
        if buf.is_null() {
            return Err(LinuxError::EINVAL);
        }
        
        let dir = Dir::from_fd(fd)?;
        let mut dir_inner = dir.inner.lock();
        
        // Read directory entries from axfs
        const MAX_ENTRIES: usize = 32;
        let mut entries: Vec<axfs::fops::DirEntry> = (0..MAX_ENTRIES)
            .map(|_| axfs::fops::DirEntry::default())
            .collect();
        let n = dir_inner.read_dir(&mut entries)?;
        
        if n == 0 {
            return Ok(0); // No more entries
        }
        
        // Convert to Linux dirent64 format
        let mut written = 0usize;
        let buf_ptr = buf as *mut u8;
        
        for i in 0..n {
            let entry = &entries[i];
            let name = entry.name_as_bytes();
            let name_len = name.len();
            let name_str = core::str::from_utf8(name).unwrap_or("<invalid>");
            let entry_type = entry.entry_type();
            
            debug!("sys_getdents64: entry[{i}] name={name_str:?}, type={entry_type:?}");
            
            // Calculate aligned record length
            let rec_len = (19 + name_len + 1 + 7) & !7; // Align to 8 bytes
            
            if written + rec_len > count {
                // Not enough space for this entry
                break;
            }
            
            // Create dirent64 structure
            // struct linux_dirent64 {
            //     u64 d_ino;
            //     i64 d_off;
            //     u16 d_reclen;
            //     u8 d_type;
            //     char d_name[];
            // }
            
            unsafe {
                let dirent_ptr = buf_ptr.add(written);
                
                // d_ino (u64)
                ptr::write_unaligned(dirent_ptr as *mut u64, i as u64 + 1);
                
                // d_off (i64) - offset to next entry
                ptr::write_unaligned(dirent_ptr.add(8) as *mut i64, (written + rec_len) as i64);
                
                // d_reclen (u16)
                ptr::write_unaligned(dirent_ptr.add(16) as *mut u16, rec_len as u16);
                
                // d_type (u8)
                let dtype = match entry_type {
                    axfs::fops::FileType::File => 8,      // DT_REG
                    axfs::fops::FileType::Dir => 4,       // DT_DIR
                    axfs::fops::FileType::SymLink => 10,  // DT_LNK
                    _ => 0,                                 // DT_UNKNOWN
                };
                debug!("sys_getdents64: writing d_type={dtype} for {name_str:?}");
                ptr::write(dirent_ptr.add(18), dtype);
                
                // d_name (null-terminated string)
                ptr::copy_nonoverlapping(name.as_ptr(), dirent_ptr.add(19), name_len);
                ptr::write(dirent_ptr.add(19 + name_len), 0u8); // null terminator
            }
            
            written += rec_len;
        }
        
        Ok(written as isize)
    })
}

/// Remove an empty directory
///
/// Return 0 if successful, -1 on error
pub fn sys_rmdir(path: *const c_char) -> c_int {
    let path = char_ptr_to_str(path);
    debug!("sys_rmdir <= {path:?}");
    syscall_body!(sys_rmdir, {
        let orig_path = path?;
        let redirected = redirect_path(orig_path);
        axfs::api::remove_dir(&redirected)?;
        Ok(0)
    })
}

/// Recursively remove a directory and all its contents
///
/// This is a helper function for implementing remove_dir_all
fn remove_dir_all_impl(path: &str) -> LinuxResult {
    // Read all entries in the directory
    let mut options = OpenOptions::new();
    options.read(true);
    let dir = axfs::fops::Directory::open_dir(path, &options)
        .map_err(|e| LinuxError::from(e))?;
    
    const MAX_ENTRIES: usize = 64;
    let mut entries: Vec<axfs::fops::DirEntry> = (0..MAX_ENTRIES)
        .map(|_| axfs::fops::DirEntry::default())
        .collect();
    
    // Collect all entry names first to avoid holding the directory lock
    let mut all_entries = Vec::new();
    let mut dir_wrapper = Dir::new(dir, path.to_string());
    
    loop {
        let n = dir_wrapper.inner.lock().read_dir(&mut entries)
            .map_err(|e| LinuxError::from(e))?;
        
        if n == 0 {
            break;
        }
        
        for i in 0..n {
            let entry = &entries[i];
            let name = entry.name_as_bytes();
            let name_str = core::str::from_utf8(name).unwrap_or("");
            
            // Skip . and ..
            if name_str == "." || name_str == ".." {
                continue;
            }
            
            let entry_type = entry.entry_type();
            all_entries.push((name_str.to_string(), entry_type));
        }
    }
    
    // Drop the directory before processing entries
    drop(dir_wrapper);
    
    // Process each entry
    for (name, entry_type) in all_entries {
        let entry_path = alloc::format!("{}/{}", path, name);
        
        match entry_type {
            axfs::fops::FileType::Dir => {
                // Recursively remove subdirectory
                remove_dir_all_impl(&entry_path)?;
            }
            _ => {
                // Remove file
                axfs::api::remove_file(&entry_path)
                    .map_err(|e| LinuxError::from(e))?;
            }
        }
    }
    
    // Finally, remove the now-empty directory
    axfs::api::remove_dir(path)
        .map_err(|e| LinuxError::from(e))?;
    
    Ok(())
}

/// Remove a directory and all its contents recursively
///
/// Return 0 if successful, -1 on error
pub fn sys_remove_dir_all(path: *const c_char) -> c_int {
    let path = char_ptr_to_str(path);
    debug!("sys_remove_dir_all <= {path:?}");
    syscall_body!(sys_remove_dir_all, {
        let orig_path = path?;
        let redirected = redirect_path(orig_path);
        remove_dir_all_impl(&redirected)?;
        Ok(0)
    })
}
