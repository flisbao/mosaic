use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::application::ports::{AlertMessage, Notifier};
use crate::infrastructure::process::{run_process, CancelFlag, HasCancelFlag, ProcessLimits};

const BELL_CHARACTER: &[u8] = b"\x07";
const NOTIFY_TIMEOUT: Duration = Duration::from_secs(5);
const NOTIFY_MAXIMUM_OUTPUT_BYTES: usize = 1_024;
pub const MAXIMUM_PENDING_NOTIFICATIONS: usize = 4;

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
    command: Vec<String>,
    cancel_flag: CancelFlag,
    pending: Arc<AtomicUsize>,
}

impl CommandNotifier {
    pub fn new(command: Vec<String>) -> Self {
        Self {
            command,
            cancel_flag: CancelFlag::default(),
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }
}

impl Notifier for CommandNotifier {
    fn notify(&self, message: &AlertMessage) {
        let Some((program, arguments)) = self.command.split_first() else {
            return;
        };
        if self.pending.fetch_add(1, Ordering::SeqCst) >= MAXIMUM_PENDING_NOTIFICATIONS {
            self.pending.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let mut command = Command::new(program);
        command
            .args(arguments)
            .arg(&message.title)
            .arg(&message.body);
        let limits = ProcessLimits {
            timeout: NOTIFY_TIMEOUT,
            maximum_output_bytes: NOTIFY_MAXIMUM_OUTPUT_BYTES,
        };
        let cancel_flag = self.cancel_flag.clone();
        let pending = Arc::clone(&self.pending);
        thread::spawn(move || {
            let _ = run_process(command, None, &limits, &cancel_flag);
            pending.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

impl HasCancelFlag for CommandNotifier {
    fn cancel_flag(&self) -> &CancelFlag {
        &self.cancel_flag
    }
}

impl Notifier for Arc<CommandNotifier> {
    fn notify(&self, message: &AlertMessage) {
        self.as_ref().notify(message);
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
    use std::time::Instant;

    use super::*;
    use crate::application::ports::Cancellable;

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
        let script = directory.path().join("com espaço");
        fs::create_dir(&script).unwrap();
        let script = script.join("notify");
        fs::write(
            &script,
            "#!/bin/sh\nprintf '%s|%s|%s' \"$1\" \"$2\" \"$3\" > \"$(dirname \"$0\")/saida.txt\"\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let output = script.parent().unwrap().join("saida.txt");
        let notifier =
            CommandNotifier::new(vec![script.display().to_string(), "--urgente".to_string()]);

        let deadline = Instant::now() + Duration::from_secs(5);
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
        CommandNotifier::new(vec!["programa-que-nao-existe-xyz".to_string()]).notify(&message());
        CommandNotifier::new(vec![]).notify(&message());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cancel_kills_a_hung_notification_command() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let notifier = CommandNotifier::new(vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo $$ > \"$0\"; exec sleep 30".to_string(),
            pid_file.display().to_string(),
        ]);
        notifier.notify(&message());
        let deadline = Instant::now() + Duration::from_secs(5);
        while fs::read_to_string(&pid_file).map_or(true, |text| !text.ends_with('\n'))
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid = fs::read_to_string(&pid_file).unwrap().trim().to_string();

        notifier.cancel();

        let state = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        let running = state
            .rsplit_once(')')
            .is_some_and(|(_, rest)| !rest.trim_start().starts_with('Z'));
        assert!(!running, "o comando de notificação sobreviveu ao cancel()");
    }

    #[test]
    fn pending_notifications_are_capped() {
        let notifier = CommandNotifier::new(vec![
            "sh".to_string(),
            "-c".to_string(),
            "exec sleep 2".to_string(),
        ]);
        for _ in 0..(MAXIMUM_PENDING_NOTIFICATIONS * 3) {
            notifier.notify(&message());
        }
        assert!(notifier.pending_count() <= MAXIMUM_PENDING_NOTIFICATIONS);
        notifier.cancel();
    }
}
