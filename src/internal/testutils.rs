cfg_if::cfg_if! {
    if #[cfg(test)] {
        use std::sync::atomic::{AtomicUsize, Ordering};

        use crate::internal::cache::database::cleanup_test_pool;
        use crate::internal::config::flush_config;

        static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

        pub(crate) fn run_with_env<F>(envs: &[(String, Option<String>)], closure: F)
        where
            F: FnOnce(),
        {
            let tempdir = tempfile::Builder::new()
                .prefix("omni_tests.")
                .rand_bytes(12)
                .tempdir()
                .expect("failed to create temp dir");

            let run_env: Vec<(String, Option<String>)> = vec![
                ("XDG_DATA_HOME".into(), None),
                ("XDG_CONFIG_HOME".into(), None),
                ("XDG_CACHE_HOME".into(), None),
                ("XDG_RUNTIME_DIR".into(), None),
                ("OMNI_DATA_HOME".into(), None),
                ("OMNI_CACHE_HOME".into(), None),
                ("OMNI_CMD_FILE".into(), None),
                ("HOMEBREW_PREFIX".into(), None),
                (
                    "HOME".into(),
                    Some(tempdir.path().join("home").to_string_lossy().to_string()),
                ),
                (
                    "PATH".into(),
                    Some(format!(
                        "{}:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                        tempdir.path().join("bin").to_string_lossy()
                    )),
                ),
            ]
            .into_iter()
            .chain(envs.iter().cloned())
            .collect();

            temp_env::with_vars(run_env, || {
                // Make sure to flush the config before the test
                flush_config("/");

                // Run the test
                closure();

                // Make sure to flush the config after the test
                flush_config("/");
            });
        }

        pub(crate) fn run_with_env_and_cache<F>(envs: &[(String, Option<String>)], closure: F)
        where
            F: FnOnce(),
        {
            let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst).to_string();

            let run_env: Vec<(String, Option<String>)> = vec![
                (
                    "TEST_POOL_ID".into(),
                    Some(format!("test-pool-{}", test_id)),
                ),
            ]
            .into_iter()
            .chain(envs.iter().cloned())
            .collect();

            run_with_env(&run_env, || {
                closure();

                cleanup_test_pool();
            })
        }
    }
}
