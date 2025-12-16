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
    fn sys_futex_wait(address: *mut u32, expected: u32, timeout: *const libc::timespec, flags: u32) -> i32;
    fn sys_futex_wake(address: *mut u32, count: i32) -> i32;
}

fn main() {
    println!("\n=== Futex Test Program ===\n");
    
    // Run all tests
    test_basic_futex();
    println!();
    
    test_multiple_waiters();
    println!();
    
    // Skip timeout test as it requires working timer interrupts
    // test_futex_timeout();
    // println!();
    
    println!("=== All Tests Completed ===\n");
}

/// Test 1: Basic futex_wait and futex_wake with single thread
fn test_basic_futex() {
    println!("[Test 1] Basic futex_wait and futex_wake");
    
    // Shared futex variable
    let futex = Arc::new(AtomicU32::new(0));
    let futex_clone = Arc::clone(&futex);
    
    // Spawn waiter thread
    let waiter = thread::spawn(move || {
        println!("  [Waiter] Waiting on futex with value 0...");
        
        let futex_ptr = futex_clone.as_ptr();
        let result = unsafe {
            sys_futex_wait(futex_ptr as *mut u32, 0, std::ptr::null(), 0)
        };
        
        println!("  [Waiter] Woken up! Result: {}", result);
        result
    });
    
    // Give waiter time to start waiting - use yield instead of sleep
    for _ in 0..10 {
        thread::yield_now();
    }
    
    // Wake the waiter
    println!("  [Main] Waking up one waiter...");
    let wake_result = unsafe {
        sys_futex_wake(futex.as_ptr() as *mut u32, 1)
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

/// Test 2: Futex with multiple waiters
fn test_multiple_waiters() {
    println!("[Test 2] Futex with multiple waiters");
    
    let futex = Arc::new(AtomicU32::new(0));
    let num_waiters = 3;
    let mut handles = vec![];
    
    // Spawn multiple waiter threads
    for i in 0..num_waiters {
        let futex_clone = Arc::clone(&futex);
        let handle = thread::spawn(move || {
            println!("  [Waiter {}] Waiting on futex...", i);
            
            let futex_ptr = futex_clone.as_ptr();
            let result = unsafe {
                sys_futex_wait(futex_ptr as *mut u32, 0, std::ptr::null(), 0)
            };
            
            println!("  [Waiter {}] Woken up! Result: {}", i, result);
            result
        });
        handles.push(handle);
    }
    
    // Give waiters time to start waiting - use yield instead of sleep
    for _ in 0..20 {
        thread::yield_now();
    }
    
    // Wake all waiters
    println!("  [Main] Waking up all {} waiters...", num_waiters);
    let wake_result = unsafe {
        sys_futex_wake(futex.as_ptr() as *mut u32, num_waiters)
    };
    println!("  [Main] Wake result: {} (threads woken)", wake_result);
    
    // Wait for all waiters to finish
    let mut all_passed = true;
    for handle in handles {
        let result = handle.join().unwrap();
        if result != 0 {
            all_passed = false;
        }
    }
    
    if all_passed && wake_result == num_waiters {
        println!("  ✓ Test 2 passed");
    } else {
        println!("  ✗ Test 2 failed: wake_result={}", wake_result);
    }
}

/// Test 3: Futex with timeout
fn test_futex_timeout() {
    println!("[Test 3] Futex with timeout");
    
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
            sys_futex_wait(futex_ptr as *mut u32, 0, &timeout as *const libc::timespec, 0)
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
        println!("  ✓ Test 3 passed (timed out as expected)");
    } else {
        println!("  ✗ Test 3 failed: result={}, elapsed={:?}", result, elapsed);
    }
}

/// Test 4: Value mismatch test
#[allow(dead_code)]
fn test_value_mismatch() {
    println!("[Test 4] Futex value mismatch");
    
    let futex = Arc::new(AtomicU32::new(42));
    
    println!("  [Main] Futex value is 42, attempting to wait expecting 0...");
    
    let futex_ptr = futex.as_ptr();
    let result = unsafe {
        // Expect 0 but actual value is 42 - should return immediately with EAGAIN
        sys_futex_wait(futex_ptr as *mut u32, 0, std::ptr::null(), 0)
    };
    
    // EAGAIN is typically -11
    if result < 0 {
        println!("  [Main] Returned immediately with error: {} (EAGAIN)", result);
        println!("  ✓ Test 4 passed");
    } else {
        println!("  ✗ Test 4 failed: expected error, got {}", result);
    }
}

/// Test 5: Selective wake test
#[allow(dead_code)]
fn test_selective_wake() {
    println!("[Test 5] Selective wake (wake fewer than total waiters)");
    
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
                sys_futex_wait(futex_ptr as *mut u32, 0, std::ptr::null(), 0)
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
        sys_futex_wake(futex.as_ptr() as *mut u32, num_to_wake)
    };
    println!("  [Main] Wake result: {} (threads woken)", wake_result);
    
    if wake_result == num_to_wake {
        println!("  ✓ Test 5 passed");
    } else {
        println!("  ✗ Test 5 failed: expected to wake {}, actually woke {}", num_to_wake, wake_result);
    }
    
    // Clean up: wake remaining waiters
    thread::sleep(Duration::from_millis(100));
    unsafe {
        sys_futex_wake(futex.as_ptr() as *mut u32, num_waiters);
    }
    
    for handle in handles {
        let _ = handle.join();
    }
}