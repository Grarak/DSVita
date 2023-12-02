macro_rules!
debug_println {
        ($($args:tt)*) => {
            if crate::DEBUG {
                println!($($args)*)
            }
        };
    }

pub(crate) use debug_println;
