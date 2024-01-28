macro_rules!
debug_println {
        ($($args:tt)*) => {
            {
                #[cfg(debug_assertions)]
                {
                    let log = format!($($args)*);
                    let current_thread = std::thread::current();
                    let thread_name = current_thread.name().unwrap();
                    println!("[{}] {}", thread_name, log);
                }
            }
        };
    }

pub(crate) use debug_println;
