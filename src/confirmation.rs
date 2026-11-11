use mosaic::application::ports::{PromptError, Prompter};

const AFFIRMATIVE_ANSWERS: [&str; 3] = ["s", "sim", "y"];
const NEGATIVE_ANSWERS: [&str; 3] = ["n", "nao", "não"];

pub fn confirm(prompter: &mut impl Prompter, question: &str) -> Result<bool, PromptError> {
    confirm_with_default(prompter, question, true)
}

pub fn confirm_default_no(
    prompter: &mut impl Prompter,
    question: &str,
) -> Result<bool, PromptError> {
    confirm_with_default(prompter, question, false)
}

fn confirm_with_default(
    prompter: &mut impl Prompter,
    question: &str,
    default: bool,
) -> Result<bool, PromptError> {
    loop {
        let answer = prompter.ask(question)?.trim().to_lowercase();
        if answer.is_empty() {
            return Ok(default);
        }
        if AFFIRMATIVE_ANSWERS.contains(&answer.as_str()) {
            return Ok(true);
        }
        if NEGATIVE_ANSWERS.contains(&answer.as_str()) {
            return Ok(false);
        }
        prompter.tell("Responda s ou n.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scripted(std::vec::IntoIter<&'static str>);

    impl Prompter for Scripted {
        fn ask(&mut self, _question: &str) -> Result<String, PromptError> {
            self.0
                .next()
                .map(str::to_string)
                .ok_or(PromptError::EndOfInput)
        }

        fn tell(&mut self, _message: &str) {}
    }

    fn scripted(answers: Vec<&'static str>) -> Scripted {
        Scripted(answers.into_iter())
    }

    #[test]
    fn default_no_accepts_only_explicit_yes_and_preserves_eof() {
        for answer in ["", "n", "nao", "não"] {
            assert!(!confirm_default_no(&mut scripted(vec![answer]), "?").unwrap());
        }
        for answer in ["S", "sim", "y"] {
            assert!(confirm_default_no(&mut scripted(vec![answer]), "?").unwrap());
        }
        assert!(confirm_default_no(&mut scripted(vec!["talvez", "s"]), "?").unwrap());
        assert_eq!(
            confirm_default_no(&mut scripted(vec![]), "?").unwrap_err(),
            PromptError::EndOfInput
        );
    }

    #[test]
    fn enter_and_yes_confirm_and_no_declines() {
        assert!(confirm(&mut scripted(vec![""]), "?").unwrap());
        assert!(confirm(&mut scripted(vec!["S"]), "?").unwrap());
        assert!(!confirm(&mut scripted(vec!["não"]), "?").unwrap());
    }

    #[test]
    fn an_unclear_answer_is_asked_again_and_the_end_of_input_is_an_error() {
        assert!(!confirm(&mut scripted(vec!["talvez", "n"]), "?").unwrap());
        assert_eq!(
            confirm(&mut scripted(vec![]), "?").unwrap_err(),
            PromptError::EndOfInput
        );
    }
}
