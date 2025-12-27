use arceos_posix_api::ctypes::iovec;
use core::ffi::c_void;
use log::info;
use crate::err;
use axerrno::LinuxError;

#[cfg(feature = "fd")]
use arceos_posix_api::sys_read as posix_read;

// Define types that might not be in ctypes_gen
pub type nfds_t = u64;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct pollfd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

#[repr(C)]
pub struct dirent64 {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

#[unsafe(no_mangle)]
pub fn sys_write(fd: i32, buf: *const u8, count: usize) -> isize {
    info!("called sys_write");
    arceos_posix_api::sys_write(fd, buf as _, count)
}

#[unsafe(no_mangle)]
pub fn sys_writev(fd: i32, iov: *const iovec, iovcnt: usize) -> isize {
    info!("called sys_writev");
    unsafe { arceos_posix_api::sys_writev(fd, iov, iovcnt as _) }
}

#[cfg(feature = "fd")]
#[unsafe(no_mangle)]
pub fn sys_close(fd: i32) -> i32 {
    info!("called sys_close");
    arceos_posix_api::sys_close(fd) as _
}

/// read from a file descriptor
///
/// read() attempts to read `len` bytes of data from the object
/// referenced by the descriptor `fd` into the buffer pointed
/// to by `buf`.
#[unsafe(no_mangle)]
//pub fn sys_read(fd: c_int, buf: *mut c_void, count: usize) -> ctypes::ssize_t;
pub fn sys_read(fd: i32, buf: *mut u8, len: usize) -> isize {
    info!("called sys_read");
    arceos_posix_api::sys_read(fd, buf as *mut c_void, len) as _
}

/// `read()` attempts to read `nbyte` of data to the object referenced by the
/// descriptor `fd` from a buffer. `readv()` performs the same
/// action, but scatters the input data from the `iovcnt` buffers specified by the
/// members of the iov array: `iov[0], iov[1], ..., iov[iovcnt-1]`.
///
/// ```custom
/// struct iovec {
///     char   *iov_base;  /* Base address. */
///     size_t iov_len;    /* Length. */
/// };
///
/// Each `iovec` entry specifies the base address and length of an area in memory from
/// which data should be written.  `readv()` will always fill an completely
/// before proceeding to the next.
#[unsafe(no_mangle)]
pub fn sys_readv(fd: i32, iov: *const iovec, iovcnt: usize) -> isize {
    info!("called sys_readv with fd={}, iovcnt={}", fd, iovcnt);
    
    if iov.is_null() || iovcnt == 0 || iovcnt > 1024 {
        return err(LinuxError::EINVAL) as isize;
    }
    
    let iovs = unsafe { core::slice::from_raw_parts(iov, iovcnt) };
    let mut total_read: isize = 0;
    
    for iovec in iovs.iter() {
        if iovec.iov_len == 0 {
            continue;
        }
        if iovec.iov_base.is_null() {
            return err(LinuxError::EFAULT) as isize;
        }
        
        let result = sys_read(fd, iovec.iov_base as *mut u8, iovec.iov_len);
        if result < 0 {
            if total_read == 0 {
                return result;
            } else {
                break;
            }
        }
        
        total_read += result;
        
        // If we read less than requested, stop here
        if result < iovec.iov_len as isize {
            break;
        }
    }
    
    total_read
}

/// The unix-like `poll` waits for one of a set of file descriptors
/// to become ready to perform I/O. The set of file descriptors to be
/// monitored is specified in the `fds` argument, which is an array
/// of structures of `pollfd`.
#[unsafe(no_mangle)]
pub fn sys_poll(fds: *mut pollfd, nfds: nfds_t, timeout: i32) -> i32 {
    info!("called sys_poll with nfds={}, timeout={}", nfds, timeout);
    
    // For now, implement a simple stub that returns immediately
    // indicating all fds are ready for I/O
    if !fds.is_null() && nfds > 0 {
        let fds_slice = unsafe { core::slice::from_raw_parts_mut(fds, nfds as usize) };
        for pollfd in fds_slice.iter_mut() {
            // Set revents to match requested events (indicating ready)
            pollfd.revents = pollfd.events;
        }
        nfds as i32
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub fn sys_fcntl(fd: i32, cmd: i32, arg: i32) -> i32 {
    info!("called sys_fcntl with fd={}, cmd={}, arg={}", fd, cmd, arg);
    
    #[cfg(feature = "fd")]
    {
        arceos_posix_api::sys_fcntl(fd, cmd, arg as usize) as _
    }
    
    #[cfg(not(feature = "fd"))]
    {
        // Stub implementation - just return success for now
        0
    }
}

/// `getdents64` reads directory entries from the directory referenced
/// by the file descriptor `fd` into the buffer pointed to by `buf`.
#[unsafe(no_mangle)]
pub fn sys_getdents64(fd: i32, dirp: *mut dirent64, count: usize) -> i64 {
    info!("called sys_getdents64 with fd={}, count={}", fd, count);
    
    if dirp.is_null() {
        return err(LinuxError::EFAULT) as i64;
    }
    
    #[cfg(feature = "fs")]
    {
        arceos_posix_api::sys_getdents64(fd, dirp as *mut u8, count) as i64
    }
    
    #[cfg(not(feature = "fs"))]
    {
        err(LinuxError::ENOSYS) as i64
    }
}

/// `rmdir` removes an empty directory.
#[unsafe(no_mangle)]
pub fn sys_rmdir(path: *const i8) -> i32 {
    info!("called sys_rmdir with path={:?}", unsafe {
        core::ffi::CStr::from_ptr(path)
    });
    
    #[cfg(feature = "fs")]
    {
        arceos_posix_api::sys_rmdir(path)
    }
    
    #[cfg(not(feature = "fs"))]
    {
        err(LinuxError::ENOSYS)
    }
}

/// `remove_dir_all` recursively removes a directory and all its contents.
/// This is a custom syscall to support Rust's std::fs::remove_dir_all.
#[unsafe(no_mangle)]
pub fn sys_remove_dir_all(path: *const i8) -> i32 {
    info!("called sys_remove_dir_all with path={:?}", unsafe {
        core::ffi::CStr::from_ptr(path)
    });
    
    #[cfg(feature = "fs")]
    {
        arceos_posix_api::sys_remove_dir_all(path)
    }
    
    #[cfg(not(feature = "fs"))]
    {
        err(LinuxError::ENOSYS)
    }
}