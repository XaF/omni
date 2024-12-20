use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command as StdCommand;
use std::process::Stdio;

use tokio::process::Command as TokioCommand;

/// Extension trait for `Command` to allow for common operations
/// between `std::process::Command` and `tokio::process::Command`.
pub trait CommandExt {
    /// Remove an environment variable from the command.
    fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>;

    // /// Clear all environment variables from the command.
    // fn env_clear(&mut self) -> &mut Self;

    /// Remove an environment variable from the command.
    fn env_remove<K>(&mut self, key: K) -> &mut Self
    where
        K: AsRef<OsStr>;

    // /// Add multiple environment variables to the command.
    // fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    // where
    // I: IntoIterator<Item = (K, V)>,
    // K: AsRef<OsStr>,
    // V: AsRef<OsStr>;

    /// Remove all environment variables that start with the given prefix.
    /// This is useful for cleaning up the environment before running a command.
    fn env_remove_prefix<K>(&mut self, prefix: K) -> &mut Self
    where
        K: AsRef<OsStr>,
    {
        let prefix = prefix.as_ref().to_string_lossy().to_string();
        let env_vars: Vec<String> = env::vars()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(key, _)| key)
            .collect();

        for var in env_vars {
            self.env_remove(&var);
        }

        self
    }

    fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self;
    fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self;
    fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self;
}

impl CommandExt for StdCommand {
    fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env(key, val)
    }

    // fn env_clear(&mut self) -> &mut Self {
    // self.env_clear()
    // }

    fn env_remove<K>(&mut self, key: K) -> &mut Self
    where
        K: AsRef<OsStr>,
    {
        self.env_remove(key)
    }

    // fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    // where
    // I: IntoIterator<Item = (K, V)>,
    // K: AsRef<OsStr>,
    // V: AsRef<OsStr>,
    // {
    // self.envs::<I, K, V>(vars)
    // }

    fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stderr(cfg)
    }

    fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stdout(cfg)
    }

    fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.current_dir(dir)
    }
}

impl CommandExt for TokioCommand {
    fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env(key, val)
    }

    // fn env_clear(&mut self) -> &mut Self {
    // self.env_clear()
    // }

    fn env_remove<K>(&mut self, key: K) -> &mut Self
    where
        K: AsRef<OsStr>,
    {
        self.env_remove(key)
    }

    // fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    // where
    // I: IntoIterator<Item = (K, V)>,
    // K: AsRef<OsStr>,
    // V: AsRef<OsStr>,
    // {
    // self.envs::<I, K, V>(vars)
    // }

    fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stderr(cfg)
    }

    fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stdout(cfg)
    }

    fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.current_dir(dir)
    }
}
