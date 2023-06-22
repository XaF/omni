use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use git_url_parse::normalize_url;
use git_url_parse::GitUrl;
use tokio::runtime::Runtime;
use tokio::time::timeout;
use url::ParseError;
use url::Url;

use crate::internal::config;

/* These helpers are to avoid the risk of Regular Expression Denial of Service (ReDos) attacks.
 * This is a similar issue to CVE-2023-32758 - https://github.com/advisories/GHSA-4xqq-73wg-5mjp
 * By setting a timeout, we prevent things from hanging indefinitely in case of such attack.
 */

static TIMEOUT_DURATION: Duration = Duration::from_secs(2);

async fn async_normalize_url(url: &str) -> Result<Url, <GitUrl as FromStr>::Err> {
    normalize_url(url)
}

pub fn safe_normalize_url(url: &str) -> Result<Url, <GitUrl as FromStr>::Err> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        match timeout(TIMEOUT_DURATION, async_normalize_url(url)).await {
            Ok(result) => result,
            Err(_) => Err(<GitUrl as FromStr>::Err::new(ParseError::Overflow)),
        }
    })
}

async fn async_git_url_parse(url: &str) -> Result<GitUrl, <GitUrl as FromStr>::Err> {
    GitUrl::parse(url)
}

pub fn safe_git_url_parse(url: &str) -> Result<GitUrl, <GitUrl as FromStr>::Err> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        match timeout(TIMEOUT_DURATION, async_git_url_parse(url)).await {
            Ok(result) => result,
            Err(_) => Err(<GitUrl as FromStr>::Err::new(ParseError::Overflow)),
        }
    })
}

pub fn format_path(worktree: &str, git_url: &GitUrl) -> PathBuf {
    // Create a path object
    let mut path = PathBuf::from(worktree.to_string());

    // Get the configured path format
    let path_format = config(".").repo_path_format.clone();

    // Replace %{host}, #{owner}, and %{repo} with the actual values
    let git_url = git_url.clone();
    let path_format = path_format.replace("%{host}", &git_url.host.unwrap());
    let path_format = path_format.replace("%{org}", &git_url.owner.unwrap());
    let path_format = path_format.replace("%{repo}", &git_url.name);

    // Split the path format into parts
    let path_format_parts = path_format.split("/");

    // Append each part to the path
    for part in path_format_parts {
        path.push(part);
    }

    // Return the path as a string
    path
}
