use mosaic::application::ports::{PromptError, Prompter};
use mosaic::domain::language::Language;

const AFFIRMATIVE_ANSWERS: [&str; 4] = ["s", "sim", "y", "yes"];
const NEGATIVE_ANSWERS: [&str; 4] = ["n", "nao", "não", "no"];

pub fn confirm(
    prompter: &mut impl Prompter,
    question: &str,
    language: Language,
) -> Result<bool, PromptError> {
    confirm_with_default(prompter, question, true, language)
}

pub fn confirm_default_no(
    prompter: &mut impl Prompter,
    question: &str,
    language: Language,
) -> Result<bool, PromptError> {
    confirm_with_default(prompter, question, false, language)
}

fn confirm_with_default(
    prompter: &mut impl Prompter,
    question: &str,
    default: bool,
    language: Language,
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
        prompter.tell(language.pick("Answer y or n.", "Responda s ou n."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosaic::domain::language::Language;

    struct Scripted {
        answers: std::vec::IntoIter<&'static str>,
        told: Vec<String>,
    }

    impl Prompter for Scripted {
        fn ask(&mut self, _question: &str) -> Result<String, PromptError> {
            self.answers
                .next()
                .map(str::to_string)
                .ok_or(PromptError::EndOfInput)
        }

        fn tell(&mut self, message: &str) {
            self.told.push(message.to_string());
        }
    }

    fn scripted(answers: Vec<&'static str>) -> Scripted {
        Scripted {
            answers: answers.into_iter(),
            told: Vec::new(),
        }
    }

    #[test]
    fn default_no_accepts_only_explicit_yes_and_preserves_eof() {
        for answer in ["", "n", "nao", "não"] {
            assert!(!confirm_default_no(
                &mut scripted(vec![answer]),
                "?",
                Language::BrazilianPortuguese
            )
            .unwrap());
        }
        for answer in ["S", "sim", "y"] {
            assert!(confirm_default_no(
                &mut scripted(vec![answer]),
                "?",
                Language::BrazilianPortuguese
            )
            .unwrap());
        }
        assert!(confirm_default_no(
            &mut scripted(vec!["talvez", "s"]),
            "?",
            Language::BrazilianPortuguese
        )
        .unwrap());
        assert_eq!(
            confirm_default_no(&mut scripted(vec![]), "?", Language::BrazilianPortuguese)
                .unwrap_err(),
            PromptError::EndOfInput
        );
    }

    #[test]
    fn enter_and_yes_confirm_and_no_declines() {
        assert!(confirm(&mut scripted(vec![""]), "?", Language::BrazilianPortuguese).unwrap());
        assert!(confirm(&mut scripted(vec!["S"]), "?", Language::BrazilianPortuguese).unwrap());
        assert!(!confirm(
            &mut scripted(vec!["não"]),
            "?",
            Language::BrazilianPortuguese
        )
        .unwrap());
    }

    #[test]
    fn an_unclear_answer_is_asked_again_and_the_end_of_input_is_an_error() {
        assert!(!confirm(
            &mut scripted(vec!["talvez", "n"]),
            "?",
            Language::BrazilianPortuguese
        )
        .unwrap());
        assert_eq!(
            confirm(&mut scripted(vec![]), "?", Language::BrazilianPortuguese).unwrap_err(),
            PromptError::EndOfInput
        );
    }

    #[test]
    fn english_answers_are_accepted_and_the_reprompt_is_in_english() {
        let mut prompter = scripted(vec!["maybe", "yes"]);
        assert!(confirm_default_no(&mut prompter, "?", Language::English).unwrap());
        assert_eq!(prompter.told, vec!["Answer y or n.".to_string()]);
        assert!(!confirm(&mut scripted(vec!["no"]), "?", Language::English).unwrap());
    }
}
