use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;

use crate::application::pomodoro_notifications::{AlertMessage, Notifier};

const BELL_CHARACTER: &[u8] = b"\x07";

pub struct BellNotifier {
    output: Mutex<Box<dyn Write + Send>>,
}

impl BellNotifier {
    pub fn new(output: Box<dyn Write + Send>) -> Self {
        Self {
            output: Mutex::new(output),
        }
    }
}

impl Notifier for BellNotifier {
    fn notify(&self, _message: &AlertMessage) {
        if let Ok(mut output) = self.output.lock() {
            let _ = output.write_all(BELL_CHARACTER);
            let _ = output.flush();
        }
    }
}

pub struct CommandNotifier {
    command: String,
}

impl CommandNotifier {
    pub fn new(command: String) -> Self {
        Self { command }
    }
}

impl Notifier for CommandNotifier {
    fn notify(&self, message: &AlertMessage) {
        let mut parts = self.command.split_whitespace().map(str::to_string);
        let Some(program) = parts.next() else {
            return;
        };
        let mut command = Command::new(program);
        command
            .args(parts)
            .arg(&message.title)
            .arg(&message.body)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        thread::spawn(move || {
            if let Ok(mut child) = command.spawn() {
                let _ = child.wait();
            }
        });
    }
}

pub struct CompositeNotifier {
    notifiers: Vec<Box<dyn Notifier>>,
}

impl CompositeNotifier {
    pub fn new(notifiers: Vec<Box<dyn Notifier>>) -> Self {
        Self { notifiers }
    }
}

impl Notifier for CompositeNotifier {
    fn notify(&self, message: &AlertMessage) {
        for notifier in &self.notifiers {
            notifier.notify(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::*;

    struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn message() -> AlertMessage {
        AlertMessage {
            title: "Foco concluído".to_string(),
            body: "Beber água\nPróximo: descanso curto".to_string(),
        }
    }

    struct CountingNotifier(Arc<Mutex<usize>>);

    impl Notifier for CountingNotifier {
        fn notify(&self, _message: &AlertMessage) {
            *self.0.lock().unwrap() += 1;
        }
    }

    #[test]
    fn bell_writes_the_bell_character() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        BellNotifier::new(Box::new(SharedBuffer(Arc::clone(&buffer)))).notify(&message());
        assert_eq!(*buffer.lock().unwrap(), vec![7u8]);
    }

    #[test]
    fn composite_notifies_every_notifier() {
        let count = Arc::new(Mutex::new(0));
        let composite = CompositeNotifier::new(vec![
            Box::new(CountingNotifier(Arc::clone(&count))),
            Box::new(CountingNotifier(Arc::clone(&count))),
        ]);
        composite.notify(&message());
        assert_eq!(*count.lock().unwrap(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn command_notifier_passes_title_and_body_as_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("notify");
        fs::write(
            &script,
            "#!/bin/sh\nprintf '%s|%s|%s' \"$1\" \"$2\" \"$3\" > \"$(dirname \"$0\")/saida.txt\"\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let notifier = CommandNotifier::new(format!("{} --urgente", script.display()));
        let output = directory.path().join("saida.txt");

        let deadline = Instant::now() + Duration::from_secs(3);
        while !output.exists() && Instant::now() < deadline {
            notifier.notify(&message());
            std::thread::sleep(Duration::from_millis(100));
        }

        let content = fs::read_to_string(&output).unwrap();
        assert_eq!(
            content,
            "--urgente|Foco concluído|Beber água\nPróximo: descanso curto"
        );
    }

    #[test]
    fn command_notifier_missing_program_does_not_panic() {
        CommandNotifier::new("programa-que-nao-existe-xyz".to_string()).notify(&message());
        CommandNotifier::new("   ".to_string()).notify(&message());
    }
}
