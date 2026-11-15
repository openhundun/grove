//! Workspace-owned PTYs with stable identities and cached automatic titles.
use crate::settings::Typography;
use ely_core::forms::TextInput;
use ely_terminal::Launch;
use ely_terminal::Terminal;
use gpui_kit::*;
use std::path::Path;
use std::path::PathBuf;

pub struct Session {
    pub id: usize,
    pub launch_cwd: PathBuf,
    pub terminal: Option<Entity<Terminal>>,
    pub error: Option<String>,
    pub subscription: Option<Subscription>,
    pub process_task: Option<Task<()>>,
    pub rename_input: Option<Entity<TextInput>>,
    pub rename_subscription: Option<Subscription>,
    pub rename_return_focus: Option<FocusHandle>,
    title: SessionTitle,
}

struct SessionTitle {
    shell: String,
    custom: Option<SharedString>,
    osc: Option<SharedString>,
    osc_group: Option<u32>,
    foreground_group: Option<u32>,
    foreground_name: Option<String>,
}

impl SessionTitle {
    fn new(shell: String) -> Self {
        Self { shell, custom: None, osc: None, osc_group: None, foreground_group: None, foreground_name: None }
    }

    fn for_process<'a>(&'a self, group: Option<u32>, name: Option<&'a str>) -> &'a str {
        self.custom
            .as_deref()
            .or_else(|| self.osc.as_deref().filter(|_| self.osc_group == group))
            .or(name)
            .unwrap_or(&self.shell)
    }

    fn display(&self) -> &str {
        self.for_process(self.foreground_group, self.foreground_name.as_deref())
    }

    fn foreground(&mut self, group: Option<u32>, name: Option<String>) -> bool {
        let changed = self.display() != self.for_process(group, name.as_deref());
        self.foreground_group = group;
        self.foreground_name = name;
        changed
    }
}

impl Session {
    pub fn display_title(&self) -> &str {
        self.title.display()
    }

    pub fn has_custom_title(&self) -> bool {
        self.title.custom.is_some()
    }

    pub fn rename(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        self.title.custom = Some(name.to_owned().into());
        true
    }

    pub fn reset_title(&mut self) {
        self.title.custom = None;
    }

    pub fn observe_title(&mut self, title: Option<SharedString>, cx: &App) -> bool {
        let previous = self.display_title().to_owned();
        self.poll_process(cx);
        self.title.osc = title;
        self.title.osc_group = self.title.foreground_group;
        previous != self.display_title()
    }

    /// Called by the session's cancellable timer and title events, never by rendering.
    pub fn poll_process(&mut self, cx: &App) -> bool {
        let Some(terminal) = &self.terminal else { return false };
        let terminal = terminal.read(cx);
        let Ok(group) = terminal.foreground_process_group_id() else { return false };
        let name = group.and_then(|pid| grove_platform::process_name(pid).ok());
        self.title.foreground(group, name)
    }

    pub fn is_live(&self, cx: &App) -> bool {
        self.terminal.as_ref().is_some_and(|terminal| terminal.read(cx).exit_status().is_none())
    }

    /// Unsubscribe and cancel metadata collection before stopping the PTY.
    pub fn dispose(&mut self, cx: &mut App) {
        self.subscription = None;
        self.process_task = None;
        self.rename_subscription = None;
        self.rename_input = None;
        self.rename_return_focus = None;
        if let Some(terminal) = self.terminal.take() {
            terminal.update(cx, |terminal, _| terminal.shutdown());
        }
    }

    pub fn spawn(id: usize, cwd: PathBuf, typography: &Typography, cx: &mut App) -> Self {
        let config = grove_platform::shell_launch(&cwd);
        let shell = Path::new(&config.program)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Shell".into());
        let mut session = Self {
            id,
            launch_cwd: cwd,
            terminal: None,
            error: None,
            subscription: None,
            process_task: None,
            rename_input: None,
            rename_subscription: None,
            rename_return_focus: None,
            title: SessionTitle::new(shell),
        };
        let program = match config.program.into_string() {
            Ok(program) => program,
            Err(_) => {
                session.error = Some("Shell path is not valid UTF-8".into());
                return session;
            }
        };
        let argument_list = match config.argument_list.into_iter().map(|argument| argument.into_string()).collect::<Result<Vec<_>, _>>() {
            Ok(argument_list) => argument_list,
            Err(_) => {
                session.error = Some("Shell argument is not valid UTF-8".into());
                return session;
            }
        };
        let launch = Launch { cwd: Some(config.cwd), program: Some((program, argument_list)), env: Vec::new() };
        match Terminal::spawn(launch, cx) {
            Ok(terminal) => {
                terminal.update(cx, |terminal, cx| {
                    terminal.set_typography(
                        typography.font_family.clone(),
                        typography.font_size,
                        FontWeight(typography.font_weight as f32),
                        typography.line_height,
                        cx,
                    )
                });
                session.terminal = Some(terminal);
                let title = session.terminal.as_ref().and_then(|terminal| terminal.read(cx).title().cloned());
                session.observe_title(title, cx);
            }
            Err(reason) => session.error = Some(format!("Unable to start terminal: {reason:#}")),
        }
        session
    }
}

#[cfg(test)]
mod tests {
    use super::SessionTitle;

    #[test]
    fn foreground_process_uses_only_its_own_osc_title() {
        let mut title = SessionTitle::new("zsh".into());
        title.foreground(Some(10), Some("zsh".into()));
        title.osc = Some("workspace".into());
        title.osc_group = Some(10);
        assert_eq!(title.display(), "workspace");
        assert!(title.foreground(Some(20), Some("sleep".into())));
        assert_eq!(title.display(), "sleep");
        title.osc = Some("build".into());
        title.osc_group = Some(20);
        assert_eq!(title.display(), "build");
        title.custom = Some("my shell".into());
        title.osc = Some("updated build".into());
        assert_eq!(title.display(), "my shell");
        assert!(!title.foreground(Some(20), Some("sleep".into())));
        title.custom = None;
        assert_eq!(title.display(), "updated build");
        title.osc = None;
        assert_eq!(title.display(), "sleep");
    }
}
