use std::io::{BufRead, Write};

use crate::application::ports::{PromptError, Prompter};

pub struct StreamPrompter<Input: BufRead, Output: Write> {
    input: Input,
    output: Output,
}

impl<Input: BufRead, Output: Write> StreamPrompter<Input, Output> {
    pub fn new(input: Input, output: Output) -> Self {
        Self { input, output }
    }
}

impl<Input: BufRead, Output: Write> Prompter for StreamPrompter<Input, Output> {
    fn ask(&mut self, question: &str) -> Result<String, PromptError> {
        write!(self.output, "{question}")
            .and_then(|()| self.output.flush())
            .map_err(|error| PromptError::Failed(error.to_string()))?;
        let mut line = String::new();
        let bytes_read = self
            .input
            .read_line(&mut line)
            .map_err(|error| PromptError::Failed(error.to_string()))?;
        if bytes_read == 0 {
            return Err(PromptError::EndOfInput);
        }
        Ok(line.trim_end_matches(['\n', '\r']).to_string())
    }

    fn tell(&mut self, message: &str) {
        let _ = writeln!(self.output, "{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_prints_the_question_and_returns_the_line_without_the_newline() {
        let mut output = Vec::new();
        let mut prompter = StreamPrompter::new("resposta\r\nsegunda\n".as_bytes(), &mut output);
        assert_eq!(prompter.ask("Pergunta: ").unwrap(), "resposta");
        assert_eq!(prompter.ask("Outra: ").unwrap(), "segunda");
        prompter.tell("aviso");
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "Pergunta: Outra: aviso\n"
        );
    }

    #[test]
    fn the_end_of_the_input_is_an_error_not_an_empty_answer() {
        let mut prompter = StreamPrompter::new("".as_bytes(), Vec::new());
        assert_eq!(prompter.ask("?").unwrap_err(), PromptError::EndOfInput);
    }
}
