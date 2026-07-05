#[cfg(all(target_os = "vita", debug_assertions))]
lazy_static::lazy_static! {
    pub static ref LOG_FILE: std::sync::Mutex<std::fs::File> = {
        let _ = std::fs::create_dir(crate::presenter::LOG_PATH);
        std::sync::Mutex::new(std::fs::File::create(crate::presenter::LOG_FILE).unwrap())
    };
}

macro_rules! debug_println {
    ($($args:tt)*) => {
        if crate::DEBUG_LOG {
            let log = format!($($args)*);
            // Interleave into the binary instruction log (when enabled) so debug lines line up with
            // the per-instruction register snapshots; otherwise print.
            if crate::debug_inst_log::is_logging() {
                crate::debug_inst_log::log_text(&log);
            } else {
                let current_thread = std::thread::current();
                let thread_name = current_thread.name().unwrap();
                println!("[{}] {}", thread_name, log);
            }
        }
    };
}

pub(crate) use debug_println;

macro_rules! info_println {
    ($($args:tt)*) => {
        let log = format!($($args)*);
        let current_thread = std::thread::current();
        let thread_name = current_thread.name().unwrap();
        let value = format!("[{}] {}", thread_name, log);
        println!("{value}");
        #[cfg(all(target_os = "vita", debug_assertions))]
        {
            let mut log_file = crate::logging::LOG_FILE.lock().unwrap();
            std::io::Write::write(&mut *log_file, value.as_bytes()).unwrap();
            std::io::Write::write_all(&mut *log_file, "\n".as_bytes()).unwrap();
        }
    };
}
pub(crate) use info_println;

macro_rules! branch_println {
    ($($args:tt)*) => {
        if crate::BRANCH_LOG {
            let log = format!($($args)*);
            // Interleave branch decisions into the binary instruction log (when enabled) so control
            // flow can be diffed alongside the per-instruction register snapshots; otherwise print.
            if crate::debug_inst_log::is_logging() {
                crate::debug_inst_log::log_text(&log);
            } else {
                let current_thread = std::thread::current();
                let thread_name = current_thread.name().unwrap();
                println!("[{}] {}", thread_name, log);
            }
        }
    };
}
pub(crate) use branch_println;

macro_rules! block_asm_print {
    ($($args:tt)*) => {
        if crate::DEBUG_LOG {
            if crate::debug_inst_log::is_logging() {
                crate::debug_inst_log::log_text_no_newline(&format!($($args)*));
            } else {
                print!($($args)*);
            }
        }
    };
}
pub(crate) use block_asm_print;

macro_rules! block_asm_println {
    ($($args:tt)*) => {
        if crate::DEBUG_LOG {
            if crate::debug_inst_log::is_logging() {
                crate::debug_inst_log::log_text(&format!($($args)*));
            } else {
                println!($($args)*);
            }
        }
    };
}
pub(crate) use block_asm_println;

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
