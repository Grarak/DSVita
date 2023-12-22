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

    pub fn schedule<F: FnOnce() + Send + 'static>(&self, f: F) {
        self.pool.spawn(f)
    }
}
