use once_cell::sync::Lazy;

pub static IO_SCHEDULER: Lazy<Scheduler> = Lazy::new(|| Scheduler::new("io_scheduler"));

pub struct Scheduler {
    pool: rayon::ThreadPool,
}

impl Scheduler {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let pool = rayon::ThreadPoolBuilder::new()
            .thread_name(move |_| name.clone())
            .num_threads(1)
            .build()
            .unwrap();
        Scheduler { pool }
    }

    pub fn schedule(&self, f: impl FnOnce() + Send + 'static) {
        self.pool.spawn(f)
    }
}
