fn main() -> std::io::Result<()> {
    cfg_if::cfg_if! {
        if #[cfg(all(unix, feature = "preemptive-schedule"))] {
            use open_coroutine_core::scheduler::Scheduler;
            use std::ffi::c_void;
            use std::sync::{Arc, Condvar, Mutex};
            use std::time::Duration;

            fn result(result: usize) -> &'static mut c_void {
                unsafe { std::mem::transmute(result) }
            }

            static mut TEST_FLAG1: bool = true;
            static mut TEST_FLAG2: bool = true;
            let pair = Arc::new((Mutex::new(true), Condvar::new()));
            let pair2 = Arc::clone(&pair);
            let handler = std::thread::spawn(move || {
                let scheduler = Scheduler::new();
                _ = scheduler.submit(|_, _| {
                    println!("coroutine1 launched");
                    while unsafe { TEST_FLAG1 } {
                        println!("loop1");
                        _ = unsafe { libc::usleep(10_000) };
                    }
                    println!("loop1 end");
                    result(1)
                }, None);
                _ = scheduler.submit(|_, _| {
                    println!("coroutine2 launched");
                    while unsafe { TEST_FLAG2 } {
                        println!("loop2");
                        _ = unsafe { libc::usleep(10_000) };
                    }
                    println!("loop2 end");
                    unsafe { TEST_FLAG1 = false };
                    result(2)
                }, None);
                _ = scheduler.submit(|_, _| {
                    println!("coroutine3 launched");
                    unsafe { TEST_FLAG2 = false };
                    result(3)
                }, None);
                scheduler.try_schedule();

                let (lock, cvar) = &*pair2;
                let mut pending = lock.lock().unwrap();
                *pending = false;
                // notify the condvar that the value has changed.
                cvar.notify_one();
            });

            // wait for the thread to start up
            let (lock, cvar) = &*pair;
            let result = cvar
                .wait_timeout_while(
                    lock.lock().unwrap(),
                    Duration::from_millis(3000),
                    |&mut pending| pending,
                )
                .unwrap();
            if result.1.timed_out() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "preemptive schedule failed",
                ))
            } else {
                unsafe {
                    handler.join().unwrap();
                    assert!(!TEST_FLAG1);
                }
                Ok(())
            }
        } else {
            println!("please enable preemptive-schedule feature");
            Ok(())
        }
    }
}