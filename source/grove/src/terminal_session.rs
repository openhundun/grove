//! Ordinary ELY PTYs owned by a workspace tab and identified by launch cwd and slot.
use crate::settings::Typography;
use ely_terminal::Launch;
use ely_terminal::Terminal;
use gpui_kit::*;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub cwd: PathBuf,
    pub slot: usize,
}

pub struct Session {
    pub terminal: Option<Entity<Terminal>>,
    pub error: Option<String>,
    _subscription: Option<Subscription>,
}

impl Session {
    pub fn is_live(&self, cx: &App) -> bool {
        self.terminal.as_ref().is_some_and(|terminal| terminal.read(cx).exit_status().is_none())
    }

    /// Unsubscribe before shutdown; rendered entity clones must not keep a closed workspace's PTY alive.
    pub fn dispose(&mut self, cx: &mut App) {
        self._subscription = None;
        if let Some(terminal) = self.terminal.take() {
            terminal.update(cx, |terminal, _| terminal.shutdown());
        }
    }

    pub fn status(&self, cx: &App) -> &'static str {
        match &self.terminal {
            Some(terminal) if terminal.read(cx).exit_status().is_some() => "Exited",
            Some(_) => "PTY",
            None => "Error",
        }
    }
    pub fn spawn(key: &SessionKey, typography: &Typography, window: &mut Window, cx: &mut App) -> Self {
        let config = grove_platform::shell_launch(&key.cwd);
        let program = match config.program.into_string() {
            Ok(program) => program,
            Err(_) => return Self { terminal: None, error: Some("Shell path is not valid UTF-8".into()), _subscription: None },
        };
        let argument_list = match config.argument_list.into_iter().map(|argument| argument.into_string()).collect::<Result<Vec<_>, _>>() {
            Ok(argument_list) => argument_list,
            Err(_) => return Self { terminal: None, error: Some("Shell argument is not valid UTF-8".into()), _subscription: None },
        };
        let launch = Launch { cwd: Some(config.cwd), program: Some((program, argument_list)), env: Vec::new() };
        let mut error = None;
        let terminal = cx.new(|cx| match Terminal::spawn(launch, cx) {
            Ok(terminal) => terminal
                .font_family(typography.font_family.clone())
                .font_size(typography.font_size)
                .font_weight(FontWeight(typography.font_weight as f32))
                .line_height(typography.line_height),
            Err(reason) => {
                error = Some(format!("Unable to start terminal: {reason:#}"));
                // GPUI's entity builder is infallible. Discard this inert entity below.
                Terminal::replay(b"", 80, 24, cx)
            }
        });
        if error.is_some() {
            return Self { terminal: None, error, _subscription: None };
        }
        let subscription = cx.subscribe(&terminal, |_, _: &ely_terminal::TerminalEvent, cx| cx.refresh_windows());
        window.focus(&terminal.focus_handle(cx), cx);
        Self { terminal: Some(terminal), error: None, _subscription: Some(subscription) }
    }
}
