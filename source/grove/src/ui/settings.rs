//! Per-area typography and independent navigation panel spacing controls.
//! Changes update a local snapshot immediately and emit `SettingsChange`; the
//! owner applies and persists them, and provides the modal shell. Font families
//! use the native searchable Select without font samples or tooltips.

use crate::settings::FontTarget;
use crate::settings::PanelSettings;
use crate::settings::Settings;
use crate::settings::Typography;
use crate::settings::config_path;
use ely_core::forms::NumberInput;
use ely_core::theme::ActiveTheme as _;
use gpui_kit::AnyElement;
use gpui_kit::App;
use gpui_kit::AppContext as _;
use gpui_kit::Context;
use gpui_kit::ElementId;
use gpui_kit::Entity;
use gpui_kit::EventEmitter;
use gpui_kit::InteractiveElement as _;
use gpui_kit::IntoElement;
use gpui_kit::ParentElement as _;
use gpui_kit::Render;
use gpui_kit::Role;
use gpui_kit::SharedString;
use gpui_kit::StatefulInteractiveElement as _;
use gpui_kit::StyleRefinement;
use gpui_kit::Styled as _;
use gpui_kit::Subscription;
use gpui_kit::Window;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::select::Select;
use gpui_kit::component::select::SelectEvent;
use gpui_kit::component::select::SelectState;
use gpui_kit::div;
use gpui_kit::px;
use std::cell::RefCell;
use std::rc::Rc;

const SECTION_LIST: [(FontTarget, &str, &str, &str); 9] = [
    (
        FontTarget::Editor,
        "editor",
        "Editor",
        "Typography for source code, commit message source, and diffs. Search placeholders use secondary UI typography.",
    ),
    (FontTarget::Preview, "preview", "Preview", "Typography for rendered Markdown and commit message previews."),
    (
        FontTarget::Terminal,
        "terminal",
        "Terminal",
        "Typography for terminal output, session titles, working directories, and exit status.",
    ),
    (
        FontTarget::Changes,
        "changes",
        "Changes",
        "Typography for file and folder names, change counts, and status values in Changes.",
    ),
    (FontTarget::Files, "files", "Files", "Typography for file and folder names in Files and the selected file path."),
    (
        FontTarget::Repositories,
        "repositories",
        "Repositories",
        "Typography for repository, branch, and worktree names, paths, and counts.",
    ),
    (
        FontTarget::Commits,
        "commits",
        "Commits",
        "Typography for commit subjects, authors, dates, hashes, and history entries.",
    ),
    (
        FontTarget::Ui,
        "ui",
        "UI",
        "Primary UI typography for navigation, headings, buttons, labels, and settings row titles. Defaults to the system font at 12 px and weight 600.",
    ),
    (
        FontTarget::Description,
        "description",
        "Description",
        "Secondary UI typography for placeholders, settings descriptions, control values, and status text. Defaults to the system font at 10 px and weight 400.",
    ),
];

#[derive(Clone)]
struct FontFamilyItem {
    family: SharedString,
    style: Rc<RefCell<StyleRefinement>>,
}

impl SearchableListItem for FontFamilyItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.family.clone()
    }

    fn value(&self) -> &SharedString {
        &self.family
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().refine_style(&self.style.borrow()).child(self.family.clone())
    }
}

pub(super) enum SettingsChange {
    Typography { target: FontTarget, value: Typography },
    Navigation { target: FontTarget, value: PanelSettings },
}

pub(super) struct SettingsPanel {
    settings: Settings,
    selected: FontTarget,
    family_state_list: [Entity<SelectState<SearchableVec<FontFamilyItem>>>; SECTION_LIST.len()],
    family_style: Rc<RefCell<StyleRefinement>>,
    _family_subscription_list: Vec<Subscription>,
    error: Option<String>,
    config_path: SharedString,
}

impl EventEmitter<SettingsChange> for SettingsPanel {}

impl SettingsPanel {
    pub(super) fn new(settings: &Settings, error: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let family_style = Rc::new(RefCell::new(settings.secondary_ui_style()));
        let mut family_list: Vec<SharedString> = cx.text_system().all_font_names().into_iter().filter(|family| !family.starts_with('.')).map(Into::into).collect();
        // Keep saved families, including the system family, even when unlisted.
        for (target, _, _, _) in SECTION_LIST {
            let family = &settings.typography(target).font_family;
            if !family_list.contains(family) {
                family_list.push(family.clone());
            }
        }
        let family_item_list: Vec<FontFamilyItem> = family_list.into_iter().map(|family| FontFamilyItem { family, style: family_style.clone() }).collect();
        let family_state_list = SECTION_LIST.map(|(target, _, _, _)| {
            cx.new(|cx| {
                let mut state = SelectState::new(SearchableVec::new(family_item_list.clone()), None, window, cx).searchable(true);
                state.set_selected_value(&settings.typography(target).font_family, window, cx);
                state
            })
        });
        let family_subscription_list = SECTION_LIST
            .iter()
            .zip(&family_state_list)
            .map(|((target, _, _, _), state)| {
                let target = *target;
                cx.subscribe(state, move |this, _, event: &SelectEvent<SearchableVec<FontFamilyItem>>, cx| {
                    let SelectEvent::Confirm(Some(family)) = event else {
                        return;
                    };
                    let mut value = this.settings.typography(target).clone();
                    value.font_family = family.clone();
                    this.set_typography(target, value, cx);
                })
            })
            .collect();
        let config_path = match config_path() {
            Ok(path) => format!("Configuration: {}", path.display()),
            Err(error) => format!("Configuration path unavailable: {error}"),
        }
        .into();
        Self {
            settings: settings.clone(),
            selected: FontTarget::Editor,
            family_state_list,
            family_style,
            _family_subscription_list: family_subscription_list,
            error,
            config_path,
        }
    }

    pub(super) fn set_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    pub(super) fn sync_panel_preferences(&mut self, settings: &Settings, cx: &mut Context<Self>) {
        if self.settings.repository_panel != settings.repository_panel || self.settings.changes_panel != settings.changes_panel || self.settings.file_panel != settings.file_panel {
            self.settings.repository_panel = settings.repository_panel;
            self.settings.changes_panel = settings.changes_panel;
            self.settings.file_panel = settings.file_panel;
            cx.notify();
        }
    }

    fn set_typography(&mut self, target: FontTarget, value: Typography, cx: &mut Context<Self>) {
        self.settings.set_typography(target, value.clone());
        if matches!(target, FontTarget::Ui | FontTarget::Description) {
            *self.family_style.borrow_mut() = self.settings.secondary_ui_style();
        }
        cx.emit(SettingsChange::Typography { target, value });
        cx.notify();
    }

    fn row(&self, title: &'static str, description: &'static str, control: impl IntoElement, cx: &Context<Self>) -> AnyElement {
        let ui_style = self.settings.ui.style();
        let secondary_style = self.settings.secondary_ui_style();
        div()
            .id((ElementId::from("settings-row"), title))
            .role(Role::Group)
            .aria_label(title)
            .aria_description(description)
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .py(px(12.))
            .border_b_1()
            .border_color(cx.theme().colors.border)
            .refine_style(&secondary_style)
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().refine_style(&ui_style).child(title))
                    .child(div().refine_style(&secondary_style).text_color(cx.theme().colors.fg_muted).child(description)),
            )
            .child(div().w(px(280.)).max_w_full().min_w_0().refine_style(&secondary_style).child(control))
            .into_any_element()
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.selected;
        let search_index = SECTION_LIST.iter().position(|(candidate, _, _, _)| *candidate == target).unwrap_or(0);
        let (_, key, title, description) = SECTION_LIST[search_index];
        let typography = self.settings.typography(target);
        let ui_style = self.settings.ui.style();
        let secondary_style = self.settings.secondary_ui_style();
        let colors = &cx.theme().colors;
        let (surface, foreground, muted, danger, danger_background) = (colors.surface, colors.fg, colors.fg_muted, colors.danger, colors.danger_subtle);

        let picker = Select::new(&self.family_state_list[search_index])
            .id((ElementId::from(key), "font-family"))
            .accessibility_label(format!("{title} font family"))
            .search_placeholder("Search font families")
            // Select's search Input uses 7/8 of its custom control size.
            .with_size(px(self.settings.description.font_size / 0.875))
            .refine_style(&secondary_style);
        let size = NumberInput::new((ElementId::from(key), "font-size"), f64::from(typography.font_size))
            .label(format!("{title} font size in pixels"))
            .range(6., 72.)
            .step(0.1)
            .precision(1)
            .suffix(div().refine_style(&secondary_style).child("px"))
            .on_change(cx.processor(move |this, number: f64, _, cx| {
                let mut value = this.settings.typography(target).clone();
                value.font_size = number as f32;
                this.set_typography(target, value, cx);
            }));
        let weight = NumberInput::new((ElementId::from(key), "font-weight"), f64::from(typography.font_weight))
            .label(format!("{title} font weight"))
            .range(100., 900.)
            .step(1.)
            .precision(0)
            .on_change(cx.processor(move |this, number: f64, _, cx| {
                let mut value = this.settings.typography(target).clone();
                value.font_weight = number as u16;
                this.set_typography(target, value, cx);
            }));
        let line_height = NumberInput::new((ElementId::from(key), "line-height"), f64::from(typography.line_height))
            .label(format!("{title} line height ratio"))
            .range(0.8, 3.)
            .step(0.1)
            .precision(1)
            .on_change(cx.processor(move |this, number: f64, _, cx| {
                let mut value = this.settings.typography(target).clone();
                value.line_height = number as f32;
                this.set_typography(target, value, cx);
            }));
        let mut page = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .refine_style(&secondary_style)
            .child(div().refine_style(&ui_style).child(title))
            .child(div().mt(px(4.)).mb(px(8.)).refine_style(&secondary_style).text_color(muted).child(description))
            .child(self.row("Font family", "Search installed font families.", picker, cx))
            .child(self.row("Font size", "Text size in pixels, from 6 to 72.", size, cx))
            .child(self.row("Font weight", "Weight from 100 (thin) to 900 (black).", weight, cx))
            .child(self.row("Line height", "Line height as a font-size ratio, from 0.8 to 3.0.", line_height, cx));
        let panel_settings = match target {
            FontTarget::Repositories => Some(&self.settings.repository_panel),
            FontTarget::Changes => Some(&self.settings.changes_panel),
            FontTarget::Files => Some(&self.settings.file_panel),
            _ => None,
        };
        if let Some(panel_settings) = panel_settings {
            let indent = NumberInput::new((ElementId::from(key), "tree-indent-width"), f64::from(panel_settings.indent_size))
                .label(format!("{title} tree indentation in pixels"))
                .range(0., 48.)
                .step(1.)
                .precision(0)
                .suffix(div().refine_style(&secondary_style).child("px"))
                .on_change(cx.processor(move |this, number: f64, _, cx| {
                    let value = match target {
                        FontTarget::Repositories => &mut this.settings.repository_panel,
                        FontTarget::Changes => &mut this.settings.changes_panel,
                        FontTarget::Files => &mut this.settings.file_panel,
                        _ => return,
                    };
                    value.indent_size = number as f32;
                    cx.emit(SettingsChange::Navigation { target, value: *value });
                    cx.notify();
                }));
            page = page.child(self.row("Tree indentation", "Pixels per nesting level in this panel's navigation tree, from 0 to 48.", indent, cx));
        }

        div()
            .id("grove-settings")
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(24.))
            .bg(surface)
            .text_color(foreground)
            .refine_style(&secondary_style)
            .children(self.error.as_ref().map(|error| {
                div()
                    .mb(px(20.))
                    .p(px(12.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(danger)
                    .bg(danger_background)
                    .text_color(danger)
                    .refine_style(&secondary_style)
                    .child(error.clone())
            }))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(24.))
                    .child(
                        div()
                            .id("settings-categories")
                            .w(px(160.))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .role(Role::Navigation)
                            .aria_label("Settings categories")
                            .children(SECTION_LIST.into_iter().map(|(candidate, key, title, _)| {
                                Button::new((ElementId::from("settings-category"), key))
                                    .accessibility_label(title)
                                    .child(div().refine_style(&ui_style).child(title))
                                    .ghost()
                                    .selected(candidate == target)
                                    .w_full()
                                    .justify_start()
                                    .refine_style(&ui_style)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.selected = candidate;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(page),
            )
            .child(div().mt(px(20.)).min_w_0().refine_style(&secondary_style).text_color(muted).child(self.config_path.clone()))
    }
}
