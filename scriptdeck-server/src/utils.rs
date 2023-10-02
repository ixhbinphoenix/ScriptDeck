#[macro_export]
macro_rules! new_shared {
    ($a:expr) => {
        Arc::new(RwLock::new($a))
    };
}
