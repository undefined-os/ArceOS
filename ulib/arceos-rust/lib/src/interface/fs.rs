use core::ffi::{c_char, c_void, c_int};
use arceos_posix_api::ctypes_gen::{mode_t, off_t};
use log::info;

/// Hermit ABI stat structure (different layout from Linux stat)
/// The hermit std library expects this layout when calling sys_stat
#[repr(C)]
#[derive(Debug, Default)]
pub struct HermitStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,  // Note: u64, not u32
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    // 4 bytes padding here for alignment
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atim: HermitTimespec,
    pub st_mtim: HermitTimespec,
    pub st_ctim: HermitTimespec,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct HermitTimespec {
    pub tv_sec: i64,
    pub tv_nsec: i32,
}

//#[cfg(feature = "fs")]
//pub use imp::fs::{sys_fstat, sys_getcwd, sys_lseek, sys_lstat, sys_open, sys_rename, sys_stat};
// TODO: implement these syscalls properly

// add "OK" comment when implemented

// pub fn sys_lseek(fd: c_int, offset: ctypes::off_t, whence: c_int) -> ctypes::off_t;
#[unsafe(no_mangle)]
pub fn sys_lseek(fd: i32, offset: isize, whence: i32) -> isize {
    info!("called sys_lseek");
    arceos_posix_api::sys_lseek(fd as c_int, offset as off_t, whence as c_int) as _
}

// pub unsafe fn sys_stat(path: *const c_char, buf: *mut HermitStat) -> c_int;
// Note: hermit std passes a hermit_abi::stat pointer, we need to convert from Linux stat
#[unsafe(no_mangle)]
pub fn sys_stat(name: *const c_char, hermit_stat: *mut HermitStat) -> i32 {
    info!("called sys_stat");
    unsafe {
        // Call the posix API which returns Linux stat format
        let mut linux_stat = arceos_posix_api::ctypes_gen::stat::default();
        let ret = arceos_posix_api::sys_stat(name, &mut linux_stat);
        if ret != 0 {
            return ret;
        }
        
        // Convert Linux stat to Hermit stat format
        (*hermit_stat) = HermitStat {
            st_dev: linux_stat.st_dev,
            st_ino: linux_stat.st_ino,
            st_nlink: linux_stat.st_nlink as u64,
            st_mode: linux_stat.st_mode,
            st_uid: linux_stat.st_uid,
            st_gid: linux_stat.st_gid,
            st_rdev: linux_stat.st_rdev,
            st_size: linux_stat.st_size,
            st_blksize: linux_stat.st_blksize,
            st_blocks: linux_stat.st_blocks,
            st_atim: HermitTimespec {
                tv_sec: linux_stat.st_atime.tv_sec,
                tv_nsec: linux_stat.st_atime.tv_nsec as i32,
            },
            st_mtim: HermitTimespec {
                tv_sec: linux_stat.st_mtime.tv_sec,
                tv_nsec: linux_stat.st_mtime.tv_nsec as i32,
            },
            st_ctim: HermitTimespec {
                tv_sec: linux_stat.st_ctime.tv_sec,
                tv_nsec: linux_stat.st_ctime.tv_nsec as i32,
            },
        };
        
        0
    }
}

// pub unsafe fn sys_fstat(fd: c_int, buf: *mut HermitStat) -> c_int;
#[unsafe(no_mangle)]
pub fn sys_fstat(fd: i32, hermit_stat: *mut HermitStat) -> i32 {
    info!("called sys_fstat");
    unsafe {
        // Call the posix API which returns Linux stat format
        let mut linux_stat = arceos_posix_api::ctypes_gen::stat::default();
        let ret = arceos_posix_api::sys_fstat(fd as c_int, &mut linux_stat);
        if ret != 0 {
            return ret;
        }
        
        // Convert Linux stat to Hermit stat format
        (*hermit_stat) = HermitStat {
            st_dev: linux_stat.st_dev,
            st_ino: linux_stat.st_ino,
            st_nlink: linux_stat.st_nlink as u64,
            st_mode: linux_stat.st_mode,
            st_uid: linux_stat.st_uid,
            st_gid: linux_stat.st_gid,
            st_rdev: linux_stat.st_rdev,
            st_size: linux_stat.st_size,
            st_blksize: linux_stat.st_blksize,
            st_blocks: linux_stat.st_blocks,
            st_atim: HermitTimespec {
                tv_sec: linux_stat.st_atime.tv_sec,
                tv_nsec: linux_stat.st_atime.tv_nsec as i32,
            },
            st_mtim: HermitTimespec {
                tv_sec: linux_stat.st_mtime.tv_sec,
                tv_nsec: linux_stat.st_mtime.tv_nsec as i32,
            },
            st_ctim: HermitTimespec {
                tv_sec: linux_stat.st_ctime.tv_sec,
                tv_nsec: linux_stat.st_ctime.tv_nsec as i32,
            },
        };
        
        0
    }
}

// pub unsafe fn sys_lstat(path: *const c_char, buf: *mut HermitStat) -> i32;
#[unsafe(no_mangle)]
pub fn sys_lstat(name: *const c_char, hermit_stat: *mut HermitStat) -> i32 {
    info!("called sys_lstat");
    unsafe {
        // Call the posix API which returns Linux stat format
        let mut linux_stat = arceos_posix_api::ctypes_gen::stat::default();
        let ret = arceos_posix_api::sys_lstat(name, &mut linux_stat);
        if ret != 0 {
            return ret as i32;
        }
        
        // Convert Linux stat to Hermit stat format
        (*hermit_stat) = HermitStat {
            st_dev: linux_stat.st_dev,
            st_ino: linux_stat.st_ino,
            st_nlink: linux_stat.st_nlink as u64,
            st_mode: linux_stat.st_mode,
            st_uid: linux_stat.st_uid,
            st_gid: linux_stat.st_gid,
            st_rdev: linux_stat.st_rdev,
            st_size: linux_stat.st_size,
            st_blksize: linux_stat.st_blksize,
            st_blocks: linux_stat.st_blocks,
            st_atim: HermitTimespec {
                tv_sec: linux_stat.st_atime.tv_sec,
                tv_nsec: linux_stat.st_atime.tv_nsec as i32,
            },
            st_mtim: HermitTimespec {
                tv_sec: linux_stat.st_mtime.tv_sec,
                tv_nsec: linux_stat.st_mtime.tv_nsec as i32,
            },
            st_ctim: HermitTimespec {
                tv_sec: linux_stat.st_ctime.tv_sec,
                tv_nsec: linux_stat.st_ctime.tv_nsec as i32,
            },
        };
        
        0
    }
}

// pub fn sys_open(filename: *const c_char, flags: c_int, mode: ctypes::mode_t) -> c_int;
#[unsafe(no_mangle)]
pub fn sys_open(name: *const c_char, flags: i32, mode: i32) -> i32 {
    info!("called sys_open");
    arceos_posix_api::sys_open(name, flags as c_int, mode as mode_t) as _
}

/// 'mkdir' attempts to create a directory,
/// it returns 0 on success and -1 on error
#[unsafe(no_mangle)]
pub fn sys_mkdir(name: *const i8, mode: u32) -> i32 {
    info!("called sys_mkdir");
    arceos_posix_api::sys_mkdir(name, mode as _) as _
}

/// delete the file it refers to `name`
#[unsafe(no_mangle)]
pub fn sys_unlink(name: *const c_char) -> i32 {
    info!("called sys_unlink");
    arceos_posix_api::sys_unlink(name) as _
}