use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::time::Duration;

use git_url_parse::normalize_url;
use git_url_parse::GitUrl;
use itertools::Itertools;
use lazy_static::lazy_static;
use tokio::runtime::Runtime;
use tokio::time::timeout;
use url::Url;

use crate::internal::commands::utils::abs_path;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::env::data_home;
use crate::internal::errors::GitUrlError;
use crate::internal::git_env;

lazy_static! {
    pub static ref PACKAGE_PATH: String = format!("{}/packages", data_home());
}

const PACKAGE_PATH_FORMAT: &str = "%{host}/%{org}/%{repo}";

pub fn package_root_path() -> String {
    PACKAGE_PATH.clone()
}

/* The safe_* helpers are to avoid the risk of Regular Expression Denial of Service (ReDos) attacks.
 * This is a similar issue to CVE-2023-32758 - https://github.com/advisories/GHSA-4xqq-73wg-5mjp
 * By setting a timeout, we prevent things from hanging indefinitely in case of such attack.
 */

static TIMEOUT_DURATION: Duration = Duration::from_secs(2);

async fn async_normalize_url(url: &str) -> Result<Url, GitUrlError> {
    Ok(normalize_url(url)?)
}

pub fn safe_normalize_url(url: &str) -> Result<Url, GitUrlError> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        match timeout(TIMEOUT_DURATION, async_normalize_url(url)).await {
            Ok(result) => result,
            Err(_) => Err(GitUrlError::NormalizeTimeout),
        }
    })
}

async fn async_git_url_parse(url: &str) -> Result<GitUrl, GitUrlError> {
    Ok(GitUrl::parse(url)?)
}

pub fn safe_git_url_parse(url: &str) -> Result<GitUrl, GitUrlError> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        match timeout(TIMEOUT_DURATION, async_git_url_parse(url)).await {
            Ok(result) => result,
            Err(_) => Err(GitUrlError::ParseTimeout),
        }
    })
}

pub fn id_from_git_url(url: &GitUrl) -> Option<String> {
    let url = url.clone();
    if let (Some(host), Some(owner), name) = (url.host, url.owner, url.name) {
        if !name.is_empty() {
            return Some(format!("{}:{}/{}", host, owner, name));
        }
    }
    None
}

pub fn full_git_url_parse(url: &str) -> Result<GitUrl, GitUrlError> {
    // let url = safe_normalize_url(url)?;
    // let git_url = safe_git_url_parse(url.as_str())?;
    let git_url = safe_git_url_parse(url)?;

    if git_url.scheme.to_string() == "file" {
        return Err(GitUrlError::UnsupportedScheme(git_url.scheme.to_string()));
    }
    if git_url.name.is_empty() {
        return Err(GitUrlError::MissingRepositoryName);
    }
    if git_url.owner.is_none() {
        return Err(GitUrlError::MissingRepositoryOwner);
    }
    if git_url.host.is_none() {
        return Err(GitUrlError::MissingRepositoryHost);
    }

    Ok(git_url)
}

pub fn format_path_with_template(worktree: &str, git_url: &GitUrl, path_format: &str) -> PathBuf {
    let git_url = git_url.clone();
    format_path_with_template_and_data(
        worktree,
        &git_url.host.unwrap(),
        &git_url.owner.unwrap(),
        &git_url.name,
        path_format,
    )
}

pub fn format_path_with_template_and_data(
    worktree: &str,
    host: &str,
    owner: &str,
    repo: &str,
    path_format: &str,
) -> PathBuf {
    // Create a path object
    let mut path = PathBuf::from(worktree.to_string());

    // Replace %{host}, #{owner}, and %{repo} with the actual values
    let path_format = path_format.to_string();
    let path_format = path_format.replace("%{host}", host);
    let path_format = path_format.replace("%{org}", owner);
    let path_format = path_format.replace("%{repo}", repo);

    // Split the path format into parts
    let path_format_parts = path_format.split('/');

    // Append each part to the path
    for part in path_format_parts {
        path.push(part);
    }

    // Return the path as a string
    path
}

pub fn package_path_from_handle(handle: &str) -> Option<PathBuf> {
    if let Ok(git_url) = full_git_url_parse(handle) {
        package_path_from_git_url(&git_url)
    } else {
        None
    }
}

pub fn package_path_from_git_url(git_url: &GitUrl) -> Option<PathBuf> {
    if git_url.scheme.to_string() == "file"
        || git_url.name.is_empty()
        || git_url.owner.is_none()
        || git_url.host.is_none()
    {
        return None;
    }

    let package_path =
        format_path_with_template(package_root_path().as_str(), git_url, PACKAGE_PATH_FORMAT);

    Some(package_path)
}

pub fn path_entry_config<T: AsRef<str>>(path: T) -> PathEntryConfig {
    let path: &str = path.as_ref();
    let git_env = git_env(path);

    let mut path_entry_config = PathEntryConfig {
        path: path.to_string(),
        package: None,
        full_path: path.to_string(),
    };

    if let (Some(id), Some(root)) = (git_env.id(), git_env.root()) {
        if PathBuf::from(root).starts_with(package_root_path()) {
            path_entry_config.package = Some(id.to_string());
            path_entry_config.path = PathBuf::from(path)
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
        }
    }

    path_entry_config
}

/// Checks if a given file path is ignored by Git according to .gitignore rules
///
/// # Arguments
/// * `file_path` - The path to the file to check
///
/// # Returns
/// * `Ok(bool)` - True if the file is ignored, false otherwise
/// * `Err(Box<dyn Error>)` - If there's an error accessing the repository or the path
///
/// # Example
/// ```rust
/// let is_ignored = is_path_gitignored("src/temp.log")?;
/// println!("Is file ignored: {}", is_ignored);
/// ```
pub fn is_path_gitignored<P: AsRef<Path>>(path: P) -> Result<bool, Box<dyn std::error::Error>> {
    let path = path.as_ref();

    // Find the directory to start the repository search from
    let mut search_dir = path;
    while !search_dir.is_dir() {
        search_dir = search_dir.parent().ok_or("Path has no parent directory")?;
    }
    let search_dir = search_dir.to_path_buf();

    // Try to find the Git repository from the path's directory
    let repo = git2::Repository::discover(search_dir)?;

    // Get the absolute path
    let abs_path = match std::fs::canonicalize(path) {
        Ok(abs_path) => abs_path,
        Err(_) => abs_path(path),
    };

    // Get the path relative to the repository root
    let repo_path = repo
        .workdir()
        .ok_or("Repository has no working directory")?;
    let rel_path = abs_path.strip_prefix(repo_path)?;

    // For directories, we check if a theoretical file inside would be ignored
    let check_path = if path.is_dir() {
        let uuid = uuid::Uuid::new_v4();
        rel_path.join(uuid.to_string())
    } else {
        rel_path.to_path_buf()
    };

    // Check if the path is ignored
    match repo.status_file(&check_path) {
        Ok(status) => Ok(status.contains(git2::Status::IGNORED)),
        Err(e) => {
            // If the path doesn't exist, we can still check if it would be ignored
            if e.code() == git2::ErrorCode::NotFound {
                Ok(repo.status_should_ignore(&check_path)?)
            } else {
                Err(e.into())
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct GitContributor {
    pub name: String,
    #[allow(dead_code)]
    pub email: String,
    #[allow(dead_code)]
    pub added: u32,
    #[allow(dead_code)]
    pub removed: u32,
}

/// Get the top contributors for a given file in a Git repository
///
/// The function uses `git log --numstat --follow --pretty=format:"%aN <%aE>" -- <file>`
/// and parses the output to get the top contributors for the file.
///
/// # Arguments
/// * `path` - The path to the file in the repository
/// * `top_n` - The number of top contributors to get
///
/// # Returns
/// * `Some(Vec<GitContributor>)` - The top contributors for the file
/// * `None` - If there's an error running the git command or parsing the output,
///           or if there are no contributors
///
/// # Example
/// ```rust
/// let top_contributors = get_top_contributors("src/main.rs", 5);
/// if let Some(contributors) = top_contributors {
///    for contributor in contributors {
///         println!("{}: {} added, {} removed", contributor.name,
///                  contributor.added, contributor.removed);
///    }
/// }
/// ```
pub fn get_top_contributors(path: &str, top_n: usize) -> Option<Vec<GitContributor>> {
    // Get the repo path
    let gitenv = git_env(path);
    let repo_path = gitenv.root()?;

    // Get the file path relative to the repository root; we can use
    // canonicalize to get the absolute path since the path is
    // supposed to exist, otherwise we can't get the top contributors
    // anyway.
    let abs_file_path = std::fs::canonicalize(path).ok()?;
    let rel_file_path = abs_file_path.strip_prefix(repo_path).ok()?;
    let rel_file_path_str = rel_file_path.to_str()?;

    // Run git shortlog to get the top contributor
    let output = StdCommand::new("git")
        .current_dir(repo_path)
        .arg("log")
        .arg("--numstat")
        .arg("--follow")
        .arg("--pretty=format:\"%aN <%aE>\"")
        .arg("--")
        .arg(rel_file_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;

    // We want to grab a two-line pattern:
    //   first lastname <email>
    //   123  456  filename

    #[derive(Debug)]
    struct GitContributions {
        added: u32,
        removed: u32,
    }

    let mut contributors = HashMap::new();
    let mut current_contributor = None;
    for line in stdout.lines() {
        if line.is_empty() {
            current_contributor = None;
            continue;
        } else if let Some(contributor) = current_contributor {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() != 3 || parts[2] != rel_file_path_str {
                continue;
            }

            let added = match parts[0].parse::<u32>() {
                Ok(added) => added,
                Err(_) => continue,
            };
            let removed = match parts[1].parse::<u32>() {
                Ok(removed) => removed,
                Err(_) => continue,
            };

            let contributions = contributors.entry(contributor).or_insert(GitContributions {
                added: 0,
                removed: 0,
            });

            contributions.added += added;
            contributions.removed += removed;
        } else {
            current_contributor = Some(line);
        }
    }

    // Get the top N contributors
    let top_contributors: Vec<_> = contributors
        .iter()
        .sorted_by_key(|(_, contributions)| (contributions.added, contributions.removed))
        .rev()
        .take(top_n)
        .flat_map(|(contributor, contributions)| {
            // Remove the quotes
            let contributor = contributor.trim_matches('"');

            // Split the name and email
            let parts: Vec<&str> = contributor.splitn(2, " <").collect();
            let name = parts.get(0)?;
            let email = parts.get(1).and_then(|email| email.strip_suffix('>'))?;

            Some(GitContributor {
                name: name.to_string(),
                email: email.to_string(),
                added: contributions.added,
                removed: contributions.removed,
            })
        })
        .collect();

    if top_contributors.is_empty() {
        return None;
    }

    Some(top_contributors)
}

#[derive(Debug)]
pub struct CodeOwners {
    path: String,
    owners: Vec<String>,
}

impl CodeOwners {
    pub fn new(path: String, owners: Vec<String>) -> Self {
        Self { path, owners }
    }

    pub fn matches<T1, T2>(&self, path: T1, repo_path: T2) -> bool
    where
        T1: AsRef<str>,
        T2: AsRef<str>,
    {
        let path = path.as_ref();
        let repo_path = repo_path.as_ref();

        // Get the path to match
        let path_to_match = if let Some(path) = self.path.strip_prefix('/') {
            Path::new(repo_path).join(path)
        } else {
            Path::new("**").join(self.path.as_str())
        };

        // Use glob to match the path
        let glob = match glob::Pattern::new(&path_to_match.to_string_lossy()) {
            Ok(glob) => glob,
            Err(_) => return false,
        };

        // Check if the path matches
        if glob.matches(path) {
            return true;
        }

        // Try checking it as a directory, by appending ** to the path
        let path_to_match = path_to_match.join("**");
        let glob = match glob::Pattern::new(&path_to_match.to_string_lossy()) {
            Ok(glob) => glob,
            Err(_) => return false,
        };

        glob.matches(path)
    }
}

/// Get the code owners for a given path in a Git repository
///
/// The function reads the CODEOWNERS file in the repository, following
/// the order that github uses to match paths. This also looks in the
/// bitbucket and gitlab locations but as a fallback.
///
/// # Arguments
/// * `path` - The path to get the code owners for
///
/// # Returns
/// * `Some(Vec<String>)` - The code owners for the path
/// * `None` - If there's an error reading the CODEOWNERS file or if
///            the path has no owners
///
/// # Example
/// ```rust
/// let code_owners = get_code_owners("src/main.rs");
/// if let Some(owners) = code_owners {
///   for owner in owners {
///     println!("Owner: {}", owner);
///   }
/// }
/// ```
pub fn get_code_owners(path: &str) -> Option<Vec<String>> {
    // Get the repo path
    let gitenv = git_env(path);
    let repo_path = gitenv.root()?;

    // Find the CODEOWNERS file
    static CODEOWNERS: [&str; 5] = [
        ".github/CODEOWNERS",
        "CODEOWNERS",
        "docs/CODEOWNERS",
        ".bitbucket/CODEOWNERS",
        ".gitlab/CODEOWNERS",
    ];

    // Check if any of the CODEOWNERS files exist
    let codeowners = CODEOWNERS
        .iter()
        .map(|path| Path::new(repo_path).join(path))
        .find(|path| path.exists())?;

    // Go over the file and get the code owners, the file is in the format:
    // ```
    // # This is a comment
    // * @owner1 @owner2
    // src/ @owner3
    // src/main.rs @owner4
    // etc.
    // ```
    //
    // The first column is the path, the second+ columns are the owners
    // The * is a wildcard for all files
    // The ** is a wildcard for all files and directories
    // If a path starts with /, it's a path relative to the repository root
    // If a path doesn't start with /, it matches anywhere in the repository,
    // equivalent to **/path
    // If a path is a directory, it matches all files in the directory.
    //
    // The latest match is used, so if a file matches multiple patterns, the last one is used,
    // we can thus process owners backwards and stop when we find a match.

    // Open the file and read it line by line
    let file = std::fs::File::open(codeowners).ok()?;
    let reader = std::io::BufReader::new(file);
    let all_code_owners = reader
        .lines()
        .filter_map(|line| line.ok())
        .filter_map(|line| {
            // Remove comments
            let line = match line.splitn(2, '#').next() {
                Some(line) => line.trim(),
                None => return None,
            };

            // Skip empty lines
            if line.is_empty() {
                return None;
            }

            // Split the line into parts
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }

            // Get the owners
            let owners = parts[1..].iter().map(|owner| owner.to_string()).collect();

            // Get the path
            let path = parts[0].to_string();

            Some(CodeOwners::new(path, owners))
        })
        .collect::<Vec<_>>();

    // Find the last matching code owner
    let code_owners = all_code_owners
        .iter()
        .rev()
        .find(|code_owners| code_owners.matches(path, repo_path))?;

    // Return the owners
    Some(code_owners.owners.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::{self, File};
    use std::io::Write;
    use std::os::unix::fs as unix_fs;
    use std::path::Path; // For symlink tests

    use tempfile::TempDir;

    use crate::internal::testutils::temp_dir;

    mod is_path_gitignored {
        use super::*;

        fn setup_git_repo() -> TempDir {
            let tmpdir = temp_dir();
            let repo = git2::Repository::init(tmpdir.path()).unwrap();

            // Create and commit initial .gitignore
            let gitignore_path = tmpdir.path().join(".gitignore");
            File::create(&gitignore_path).expect("Failed to create .gitignore");

            let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
            let mut index = repo.index().expect("Failed to get index");
            index
                .add_path(Path::new(".gitignore"))
                .expect("Failed to add .gitignore");
            index.write().expect("Failed to write index");

            let tree_id = index.write_tree().expect("Failed to write tree");
            let tree = repo.find_tree(tree_id).expect("Failed to find tree");
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                "Initial commit with empty .gitignore",
                &tree,
                &[],
            )
            .expect("Failed to commit");

            tmpdir
        }

        fn update_gitignore(repo_dir: &Path, content: &str) {
            let gitignore_path = repo_dir.join(".gitignore");
            fs::write(&gitignore_path, content).expect("Failed to update .gitignore");
        }

        #[test]
        fn test_basic_patterns() {
            let temp_dir = setup_git_repo();
            update_gitignore(temp_dir.path(), "*.log\n/node_modules\n");

            // Test file extension pattern
            let log_file = temp_dir.path().join("test.log");
            File::create(&log_file).unwrap();
            assert!(is_path_gitignored(&log_file).unwrap());

            // Test exact directory pattern
            let node_modules = temp_dir.path().join("node_modules");
            fs::create_dir(&node_modules).unwrap();
            assert!(is_path_gitignored(&node_modules).unwrap());
        }

        #[test]
        fn test_nested_gitignore() {
            let temp_dir = setup_git_repo();

            // Create root .gitignore
            update_gitignore(temp_dir.path(), "*.log\n!important/*.log\n");

            // Create nested .gitignore
            let subdir = temp_dir.path().join("important");
            fs::create_dir(&subdir).unwrap();
            fs::write(subdir.join(".gitignore"), "secret.log\n").unwrap();

            // Test file affected by root .gitignore but negated for subdirectory
            let important_log = subdir.join("test.log");
            File::create(&important_log).unwrap();
            assert!(!is_path_gitignored(&important_log).unwrap());

            // Test file explicitly ignored in nested .gitignore
            let secret_log = subdir.join("secret.log");
            File::create(&secret_log).unwrap();
            assert!(is_path_gitignored(&secret_log).unwrap());
        }

        #[test]
        fn test_symlinks() {
            let temp_dir = setup_git_repo();
            update_gitignore(temp_dir.path(), "target/\n*.log\n");

            // Create a real directory that's ignored
            let real_dir = temp_dir.path().join("target");
            fs::create_dir(&real_dir).unwrap();

            // Create a symlink to the ignored directory
            let symlink_dir = temp_dir.path().join("linked_target");
            unix_fs::symlink(&real_dir, &symlink_dir).unwrap();

            // Test that both real path and symlink are ignored
            assert!(is_path_gitignored(&real_dir).unwrap());
            assert!(is_path_gitignored(&symlink_dir).unwrap());
        }

        #[test]
        fn test_nonexistent_paths() {
            let temp_dir = setup_git_repo();
            let temp_path =
                std::fs::canonicalize(temp_dir.path()).expect("Failed to get canonical path");

            update_gitignore(&temp_path, "*.log\n/build/\n");

            // Test nonexistent file
            let nonexistent = temp_path.join("nonexistent.log");
            assert!(is_path_gitignored(&nonexistent).unwrap());

            // Test nonexistent directory
            let nonexistent_dir = temp_path.join("build/test-file");
            assert!(is_path_gitignored(&nonexistent_dir).unwrap());
        }

        #[test]
        fn test_complex_patterns() {
            let temp_dir = setup_git_repo();
            update_gitignore(
                temp_dir.path(),
                "# Complex patterns\n\
                 **/node_modules/**\n\
                 *.log\n\
                 !important.log\n\
                 src/**/*.test.js\n\
                 **/temp/\n\
                 /*.txt\n",
            );

            // Test deeply nested node_modules
            let nested_modules = temp_dir.path().join("frontend/src/node_modules");
            fs::create_dir_all(&nested_modules).unwrap();
            assert!(is_path_gitignored(&nested_modules).unwrap());

            // Test negated pattern
            let important = temp_dir.path().join("important.log");
            File::create(&important).unwrap();
            assert!(!is_path_gitignored(&important).unwrap());

            // Test pattern with multiple globs
            let test_file = temp_dir.path().join("src/components/button.test.js");
            fs::create_dir_all(test_file.parent().unwrap()).unwrap();
            File::create(&test_file).unwrap();
            assert!(is_path_gitignored(&test_file).unwrap());

            // Test root-level pattern
            let root_txt = temp_dir.path().join("root.txt");
            File::create(&root_txt).unwrap();
            assert!(is_path_gitignored(&root_txt).unwrap());
        }

        #[test]
        fn test_special_paths() {
            let temp_dir = setup_git_repo();
            update_gitignore(
                temp_dir.path(),
                "# Edge cases\n\
                 /\\\\ weird\\\\chars/*.txt\n\
                 !/*.md\n\
                 *.bak\n\
                 !/\\[special\\]*.bak\n",
            );

            // Test paths with special characters
            let weird_dir = temp_dir.path().join("\\ weird\\chars");
            fs::create_dir_all(&weird_dir).unwrap();
            let weird_file = weird_dir.join("test.txt");
            File::create(&weird_file).unwrap();
            assert!(is_path_gitignored(&weird_file).unwrap());

            // Test paths with brackets
            let special_file = temp_dir.path().join("[special]file.bak");
            File::create(&special_file).unwrap();
            assert!(!is_path_gitignored(&special_file).unwrap());
        }

        #[test]
        fn test_outside_repository() {
            let temp_dir = temp_dir();
            let test_file = temp_dir.path().join("test.txt");
            File::create(&test_file).expect("Failed to create file");
            assert!(is_path_gitignored(&test_file).is_err());
        }

        #[test]
        fn test_without_gitignore() {
            let temp_dir = setup_git_repo();
            let test_file = temp_dir.path().join("test.txt");
            File::create(&test_file).expect("Failed to create file");
            assert!(!is_path_gitignored(&test_file).unwrap());
        }

        #[test]
        fn test_with_corrupted_gitignore() {
            let temp_dir = setup_git_repo();
            let invalid_content = vec![0xFF, 0xFF, 0xFF, 0xFF];
            fs::write(temp_dir.path().join(".gitignore"), invalid_content).unwrap();
            let test_file = temp_dir.path().join("test.txt");
            File::create(&test_file).unwrap();
            assert!(!is_path_gitignored(&test_file).unwrap()); // Should still work
        }

        #[test]
        fn test_parent_dir_patterns() {
            let temp_dir = setup_git_repo();
            update_gitignore(
                temp_dir.path(),
                "foo/**/bar\n\
                 **/baz\n\
                 /qux/**/*.txt\n",
            );

            // Test deep nested pattern matching
            let nested_bar = temp_dir.path().join("foo/a/b/c/bar");
            fs::create_dir_all(&nested_bar).unwrap();
            assert!(is_path_gitignored(&nested_bar).unwrap());

            // Test pattern matching anywhere
            let deep_baz = temp_dir.path().join("x/y/baz");
            fs::create_dir_all(&deep_baz).unwrap();
            assert!(is_path_gitignored(&deep_baz).unwrap());

            // Test root-anchored deep pattern
            let qux_txt = temp_dir.path().join("qux/a/b/test.txt");
            fs::create_dir_all(qux_txt.parent().unwrap()).unwrap();
            File::create(&qux_txt).unwrap();
            assert!(is_path_gitignored(&qux_txt).unwrap());
        }
    }

    mod get_code_owners {
        use super::*;

        fn setup_git_repo() -> TempDir {
            let tmpdir = temp_dir();
            let repo = git2::Repository::init(tmpdir.path()).expect("Failed to init git repo");

            // Create initial commit to make it a valid repo
            let signature = git2::Signature::now("Test User", "test@example.com")
                .expect("Failed to create signature");
            let tree_id = {
                let mut index = repo.index().expect("Failed to get index");
                index.write_tree().expect("Failed to write tree")
            };
            let tree = repo.find_tree(tree_id).expect("Failed to find tree");
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                "Initial commit",
                &tree,
                &[],
            )
            .expect("Failed to commit");

            tmpdir
        }

        fn write_codeowners(repo_dir: &Path, codeowners_path: &str, content: &str) {
            let full_path = repo_dir.join(codeowners_path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            let mut file = File::create(full_path).unwrap();
            write!(file, "{}", content).unwrap();
        }

        #[test]
        fn test_basic_matching() {
            let repo = setup_git_repo();
            write_codeowners(
                repo.path(),
                ".github/CODEOWNERS",
                "*.rs @rust-team\nsrc/ @src-team\n",
            );

            let test_file = repo.path().join("test.rs");
            File::create(&test_file).expect("Failed to create file");

            let owners = get_code_owners(test_file.to_str().unwrap());
            assert!(owners.is_some());
            assert_eq!(owners.unwrap(), vec!["@rust-team"]);
        }

        #[test]
        fn test_directory_matching() {
            let repo = setup_git_repo();
            write_codeowners(repo.path(), ".github/CODEOWNERS", "src/ @src-team\n");

            let test_file = repo.path().join("src").join("test.rs");
            fs::create_dir_all(test_file.parent().unwrap()).unwrap();
            File::create(&test_file).unwrap();

            let owners = get_code_owners(test_file.to_str().unwrap());
            assert!(owners.is_some());
            assert_eq!(owners.unwrap(), vec!["@src-team"]);
        }

        #[test]
        fn test_multiple_owners() {
            let repo = setup_git_repo();
            write_codeowners(
                repo.path(),
                ".github/CODEOWNERS",
                "*.rs @rust-team @code-reviewers",
            );

            let test_file = repo.path().join("test.rs");
            File::create(&test_file).unwrap();

            let owners = get_code_owners(test_file.to_str().unwrap());
            assert!(owners.is_some());
            assert_eq!(owners.unwrap(), vec!["@rust-team", "@code-reviewers"]);
        }

        #[test]
        fn test_last_match_wins() {
            let repo = setup_git_repo();
            write_codeowners(
                repo.path(),
                ".github/CODEOWNERS",
                "*.rs @rust-team\ntest.rs @specific-team",
            );

            let test_file = repo.path().join("test.rs");
            File::create(&test_file).unwrap();

            let owners = get_code_owners(test_file.to_str().unwrap());
            assert!(owners.is_some());
            assert_eq!(owners.unwrap(), vec!["@specific-team"]);
        }

        #[test]
        fn test_no_match() {
            let repo = setup_git_repo();
            write_codeowners(repo.path(), ".github/CODEOWNERS", "*.rs @rust-team");

            let test_file = repo.path().join("test.txt");
            File::create(&test_file).unwrap();

            let owners = get_code_owners(test_file.to_str().unwrap());
            assert!(owners.is_none());
        }

        #[test]
        fn test_fallback_codeowners_locations() {
            let repo = setup_git_repo();
            write_codeowners(
                repo.path(),
                "CODEOWNERS", // Root location
                "*.rs @rust-team",
            );

            let test_file = repo.path().join("test.rs");
            File::create(&test_file).unwrap();

            let owners = get_code_owners(test_file.to_str().unwrap());
            assert!(owners.is_some());
            assert_eq!(owners.unwrap(), vec!["@rust-team"]);
        }
    }

    mod get_top_contributors {
        use super::*;
        use git2::{Repository, Signature};

        fn setup_git_repo_with_history() -> (TempDir, String) {
            let tmpdir = temp_dir();
            let repo = Repository::init(tmpdir.path()).expect("Failed to init git repo");

            // Create test file
            let test_file_path = tmpdir.path().join("test.txt");
            let test_file_rel_path = "test.txt";

            // Create multiple commits with different authors
            let commits = vec![
                ("Alice Smith", "alice@example.com", "Initial content"),
                ("Bob Jones", "bob@example.com", "Update 1"),
                ("Alice Smith", "alice@example.com", "Update 2"),
                ("Charlie Brown", "charlie@example.com", "Update 3"),
            ];

            for (name, email, message) in commits {
                let signature = Signature::now(name, email).expect("Failed to create signature");

                // Write content to file
                let mut file = File::create(&test_file_path).expect("Failed to create file");
                writeln!(file, "{}", message).expect("Failed to write to file");

                // Stage and commit
                let mut index = repo.index().expect("Failed to get index");
                index
                    .add_path(Path::new(test_file_rel_path))
                    .expect("Failed to add path");
                index.write().expect("Failed to write index");

                let tree_id = index.write_tree().expect("Failed to write tree");
                let tree = repo.find_tree(tree_id).expect("Failed to find tree");

                let parent = repo
                    .head()
                    .ok()
                    .map(|head| head.peel_to_commit().expect("Failed to peel"));
                let parents = parent.as_ref().map_or_else(Vec::new, |c| vec![c]);

                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &parents,
                )
                .expect("Failed to commit");
            }

            (tmpdir, test_file_path.to_string_lossy().to_string())
        }

        #[test]
        fn test_simple() {
            let (_repo_dir, test_file) = setup_git_repo_with_history();

            // Print environment for file
            let gitenv = git_env(&test_file);
            eprintln!("{:?}", gitenv);

            let contributors = get_top_contributors(&test_file, 3);
            assert!(contributors.is_some());

            let contributors = contributors.unwrap();
            assert_eq!(contributors.len(), 3);

            // Alice should be the top contributor with 2 commits
            assert_eq!(contributors[0].name, "Alice Smith");
            assert_eq!(contributors[0].email, "alice@example.com");

            // Verify we got all contributors
            let names: Vec<String> = contributors.iter().map(|c| c.name.clone()).collect();
            assert!(names.contains(&"Bob Jones".to_string()));
            assert!(names.contains(&"Charlie Brown".to_string()));
        }

        #[test]
        fn test_more_than_limit() {
            let (_repo_dir, test_file) = setup_git_repo_with_history();

            let contributors = get_top_contributors(&test_file, 2);
            assert!(contributors.is_some());

            let contributors = contributors.unwrap();
            assert_eq!(contributors.len(), 2);
        }

        #[test]
        fn test_less_than_limit() {
            let (_repo_dir, test_file) = setup_git_repo_with_history();

            let contributors = get_top_contributors(&test_file, 5);
            assert!(contributors.is_some());

            let contributors = contributors.unwrap();
            assert_eq!(contributors.len(), 3);
        }

        #[test]
        fn test_nonexistent_file() {
            let (repo_dir, _) = setup_git_repo_with_history();
            let nonexistent_file = repo_dir.path().join("nonexistent.txt");

            let contributors = get_top_contributors(nonexistent_file.to_str().unwrap(), 1);
            assert!(contributors.is_none());
        }

        #[test]
        fn test_empty_repo() {
            let tmpdir = temp_dir();
            let _repo = Repository::init(tmpdir.path()).unwrap();

            let test_file = tmpdir.path().join("test.txt");
            File::create(&test_file).unwrap();

            let contributors = get_top_contributors(test_file.to_str().unwrap(), 1);
            assert!(contributors.is_none());
        }
    }
}
