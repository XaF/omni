pub mod base;
pub use base::Command;

mod builtin;

mod fromconfig;

mod frommakefile;

mod frompath;

pub mod loader;
pub use loader::command_loader;
pub use loader::COMMAND_LOADER;

pub mod path;
pub use path::OMNIPATH;

pub mod utils;
