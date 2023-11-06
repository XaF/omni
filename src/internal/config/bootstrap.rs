use std::process::exit;

use crate::internal::commands::config_bootstrap;
use crate::internal::config::global_config_loader;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_error;
use crate::omni_print;

pub fn ensure_bootstrap() {
    // Get the global configuration
    let config_loader = global_config_loader();
    if !config_loader.loaded_config_files.is_empty() {
        return;
    }

    omni_print!("Oh, hello! \u{1F44B}");
    omni_print!("This seems to be the first time you're using omni.");
    omni_print!("Let's get you started with a few questions.");

    match config_bootstrap() {
        Ok(true) => {
            omni_print!("All done! Your configuration file has been written \u{1F389}");
        }
        Ok(false) => {
            omni_print!("Alright, I won't write your configuration for now \u{1F44D}");
            omni_print!(format!(
                "You can always run {} later.",
                "omni config bootstrap".to_string().light_yellow()
            ));
        }
        Err(err) => {
            omni_error!(format!("{}", err), "config bootstrap");
            exit(1);
        }
    }
}
