use std::fmt::Write;

pub const PLUGIN_COMMAND: &str = "python3";
const UNICODE_ESCAPE_WIDTH: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetDraft {
    pub id: String,
    pub arguments: Vec<String>,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub inherit_env: Vec<String>,
    pub environment: Vec<(String, String)>,
}

impl WidgetDraft {
    pub fn to_toml_block(&self) -> String {
        let mut block = String::from("[[widgets]]\n");
        push_assignment(&mut block, "id", &quoted(&self.id));
        push_assignment(&mut block, "command", &quoted(PLUGIN_COMMAND));
        push_assignment(&mut block, "args", &quoted_list(&self.arguments));
        push_assignment(
            &mut block,
            "interval_seconds",
            &self.interval_seconds.to_string(),
        );
        push_assignment(
            &mut block,
            "timeout_seconds",
            &self.timeout_seconds.to_string(),
        );
        if !self.inherit_env.is_empty() {
            push_assignment(&mut block, "inherit_env", &quoted_list(&self.inherit_env));
        }
        if !self.environment.is_empty() {
            block.push_str("\n[widgets.env]\n");
            for (name, value) in &self.environment {
                push_assignment(&mut block, name, &quoted(value));
            }
        }
        block
    }
}

fn push_assignment(block: &mut String, key: &str, value: &str) {
    block.push_str(key);
    block.push_str(" = ");
    block.push_str(value);
    block.push('\n');
}

fn quoted_list(items: &[String]) -> String {
    let quoted_items: Vec<String> = items.iter().map(|item| quoted(item)).collect();
    format!("[{}]", quoted_items.join(", "))
}

fn quoted(text: &str) -> String {
    let mut escaped = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(
                    escaped,
                    "\\u{:0width$X}",
                    u32::from(control),
                    width = UNICODE_ESCAPE_WIDTH
                );
            }
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> WidgetDraft {
        WidgetDraft {
            id: "jira-minhas-tarefas".to_string(),
            arguments: vec![
                "plugins/jira/jira.py".to_string(),
                "--jql".to_string(),
                "assignee = currentUser()".to_string(),
            ],
            interval_seconds: 120,
            timeout_seconds: 45,
            inherit_env: vec!["JIRA_TOKEN_EMPRESA".to_string()],
            environment: vec![
                ("JIRA_SITE".to_string(), "empresa.atlassian.net".to_string()),
                (
                    "JIRA_TOKEN_ENV".to_string(),
                    "JIRA_TOKEN_EMPRESA".to_string(),
                ),
            ],
        }
    }

    #[test]
    fn block_matches_the_documented_layout() {
        let expected = "[[widgets]]\n\
id = \"jira-minhas-tarefas\"\n\
command = \"python3\"\n\
args = [\"plugins/jira/jira.py\", \"--jql\", \"assignee = currentUser()\"]\n\
interval_seconds = 120\n\
timeout_seconds = 45\n\
inherit_env = [\"JIRA_TOKEN_EMPRESA\"]\n\
\n\
[widgets.env]\n\
JIRA_SITE = \"empresa.atlassian.net\"\n\
JIRA_TOKEN_ENV = \"JIRA_TOKEN_EMPRESA\"\n";
        assert_eq!(draft().to_toml_block(), expected);
    }

    #[test]
    fn empty_inherit_env_and_environment_are_left_out() {
        let mut bare = draft();
        bare.inherit_env.clear();
        bare.environment.clear();
        let block = bare.to_toml_block();
        assert!(!block.contains("inherit_env"));
        assert!(!block.contains("[widgets.env]"));
    }

    #[test]
    fn quotes_backslashes_and_control_characters_are_escaped() {
        let mut tricky = draft();
        tricky.arguments = vec!["a \"b\" \\ c\nd\u{1}".to_string()];
        let block = tricky.to_toml_block();
        assert!(block.contains(r#"args = ["a \"b\" \\ c\nd\u0001"]"#));
        let parsed: toml::Table = toml::from_str(&block).unwrap();
        let widgets = parsed["widgets"].as_array().unwrap();
        assert_eq!(
            widgets[0]["args"][0].as_str().unwrap(),
            "a \"b\" \\ c\nd\u{1}"
        );
    }
}
