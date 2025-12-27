use core::alloc::Layout;
use log::{error, info};
#[cfg(feature = "paging")]
use memory_addr::VirtAddr;
#[cfg(feature = "paging")]
use axhal::paging::MappingFlags;

#[unsafe(no_mangle)]
pub fn sys_malloc(size: usize, align: usize) -> *mut u8 {
    info!("called sys_malloc with size {} and align {}", size, align);
    if let Ok(layout) = Layout::from_size_align(size, align) {
        unsafe { alloc::alloc::alloc(layout) }
    } else {
        core::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub fn sys_free(ptr: *mut u8, size: usize, align: usize) {
    info!("called sys_free");
    if let Ok(layout) = Layout::from_size_align(size, align) {
        unsafe { alloc::alloc::dealloc(ptr, layout) }
    } else {
        error!(
            "sys_free called with invalid layout: size {}, align {}",
            size, align
        );
    }
}

#[unsafe(no_mangle)]
pub fn sys_realloc(ptr: *mut u8, size: usize, align: usize, new_size: usize) -> *mut u8 {
    info!("called sys_realloc");
    if let Ok(layout) = Layout::from_size_align(size, align) {
        unsafe { alloc::alloc::realloc(ptr, layout, new_size) }
    } else {
        core::ptr::null_mut()
    }
}

/// Creates a new virtual memory mapping of the `size` specified with
/// protection bits specified in `prot_flags`.
#[unsafe(no_mangle)]
pub fn sys_mmap(size: usize, prot_flags: u32, ret: &mut *mut u8) -> i32 {
    info!("called sys_mmap");
    let layout = match Layout::from_size_align(size, 0x1000) {
        Ok(layout) => layout,
        Err(_) => return -1,
    };
    let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        return -1;
    }
    *ret = ptr;
    0
}

/// Unmaps memory at the specified `ptr` for `size` bytes.
#[unsafe(no_mangle)]
pub fn sys_munmap(ptr: *mut u8, size: usize) -> i32 {
    info!("called sys_munmap");
    let layout = match Layout::from_size_align(size, 0x1000) {
        Ok(layout) => layout,
        Err(_) => return -1,
    };
    unsafe { alloc::alloc::dealloc(ptr, layout) };
    0
}
/// Configures the protections associated with a region of virtual memory
/// starting at `ptr` and going to `size`.
///
/// Returns 0 on success and an error code on failure.
#[unsafe(no_mangle)]
pub fn sys_mprotect(ptr: *mut u8, size: usize, prot_flags: u32) -> i32 {
    info!("called sys_mprotect with ptr={:p}, size={:#x}, prot_flags={:#x}", ptr, size, prot_flags);
    
    #[cfg(feature = "paging")]
    {
        // Convert POSIX protection flags to ArceOS MappingFlags
        let mut flags = MappingFlags::USER;
        
        // PROT_READ = 0x1
        if prot_flags & 0x1 != 0 {
            flags |= MappingFlags::READ;
        }
        // PROT_WRITE = 0x2
        if prot_flags & 0x2 != 0 {
            flags |= MappingFlags::WRITE;
        }
        // PROT_EXEC = 0x4
        if prot_flags & 0x4 != 0 {
            flags |= MappingFlags::EXECUTE;
        }
        
        let vaddr = VirtAddr::from(ptr as usize);
        
        // Try to update protections in the kernel address space
        match axmm::kernel_aspace().lock().protect(vaddr, size, flags) {
            Ok(_) => {
                info!("sys_mprotect succeeded");
                0
            }
            Err(e) => {
                error!("sys_mprotect failed: {:?}", e);
                -1
            }
        }
    }
    
    #[cfg(not(feature = "paging"))]
    {
        error!("sys_mprotect not supported in this configuration");
        -1
    }
}
