//! A font settings panel for Editor, Terminal, Changes, Files, Repositories, and Commits.
//! Choices update a local snapshot and emit `FontChange`; the owner applies fonts
//! and persists application settings.

use crate::settings::FontTarget;
use crate::settings::Settings;
use ely_core::forms::FontPicker;
use ely_core::forms::TextInput;
use ely_core::primitives::IconName;
use ely_core::theme::ActiveTheme as _;
use ely_settings::SettingsLayout;
use ely_settings::SettingsRow;
use ely_settings::SettingsSection;
use gpui_kit::AppContext as _;
use gpui_kit::Context;
use gpui_kit::Entity;
use gpui_kit::EventEmitter;
use gpui_kit::FontWeight;
use gpui_kit::InteractiveElement as _;
use gpui_kit::IntoElement;
use gpui_kit::ParentElement as _;
use gpui_kit::Render;
use gpui_kit::SharedString;
use gpui_kit::StatefulInteractiveElement as _;
use gpui_kit::Styled as _;
use gpui_kit::Window;
use gpui_kit::div;
use gpui_kit::px;

const SECTION_LIST: [(&str, &str, IconName); 6] = [
    ("editor", "Editor", IconName::Code),
    ("terminal", "Terminal", IconName::Terminal),
    ("changes", "Changes", IconName::GitPullRequest),
    ("files", "Files", IconName::Folder),
    ("repositories", "Repositories", IconName::GitPullRequest),
    ("commits", "Commits", IconName::GitPullRequest),
];

pub(super) struct FontChange {
    pub target: FontTarget,
    pub family: SharedString,
}

pub(super) struct SettingsPanel {
    settings: Settings,
    selected: FontTarget,
    search_input_list: [Entity<TextInput>; 6],
    error: Option<String>,
}

impl EventEmitter<FontChange> for SettingsPanel {}

impl SettingsPanel {
    pub(super) fn new(settings: &Settings, error: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input_list = [
            "Editor font family",
            "Terminal font family",
            "Changes data font family",
            "Files data font family",
            "Repositories and worktrees font family",
            "Commits history font family",
        ]
        .map(|label| cx.new(|cx| TextInput::new(window, cx).placeholder("Search font families").label(label)));
        Self { settings: settings.clone(), selected: FontTarget::Editor, search_input_list, error }
    }

    pub(super) fn set_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.selected;
        let (key, title, description, search_index, preview): (_, _, _, _, &[&str]) = match target {
            FontTarget::Editor => (
                "editor",
                "Editor",
                "The font used for source code, commit messages, and diffs.",
                0,
                &["fn main() {", "    println!(\"Hello, Grove · 你好，世界\");", "}", "\u{e7a8} main.rs   \u{e0a0} main   0123456789"],
            ),
            FontTarget::Terminal => (
                "terminal",
                "Terminal",
                "The font used for terminal output, session titles, working directories, and exit status.",
                1,
                &["\u{f120} ~/grove  \u{e0a0} main", "$ cargo run --workspace", "Hello, Grove · 终端已就绪  0123456789"],
            ),
            FontTarget::Changes => (
                "changes",
                "Changes",
                "The font used for file and folder names, change counts, and status values in Changes.",
                2,
                &["\u{e7a8} source/main.rs", "\u{f15c} 文档/使用说明.md", "\u{f07b} assets  \u{e0a0} main  0123456789"],
            ),
            FontTarget::Files => (
                "files",
                "Files",
                "The font used for file and folder names in Files and the selected file path in Preview.",
                3,
                &["\u{f07b} source", "\u{e7a8} main.rs", "\u{f15c} 文档/使用说明.md  0123456789"],
            ),
            FontTarget::Repositories => (
                "repositories",
                "Repositories",
                "The font used for repository, branch, and worktree names, paths, and counts.",
                4,
                &["\u{f07b} grove · 代码仓库", "\u{e0a0} main  \u{f1d3} feature/字体设置", "\u{f07b} grove-worktree  0123456789"],
            ),
            FontTarget::Commits => (
                "commits",
                "Commits",
                "The font used for commit subjects, authors, dates, hashes, and history entries.",
                5,
                &[
                    "完善字体设置 · Configure independent fonts",
                    "Icefery · 2026-11-11 · a1b2c3d",
                    "\u{e0a0} main  \u{f02b} v1.0  0123456789",
                    "Staged changes · 已暂存更改",
                ],
            ),
        };
        let family = self.settings.font(target);
        let colors = &cx.theme().colors;
        let (surface, sunken, foreground, muted, border, danger, danger_background) = (colors.surface, colors.sunken, colors.fg, colors.fg_muted, colors.border, colors.danger, colors.danger_subtle);
        let picker = FontPicker::new(key, &self.search_input_list[search_index])
            .selected(family.clone())
            .on_change(cx.listener(move |this, family: &SharedString, _, cx| {
                this.settings.set_font(target, family.clone());
                cx.emit(FontChange { target, family: family.clone() });
                cx.notify();
            }));
        let page = SettingsSection::new(title)
            .description(description)
            .row(
                SettingsRow::new("Font family")
                    .description("Type to search installed fonts.")
                    .control(div().w(px(280.)).max_w_full().child(picker)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .mt(px(16.))
                    .p(px(16.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(border)
                    .bg(sunken)
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_size(px(12.)).child("Preview"))
                    .child(div().text_size(px(11.)).text_color(muted).child(family.clone()))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .font_family(family)
                            .text_size(px(13.))
                            .text_color(foreground)
                            .children(preview.iter().map(|line| div().child(*line))),
                    ),
            );
        div()
            .id("grove-font-settings")
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(24.))
            .bg(surface)
            .text_color(foreground)
            .children(self.error.as_ref().map(|error| {
                div()
                    .mb(px(20.))
                    .p(px(12.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(danger)
                    .bg(danger_background)
                    .text_color(danger)
                    .text_size(px(12.))
                    .child(error.clone())
            }))
            .child(
                SettingsLayout::new(
                    "component-font-settings",
                    SECTION_LIST,
                    key,
                    cx.listener(|this, key: &SharedString, _, cx| {
                        this.selected = match key.as_ref() {
                            "editor" => FontTarget::Editor,
                            "terminal" => FontTarget::Terminal,
                            "changes" => FontTarget::Changes,
                            "files" => FontTarget::Files,
                            "repositories" => FontTarget::Repositories,
                            "commits" => FontTarget::Commits,
                            _ => return,
                        };
                        cx.notify();
                    }),
                )
                .page(page),
            )
    }
}
