use arceos_api::modules::axlog::info;
use arceos_posix_api::ctypes::timespec;

#[cfg(feature = "sync")]
use core::ptr;
#[cfg(feature = "sync")]
use linux_raw_sys::general::{FUTEX_WAIT, FUTEX_WAKE, FUTEX_WAIT_BITSET, FUTEX_WAKE_BITSET};

/*
    sys_futex_wait and sys_futex_wake must exist in a hermit target
    regardless of whether feature "sync" is enabled.

    This is because the Rust std library for hermit target now has
    futex symbols as dependencies. 
    
    When you build with -Zbuild-std=std,panic_abort, you're compiling
    the Rust standard library from source for the hermit target.
    The std library includes synchronization primitives (like Mutex,
    RwLock, Condvar, etc.) that use futex under the hood on hermit.

    Even though your simple helloworld program doesn't use any of these
    primitives directly, the std library code itself references these
    sys_futex_* symbols in its hermit platform implementation.
    The linker requires all symbols to be defined, even if they're
    never called at runtime.
*/

#[cfg(feature = "sync")]
#[unsafe(no_mangle)]
pub fn sys_futex_wait(
    address: *mut u32,
    expected: u32,
    timeout: *const timespec,
    _flags: u32,
) -> i32 {
    info!("called sys_futex_wait");
    sys_futex(address,
        FUTEX_WAIT,
        expected,
        timeout,
        ptr::null(),
        0
    ) as i32
}

#[cfg(not(feature = "sync"))]
#[unsafe(no_mangle)]
pub fn sys_futex_wait(
    _address: *mut u32,
    _expected: u32,
    _timeout: *const timespec,
    _flags: u32,
) -> i32 {
    info!("called sys_futex_wait (stub - sync feature not enabled)");
    // Return success immediately - futex not needed without multithreading
    0
}

#[cfg(feature = "sync")]
#[unsafe(no_mangle)]
pub fn sys_futex_wake(address: *mut u32, count: i32) -> i32 {
    info!("called sys_futex_wake");
    sys_futex(address,
        FUTEX_WAKE,
        count as u32,
        ptr::null(),
        ptr::null(),
        0
    ) as i32
}

#[cfg(not(feature = "sync"))]
#[unsafe(no_mangle)]
pub fn sys_futex_wake(_address: *mut u32, _count: i32) -> i32 {
    info!("called sys_futex_wake (stub - sync feature not enabled)");
    // Return 0 - no waiters to wake without multithreading
    0
}

// sys_futex_wait_bitset and sys_futex_wake_bitset are not must for hermit target,
// but we provide them here for completeness.

#[cfg(feature = "sync")]
#[unsafe(no_mangle)]
pub fn sys_futex_wait_bitset(
    address: *mut u32,
    expected: u32,
    timeout: *const timespec,
    bitset: u32,
    _flags: u32,
) -> i32 {
    info!("called sys_futex_wait_bitset");
    sys_futex(
        address,
        FUTEX_WAIT_BITSET,
        expected,
        timeout,
        ptr::null(),
        bitset,
    ) as i32
}

#[cfg(feature = "sync")]
#[unsafe(no_mangle)]
pub fn sys_futex_wake_bitset(address: *mut u32, count: i32, bitset: u32) -> i32 {
    info!("called sys_futex_wake_bitset");
    sys_futex(
        address,
        FUTEX_WAKE_BITSET,
        count as u32,
        ptr::null(),
        ptr::null(),
        bitset,
    ) as i32
}

#[cfg(feature = "sync")]
// NOTE: This is an internal helper function, not exported to avoid symbol conflict
// with the sys_futex in arceos_posix_api
fn sys_futex(
    uaddr: *const u32,
    futex_op: u32,
    value: u32,
    timeout: *const timespec,
    uaddr2: *const u32,
    value3: u32,
) -> isize {
    info!("called sys_futex");
    match arceos_posix_api::sys_futex(uaddr, futex_op, value, timeout, uaddr2, value3) {
        Ok(ret) => ret,
        Err(e) => -(e.code() as isize),
    }
}
