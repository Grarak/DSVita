use dsvita::utils::{set_thread_prio_affinity, ThreadAffinity, ThreadPriority};
use std::thread;

// Lives in the bin (not the lib): #[used] statics in an rlib are not reliably pulled
// out of the archive by the linker, and newlib resolves this symbol weakly.
#[used]
#[export_name = "_newlib_heap_size_user"]
pub static _NEWLIB_HEAP_SIZE_USER: u32 = 256 * 1024 * 1024; // 256 MiB

fn main() {
    // For some reason setting the stack size with the global variable doesn't work
    // #[used]
    // #[export_name = "sceUserMainThreadStackSize"]
    // pub static SCE_USER_MAIN_THREAD_STACK_SIZE: u32 = 4 * 1024 * 1024;
    // Instead just create a new thread with stack size set
    if cfg!(target_os = "vita") {
        set_thread_prio_affinity(ThreadPriority::Low, &[ThreadAffinity::Core0]);
    }
    thread::Builder::new()
        .name("actual_main".to_string())
        .stack_size(4 * 1024 * 1024)
        .spawn(dsvita::actual_main)
        .unwrap()
        .join()
        .unwrap();
}
