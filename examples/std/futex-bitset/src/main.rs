#[cfg(target_os = "hermit")]
use arceos_rust as _;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

// Import the syscall functions we need to test
// These are implemented in ulib/arceos-rust/lib/src/syscall.rs
// We declare them as extern since they're exported with #[no_mangle]
unsafe extern "C" {
    fn sys_futex_wait_bitset(address: *mut u32, expected: u32, timeout: *const libc::timespec, flags: u32) -> i32;
    fn sys_futex_wake_bitset(address: *mut u32, count: i32, bitset: u32) -> i32;
}

fn main() {
    println!("\n=== Bitset Futex Test Program ===\n");
    
    // Run all tests
    test_basic_futex();
    println!();
    
    test_multiple_waiters();
    println!();
    
    test_bitset_overlap();
    println!();
    
    // Skip timeout test as it requires working timer interrupts
    // test_futex_timeout();
    // println!();
    
    println!("=== All Tests Completed ===\n");
}

/// Test 1: Basic futex_wait_bitset and futex_wake_bitset with FUTEX_BITSET_MATCH_ANY
fn test_basic_futex() {
    println!("[Test 1] Basic futex_wait_bitset and futex_wake_bitset with MATCH_ANY");
    
    const FUTEX_BITSET_MATCH_ANY: u32 = 0xffffffff;
    
    // Shared futex variable
    let futex = Arc::new(AtomicU32::new(0));
    let futex_clone = Arc::clone(&futex);
    
    // Spawn waiter thread
    let waiter = thread::spawn(move || {
        println!("  [Waiter] Waiting on futex with bitset 0xffffffff...");
        
        let futex_ptr = futex_clone.as_ptr();
        let result = unsafe {
            sys_futex_wait_bitset(futex_ptr as *mut u32, 0, std::ptr::null(), FUTEX_BITSET_MATCH_ANY)
        };
        
        println!("  [Waiter] Woken up! Result: {}", result);
        result
    });
    
    // Give waiter time to start waiting - use yield instead of sleep
    for _ in 0..10 {
        thread::yield_now();
    }
    
    // Wake the waiter
    println!("  [Main] Waking up one waiter with bitset 0xffffffff...");
    let wake_result = unsafe {
        sys_futex_wake_bitset(futex.as_ptr() as *mut u32, 1, FUTEX_BITSET_MATCH_ANY)
    };
    println!("  [Main] Wake result: {} (threads woken)", wake_result);
    
    // Wait for waiter to finish
    let result = waiter.join().unwrap();
    
    if result == 0 && wake_result == 1 {
        println!("  ✓ Test 1 passed");
    } else {
        println!("  ✗ Test 1 failed: wait_result={}, wake_result={}", result, wake_result);
    }
}

/// Test 2: Futex with multiple waiters using different bitsets (selective wake)
fn test_multiple_waiters() {
    println!("[Test 2] Selective wake with different bitsets");
    
    const BITSET_A: u32 = 0b0001;  // Bit 0
    const BITSET_B: u32 = 0b0010;  // Bit 1
    const BITSET_C: u32 = 0b0100;  // Bit 2
    
    let futex = Arc::new(AtomicU32::new(0));
    let mut handles = vec![];
    
    // Spawn waiters with different bitsets
    for (i, bitset) in [(0, BITSET_A), (1, BITSET_B), (2, BITSET_C)].iter() {
        let futex_clone = Arc::clone(&futex);
        let i = *i;
        let bitset = *bitset;
        let handle = thread::spawn(move || {
            println!("  [Waiter {}] Waiting on futex with bitset 0b{:04b}...", i, bitset);
            
            let futex_ptr = futex_clone.as_ptr();
            let result = unsafe {
                sys_futex_wait_bitset(futex_ptr as *mut u32, 0, std::ptr::null(), bitset)
            };
            
            println!("  [Waiter {}] Woken up! Result: {}", i, result);
            (i, result)
        });
        handles.push(handle);
    }
    
    // Give waiters time to start waiting
    for _ in 0..20 {
        thread::yield_now();
    }
    
    // Wake only waiters with BITSET_A (should wake waiter 0)
    println!("  [Main] Waking waiters with bitset 0b{:04b}...", BITSET_A);
    let wake_result_a = unsafe {
        sys_futex_wake_bitset(futex.as_ptr() as *mut u32, 10, BITSET_A)
    };
    println!("  [Main] Wake result: {} (threads woken)", wake_result_a);
    
    for _ in 0..10 {
        thread::yield_now();
    }
    
    // Wake waiters with BITSET_B | BITSET_C (should wake waiters 1 and 2)
    println!("  [Main] Waking waiters with bitset 0b{:04b}...", BITSET_B | BITSET_C);
    let wake_result_bc = unsafe {
        sys_futex_wake_bitset(futex.as_ptr() as *mut u32, 10, BITSET_B | BITSET_C)
    };
    println!("  [Main] Wake result: {} (threads woken)", wake_result_bc);
    
    // Wait for all waiters to finish
    let mut all_passed = true;
    for handle in handles {
        let (_, result) = handle.join().unwrap();
        if result != 0 {
            all_passed = false;
        }
    }
    
    if all_passed && wake_result_a == 1 && wake_result_bc == 2 {
        println!("  ✓ Test 2 passed (selective wake worked correctly)");
    } else {
        println!("  ✗ Test 2 failed: wake_a={}, wake_bc={}", wake_result_a, wake_result_bc);
    }
}

/// Test 3: Bitset overlap test (wake with overlapping bits)
fn test_bitset_overlap() {
    println!("[Test 3] Bitset overlap - wake waiters with overlapping bits");
    
    const BITSET_1: u32 = 0b0011;  // Bits 0,1
    const BITSET_2: u32 = 0b0110;  // Bits 1,2
    const BITSET_3: u32 = 0b1100;  // Bits 2,3
    const WAKE_BITSET: u32 = 0b0100;  // Bit 2 - should wake waiters 1 and 2
    
    let futex = Arc::new(AtomicU32::new(0));
    let mut handles = vec![];
    
    // Spawn waiters with different overlapping bitsets
    for (i, bitset) in [(0, BITSET_1), (1, BITSET_2), (2, BITSET_3)].iter() {
        let futex_clone = Arc::clone(&futex);
        let i = *i;
        let bitset = *bitset;
        let handle = thread::spawn(move || {
            println!("  [Waiter {}] Waiting with bitset 0b{:04b}...", i, bitset);
            
            let futex_ptr = futex_clone.as_ptr();
            let result = unsafe {
                sys_futex_wait_bitset(futex_ptr as *mut u32, 0, std::ptr::null(), bitset)
            };
            
            println!("  [Waiter {}] Result: {}", i, result);
            (i, result)
        });
        handles.push(handle);
    }
    
    // Give waiters time to start waiting
    for _ in 0..20 {
        thread::yield_now();
    }
    
    // Wake with bitset that overlaps with BITSET_2 and BITSET_3
    println!("  [Main] Waking with bitset 0b{:04b} (should wake waiters 1 and 2)...", WAKE_BITSET);
    let wake_result = unsafe {
        sys_futex_wake_bitset(futex.as_ptr() as *mut u32, 10, WAKE_BITSET)
    };
    println!("  [Main] Wake result: {} (threads woken)", wake_result);
    
    for _ in 0..10 {
        thread::yield_now();
    }
    
    // Wake remaining waiters
    println!("  [Main] Cleaning up - waking remaining waiters...");
    let cleanup_result = unsafe {
        sys_futex_wake_bitset(futex.as_ptr() as *mut u32, 10, 0xffffffff)
    };
    println!("  [Main] Cleanup wake result: {}", cleanup_result);
    
    // Wait for all waiters to finish
    let mut woken_waiters = Vec::new();
    for handle in handles {
        let (i, result) = handle.join().unwrap();
        if result == 0 {
            woken_waiters.push(i);
        }
    }
    
    if wake_result == 2 {
        println!("  ✓ Test 3 passed (correctly woke 2 waiters with overlapping bitsets)");
    } else {
        println!("  ✗ Test 3 failed: expected to wake 2, actually woke {}", wake_result);
    }
}

/// Test 4: Futex with timeout
fn test_futex_timeout() {
    println!("[Test 4] Futex with timeout");
    
    const FUTEX_BITSET_MATCH_ANY: u32 = 0xffffffff;
    
    let futex = Arc::new(AtomicU32::new(0));
    let futex_clone = Arc::clone(&futex);
    
    // Spawn waiter with timeout
    let waiter = thread::spawn(move || {
        println!("  [Waiter] Waiting on futex with 500ms timeout...");
        
        // Set timeout to 500ms
        let timeout = libc::timespec {
            tv_sec: 0,
            tv_nsec: 500_000_000, // 500ms
        };
        
        let start = Instant::now();
        let futex_ptr = futex_clone.as_ptr();
        let result = unsafe {
            sys_futex_wait_bitset(futex_ptr as *mut u32, 0, &timeout as *const libc::timespec, FUTEX_BITSET_MATCH_ANY)
        };
        let elapsed = start.elapsed();
        
        println!("  [Waiter] Timeout occurred! Result: {}, Elapsed: {:?}", result, elapsed);
        (result, elapsed)
    });
    
    // Don't wake, let it timeout
    let (result, elapsed) = waiter.join().unwrap();
    
    // ETIMEDOUT is typically -62 or -110 depending on platform
    // Check that it timed out and took approximately the right time
    if result < 0 && elapsed >= Duration::from_millis(450) && elapsed <= Duration::from_millis(650) {
        println!("  ✓ Test 4 passed (timed out as expected)");
    } else {
        println!("  ✗ Test 4 failed: result={}, elapsed={:?}", result, elapsed);
    }
}

/// Test 5: Value mismatch test
#[allow(dead_code)]
fn test_value_mismatch() {
    println!("[Test 5] Futex value mismatch");
    
    const FUTEX_BITSET_MATCH_ANY: u32 = 0xffffffff;
    
    let futex = Arc::new(AtomicU32::new(42));
    
    println!("  [Main] Futex value is 42, attempting to wait expecting 0...");
    
    let futex_ptr = futex.as_ptr();
    let result = unsafe {
        // Expect 0 but actual value is 42 - should return immediately with EAGAIN
        sys_futex_wait_bitset(futex_ptr as *mut u32, 0, std::ptr::null(), FUTEX_BITSET_MATCH_ANY)
    };
    
    // EAGAIN is typically -11
    if result < 0 {
        println!("  [Main] Returned immediately with error: {} (EAGAIN)", result);
        println!("  ✓ Test 5 passed");
    } else {
        println!("  ✗ Test 5 failed: expected error, got {}", result);
    }
}

/// Test 6: Selective wake test
#[allow(dead_code)]
fn test_selective_wake() {
    println!("[Test 6] Selective wake (wake fewer than total waiters)");
    
    const FUTEX_BITSET_MATCH_ANY: u32 = 0xffffffff;
    
    let futex = Arc::new(AtomicU32::new(0));
    let num_waiters = 5;
    let num_to_wake = 2;
    let mut handles = vec![];
    
    // Spawn multiple waiter threads
    for i in 0..num_waiters {
        let futex_clone = Arc::clone(&futex);
        let handle = thread::spawn(move || {
            println!("  [Waiter {}] Waiting on futex...", i);
            
            let futex_ptr = futex_clone.as_ptr();
            let result = unsafe {
                sys_futex_wait_bitset(futex_ptr as *mut u32, 0, std::ptr::null(), FUTEX_BITSET_MATCH_ANY)
            };
            
            println!("  [Waiter {}] Result: {}", i, result);
            result
        });
        handles.push(handle);
    }
    
    // Give waiters time to start waiting
    thread::sleep(Duration::from_millis(200));
    
    // Wake only some waiters
    println!("  [Main] Waking up {} out of {} waiters...", num_to_wake, num_waiters);
    let wake_result = unsafe {
        sys_futex_wake_bitset(futex.as_ptr() as *mut u32, num_to_wake, FUTEX_BITSET_MATCH_ANY)
    };
    println!("  [Main] Wake result: {} (threads woken)", wake_result);
    
    if wake_result == num_to_wake {
        println!("  ✓ Test 6 passed");
    } else {
        println!("  ✗ Test 6 failed: expected to wake {}, actually woke {}", num_to_wake, wake_result);
    }
    
    // Clean up: wake remaining waiters
    thread::sleep(Duration::from_millis(100));
    unsafe {
        sys_futex_wake_bitset(futex.as_ptr() as *mut u32, num_waiters, FUTEX_BITSET_MATCH_ANY);
    }
    
    for handle in handles {
        let _ = handle.join();
    }
}