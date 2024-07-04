use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::user_interface::colors::StringColor;

#[derive(Debug, Clone)]
pub struct VoidCommand {
    name: Vec<String>,
    type_ordering: usize,
    category: Vec<String>,
}

impl VoidCommand {
    pub fn new_for_help(name: Vec<String>) -> Self {
        Self {
            name,
            type_ordering: 0,
            category: vec![],
        }
    }

    pub fn new(name: Vec<String>, type_ordering: usize, category: Vec<String>) -> Self {
        Self {
            name,
            type_ordering,
            category,
        }
    }

    pub fn name(&self) -> Vec<String> {
        self.name.clone()
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(format!(
            "Provides {} commands",
            self.name.join(" ").italic(),
        ))
    }

    pub fn type_sort_order(&self) -> usize {
        self.type_ordering
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg::new_required_with_desc("subcommand", "Subcommand to be called"),
                SyntaxOptArg::new_option_with_desc(
                    "options...",
                    "Options to pass to the subcommand",
                ),
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(self.category.clone())
    }
}
