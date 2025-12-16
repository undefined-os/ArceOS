use alloc::sync::Arc;
use axsync::Mutex;
use axerrno::{LinuxError, LinuxResult};
use crate::ctypes::timespec;
use linux_raw_sys::general::{
    FUTEX_CMD_MASK, FUTEX_CMP_REQUEUE, FUTEX_REQUEUE, FUTEX_WAIT,
    FUTEX_WAIT_BITSET, FUTEX_WAKE, FUTEX_WAKE_BITSET,
};
use axtask::WaitQueue;
use alloc::collections::BTreeMap;

fn timespec_to_duration(ts: timespec) -> core::time::Duration {
    core::time::Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// Futex queue entry that supports bitset-based waking
struct FutexBitsetQueue {
    /// Map of bitset to wait queues for bitset-based operations
    bitset_queues: BTreeMap<u32, Arc<WaitQueue>>,
}

impl FutexBitsetQueue {
    fn new() -> Self {
        Self {
            bitset_queues: BTreeMap::new(),
        }
    }

    fn get_bitset_queue(&mut self, bitset: u32) -> Arc<WaitQueue> {
        self.bitset_queues
            .entry(bitset)
            .or_insert_with(|| Arc::new(WaitQueue::new()))
            .clone()
    }

    fn wake_bitset(&self, max_count: u32, wake_bitset: u32) -> isize {
        let mut total_woken = 0;
        let mut remaining = max_count;

        // Wake waiters whose bitset overlaps with wake_bitset
        for (&wait_bitset, queue) in &self.bitset_queues {
            if remaining == 0 {
                break;
            }
            // Check if bitsets have any common bits
            if wait_bitset & wake_bitset != 0 {
                for _ in 0..remaining {
                    if !queue.notify_one(false) {
                        break;
                    }
                    total_woken += 1;
                    remaining -= 1;
                }
            }
        }

        total_woken
    }
}

fn new_futex() -> Arc<WaitQueue> {
    Arc::new(WaitQueue::new())
}

fn new_futex_bitset() -> FutexBitsetQueue {
    FutexBitsetQueue::new()
}

// Since unikernel has no concepts of process and thread, we use a global futex table here.
static FUTEX_TABLE: Mutex<BTreeMap<usize, Arc<WaitQueue>>> = Mutex::new(BTreeMap::new());
static FUTEX_BITSET_TABLE: Mutex<BTreeMap<usize, FutexBitsetQueue>> = Mutex::new(BTreeMap::new());

/// Futex syscall
#[unsafe(no_mangle)]
pub fn sys_futex(
    uaddr: *const u32,
    futex_op: u32,
    value: u32,
    timeout: *const timespec,
    uaddr2: *const u32,
    value3: u32,
) -> LinuxResult<isize> {
    debug!("syscall futex");

    let futex_table = &FUTEX_TABLE;
    let addr = uaddr as usize;
    let command = futex_op & (FUTEX_CMD_MASK as u32);
    match command {
        FUTEX_WAIT => {
            // Get or create the wait queue first
            let wq = futex_table
                .lock()
                .entry(addr)
                .or_insert_with(new_futex)
                .clone();
            
            // Critical: Check the futex value AFTER getting the wait queue
            // This prevents the race where another thread wakes us before we start waiting
            if unsafe { *uaddr } != value {
                return Err(LinuxError::EAGAIN);
            }

            if !timeout.is_null() {
                wq.wait_timeout(timespec_to_duration(unsafe { *timeout }));
            } else {
                wq.wait();
            }

            Ok(0)
        }
        FUTEX_WAKE => {
            let wq = futex_table.lock().get(&addr).cloned();
            let mut count = 0;
            if let Some(wq) = wq {
                for _ in 0..value {
                    if !wq.notify_one(false) {
                        break;
                    }
                    count += 1;
                }
            }
            axtask::yield_now();
            Ok(count)
        }
        FUTEX_REQUEUE | FUTEX_CMP_REQUEUE => {
            if command == FUTEX_CMP_REQUEUE && unsafe { *uaddr } != value3 {
                return Err(LinuxError::EAGAIN);
            }
            let value2 = timeout as usize as u32;

            let mut futex_table = futex_table.lock();
            let wq = futex_table.get(&addr).cloned();
            let wq2 = futex_table
                .entry(uaddr2 as usize)
                .or_insert_with(new_futex)
                .clone();
            drop(futex_table);

            let mut count = 0;
            if let Some(wq) = wq {
                for _ in 0..value {
                    if !wq.notify_one(false) {
                        break;
                    }
                    count += 1;
                }
                count += wq.requeue(value2 as usize, &wq2) as isize;
            }
            Ok(count)
        }
        FUTEX_WAIT_BITSET => {
            let bitset = value3;
            
            // Validate bitset (must not be 0)
            if bitset == 0 {
                return Err(LinuxError::EINVAL);
            }
            
            // Get or create the futex queue and get the appropriate wait queue for this bitset
            let wq = {
                let mut bitset_table = FUTEX_BITSET_TABLE.lock();
                let futex_queue = bitset_table.entry(addr).or_insert_with(new_futex_bitset);
                futex_queue.get_bitset_queue(bitset)
            };
            
            // Critical: Check the futex value AFTER getting the wait queue
            // This prevents the race where another thread wakes us before we start waiting
            if unsafe { *uaddr } != value {
                return Err(LinuxError::EAGAIN);
            }

            if !timeout.is_null() {
                wq.wait_timeout(timespec_to_duration(unsafe { *timeout }));
            } else {
                wq.wait();
            }
            Ok(0)
        }
        FUTEX_WAKE_BITSET => {
            let bitset = value3;
            
            // Validate bitset (must not be 0)
            if bitset == 0 {
                return Err(LinuxError::EINVAL);
            }
            
            let count = {
                let bitset_table = FUTEX_BITSET_TABLE.lock();
                if let Some(futex_queue) = bitset_table.get(&addr) {
                    futex_queue.wake_bitset(value, bitset)
                } else {
                    0
                }
            };
            
            axtask::yield_now();
            Ok(count)
        }
        _ => {
            warn!("[sys_futex] unknown command: {}", command);
            Err(LinuxError::ENOSYS)
        }
    }
}