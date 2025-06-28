#[cfg(feature = "profiling")]
#[global_allocator]
static GLOBAL: tracy_client::ProfiledAllocator<std::alloc::System> = tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

macro_rules! profiling_init {
    () => {
        #[cfg(feature = "profiling")]
        {
            tracy_client::Client::start();
            crate::logging::info_println!("Start profiling");
        }
    };
}

pub(crate) use profiling_init;

macro_rules! profiling_set_thread_name {
    ($name:literal) => {
        #[cfg(feature = "profiling")]
        tracy_client::set_thread_name!($name);
    };
}

pub(crate) use profiling_set_thread_name;

macro_rules! profiling_frame_mark {
    () => {
        #[cfg(feature = "profiling")]
        tracy_client::frame_mark();
    };
}

pub(crate) use profiling_frame_mark;

macro_rules! profiling_secondary_frame_mark {
    ($name:literal) => {
        #[cfg(feature = "profiling")]
        tracy_client::secondary_frame_mark!($name);
    };
}

pub(crate) use profiling_secondary_frame_mark;
