use std::path::PathBuf;

pub const APP_DIRECTORY_NAME: &str = "mosaic";
pub const PRIORITY_PROMPT_RELATIVE_PATH: &str = "prompts/prioridade.md";

const LLM_WORKING_DIRECTORY_NAME: &str = "llm";
const RECOMMENDATION_FILE_NAME: &str = "recommendation.json";
const UPDATE_CHECK_FILE_NAME: &str = "update-check.json";

pub fn config_directory() -> Option<PathBuf> {
    dirs::config_dir().map(inside_app_directory)
}

pub fn state_directory() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(inside_app_directory)
}

pub fn llm_working_directory() -> Option<PathBuf> {
    state_directory().map(|directory| directory.join(LLM_WORKING_DIRECTORY_NAME))
}

pub fn recommendation_file() -> Option<PathBuf> {
    state_directory().map(|directory| directory.join(RECOMMENDATION_FILE_NAME))
}

pub fn update_check_file() -> Option<PathBuf> {
    state_directory().map(|directory| directory.join(UPDATE_CHECK_FILE_NAME))
}

fn inside_app_directory(base: PathBuf) -> PathBuf {
    base.join(APP_DIRECTORY_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_directory_is_named_after_the_app() {
        assert_eq!(
            inside_app_directory(PathBuf::from("/base")),
            PathBuf::from("/base/mosaic")
        );
    }

    #[test]
    fn llm_and_recommendation_paths_live_inside_the_state_directory() {
        let Some(state) = state_directory() else {
            return;
        };
        assert_eq!(llm_working_directory(), Some(state.join("llm")));
        assert_eq!(
            recommendation_file(),
            Some(state.join("recommendation.json"))
        );
        assert_eq!(update_check_file(), Some(state.join("update-check.json")));
    }

    #[test]
    fn priority_prompt_path_is_relative_to_the_config_directory() {
        assert!(std::path::Path::new(PRIORITY_PROMPT_RELATIVE_PATH).is_relative());
        assert!(PRIORITY_PROMPT_RELATIVE_PATH.ends_with("prioridade.md"));
    }
}
