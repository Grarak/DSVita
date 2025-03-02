macro_rules! debug_println {
    ($($args:tt)*) => {
        if crate::DEBUG_LOG {
            let log = format!($($args)*);
            let current_thread = std::thread::current();
            let thread_name = current_thread.name().unwrap();
            println!("[{}] {}", thread_name, log);
        }
    };
}
pub(crate) use debug_println;

macro_rules! branch_println {
    ($($args:tt)*) => {
        if crate::BRANCH_LOG {
            let log = format!($($args)*);
            let current_thread = std::thread::current();
            let thread_name = current_thread.name().unwrap();
            println!("[{}] {}", thread_name, log);
        }
    };
}
pub(crate) use branch_println;

macro_rules! debug_panic {
    ($($args:tt)*) => {
        if crate::IS_DEBUG {
            panic!($($args)*)
        } else {
            unsafe { std::hint::unreachable_unchecked() }
        }
    };
}
pub(crate) use debug_panic;
