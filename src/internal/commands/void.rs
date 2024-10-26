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
            parameters: vec![
                SyntaxOptArg {
                    name: "subcommand".to_string(),
                    desc: Some("Subcommand to be called".to_string()),
                    required: true,
                    ..Default::default()
                },
                SyntaxOptArg {
                    name: "options".to_string(),
                    desc: Some("Options to pass to the subcommand".to_string()),
                    leftovers: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(self.category.clone())
    }
}
