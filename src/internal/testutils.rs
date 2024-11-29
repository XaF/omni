cfg_if::cfg_if! {
    if #[cfg(test)] {
        use std::sync::atomic::{AtomicUsize, Ordering};

        use crate::internal::cache::database::cleanup_test_pool;
        use crate::internal::config::flush_config;

        static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);
        static RUN_WITH_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        pub(crate) fn run_with_env<F>(envs: &[(String, Option<String>)], closure: F)
        where
            F: FnOnce(),
        {
            // Take the lock, we need to manage it ourselves because we want to
            // avoid side-effects of the environment variables being already set
            // for another test, which could impact this test
            let _lock = RUN_WITH_ENV_LOCK.lock().expect("failed to lock");

            // We create a temporary directory which will host all file-system
            // related operations for the test environment
            let tempdir = tempfile::Builder::new()
                .prefix("omni_tests.")
                .rand_bytes(12)
                .tempdir()
                .expect("failed to create temp dir");

            // Create a 'tmp/' directory in the temp dir
            let tmp_dir = tempdir.path().join("tmp");
            std::fs::create_dir(&tmp_dir).expect("failed to create tmp dir");

            // CPrepare a unique test ID that can be used to identify test resources
            let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst).to_string();

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
                (
                    "TMPDIR".into(),
                    Some(tmp_dir.to_string_lossy().to_string()),
                ),
                (
                    "TEST_POOL_ID".into(),
                    Some(format!("test-pool-{}", test_id)),
                ),
            ]
            .into_iter()
            .chain(envs.iter().cloned())
            .collect();

            // Make sure to flush the config before the test; this is required
            // as otherwise the config is optimized to be kept in memory instead
            // of being re-read from the file system each time
            flush_config("/");

            // Run the test with the temporary environment
            temp_env::with_vars(run_env, || {
                // Run the test closure
                closure();

                // Cleanup the test pool if it was created
                cleanup_test_pool();
            });

            // Make sure to flush the config after the test
            flush_config("/");
        }
    }
}
