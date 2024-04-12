use crate::internal::cache::CacheObject;
use crate::internal::cache::RepositoriesCache;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::ORG_LOADER;
use crate::internal::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_error;
use crate::omni_info;

pub fn is_trusted<T: AsRef<str>>(path: T) -> bool {
    let path = path.as_ref();
    let git = git_env(path);
    if git.in_repo() && git.has_origin() {
        for org in ORG_LOADER.orgs() {
            if org.config.trusted && org.hosts_repo(git.origin().unwrap()) {
                return true;
            }
        }
    }

    let workdir = workdir(path);
    if let Some(workdir_id) = workdir.trust_id() {
        RepositoriesCache::get().has_trusted(&workdir_id)
    } else {
        false
    }
}

pub fn is_trusted_or_ask(path: &str, ask: String) -> bool {
    if is_trusted(path) {
        return true;
    }

    if !shell_is_interactive() {
        return false;
    }

    let mut choices = vec![('y', "Yes, this time (and ask me everytime)"), ('n', "No")];

    let workdir = workdir(path);
    if let Some(workdir_id) = workdir.trust_id() {
        choices.insert(0, ('a', "Yes, always (add to trusted directories)"));
        omni_info!(format!(
            "The directory {} is not in your trusted directories.",
            workdir_id.light_blue()
        ));
        omni_info!(format!(
            "{} all repositories in a trusted organization are automatically trusted.",
            "Tip:".bold()
        ));
    } else {
        omni_info!(format!("The path {} is not trusted.", path.light_blue()));
    };

    let question = requestty::Question::expand("trust_repo")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message(ask)
        .choices(choices)
        .default('y')
        .build();

    match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::ExpandItem(expanditem) => match expanditem.key {
                'y' => true,
                'n' => false,
                'a' => add_trust(path),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}", format!("[✘] {:?}", err).red());
            false
        }
    }
}

pub fn add_trust(path: &str) -> bool {
    let wd = workdir(path);
    if let Some(workdir_id) = wd.trust_id() {
        if let Err(err) = RepositoriesCache::exclusive(|repos| repos.add_trusted(&workdir_id)) {
            omni_error!(format!("Unable to update cache: {:?}", err.to_string()));
            return false;
        }
    } else {
        omni_error!("Unable to get repository id");
        return false;
    }
    true
}

pub fn remove_trust(path: &str) -> bool {
    let wd = workdir(path);
    if let Some(workdir_id) = wd.trust_id() {
        if let Err(err) = RepositoriesCache::exclusive(|repos| repos.remove_trusted(&workdir_id)) {
            omni_error!(format!("Unable to update cache: {:?}", err.to_string()));
            return false;
        }
    } else {
        omni_error!("Unable to get repository id");
        return false;
    }
    true
}
