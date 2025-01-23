mod org;
pub(crate) use org::Org;
pub(crate) use org::Repo;
pub(crate) use org::ORG_LOADER;

mod utils;
pub(crate) use utils::format_path_with_template;
pub(crate) use utils::full_git_url_parse;
pub(crate) use utils::id_from_git_url;
pub(crate) use utils::is_path_gitignored;
pub(crate) use utils::package_path_from_git_url;
pub(crate) use utils::package_path_from_handle;
pub(crate) use utils::package_root_path;
pub(crate) use utils::path_entry_config;
pub(crate) use utils::safe_git_url_parse;
pub(crate) use utils::safe_normalize_url;

mod updater;
pub(crate) use updater::auto_update_async;
pub(crate) use updater::auto_update_on_command_not_found;
pub(crate) use updater::exec_update;
pub(crate) use updater::exec_update_and_log_on_error;
pub(crate) use updater::report_update_error;
pub(crate) use updater::GitRepoUpdater;
