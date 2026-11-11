use std::path::PathBuf;

mod icon;
mod patch_view;
mod settings;
mod terminal_session;
mod ui;
mod workspace_state;

use gpui_kit::component::Theme;
use gpui_kit::component::ThemeMode;
use gpui_kit::component::TitleBar;
use gpui_kit::*;
use ui::ChangesMode;
use ui::FilesMode;
use ui::Grove;
use ui::TerminalMode;

actions!(grove, [Quit]);

fn main() {
    let root_list = match workspace_root_list() {
        Ok(root_list) => root_list,
        Err(error) => {
            eprintln!("Grove: {error}");
            std::process::exit(2);
        }
    };
    gpui_kit::application().with_assets(ely_core::Assets::before(icon::Assets)).run(move |cx| {
        gpui_kit::init(cx);
        ely_core::init(cx).expect("failed to initialize ELY components");
        ely_editor::bind_keys(cx);
        ely_terminal::bind_keys(cx);
        configure_ely(cx);
        Theme::change(ThemeMode::Light, None, cx);
        Theme::global_mut(cx).mono_font_family = grove_platform::MONO_FONT.into();
        Theme::global_mut(cx).mono_font_size = px(11.5);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new(grove_platform::QUIT_KEY, Quit, None),
            KeyBinding::new(grove_platform::MODE_KEY_LIST[0], ChangesMode, Some("Grove")),
            KeyBinding::new(grove_platform::MODE_KEY_LIST[1], FilesMode, Some("Grove")),
            KeyBinding::new(grove_platform::MODE_KEY_LIST[2], TerminalMode, Some("Grove")),
            KeyBinding::new("ctrl-`", TerminalMode, Some("Grove")),
        ]);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let mut options = TitleBar::window_options();
        options.window_bounds = Some(WindowBounds::centered(size(px(1440.), px(960.)), cx));
        options.window_min_size = Some(size(px(960.), px(620.)));
        if let Some(titlebar) = options.titlebar.as_mut() {
            titlebar.title = Some("Grove".into());
        }
        gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| Grove::new(root_list, window, cx))).expect("failed to open Grove window");
        cx.activate(true);
    });
}

fn configure_ely(cx: &mut App) {
    use ely_core::theme::Density;
    use ely_core::theme::Mode;
    use ely_core::theme::Palette;
    use ely_core::theme::Theme as ElyTheme;
    let mut palette = Palette::light(false);
    palette.bg = rgb(0xffffff).into();
    palette.surface = rgb(0xffffff).into();
    palette.sunken = rgb(0xf5f5f7).into();
    palette.border = rgb(0xe8e8ed).into();
    palette.fg = rgb(0x1d1d1f).into();
    palette.fg_muted = rgb(0x6e6e73).into();
    palette.accent = rgb(0x0071e3).into();
    palette.focus = rgb(0x0071e3).into();
    palette.selection = rgb(0xeaf2fd).into();
    ElyTheme::set_palette(Mode::Light, Some(palette), cx);
    ElyTheme::set_mode_now(Mode::Light, cx);
    let theme = cx.global_mut::<ElyTheme>();
    theme.font_family = grove_platform::SYSTEM_FONT.into();
    theme.mono_family = grove_platform::MONO_FONT.into();
    theme.font_scale = 0.96;
    theme.radius_scale = 0.5;
    theme.density = Density::Compact;
    theme.reduced_motion = true;
}

fn workspace_root_list() -> Result<Vec<PathBuf>, String> {
    let mut root_list: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if root_list.is_empty() {
        root_list.push(std::env::current_dir().map_err(|error| error.to_string())?);
    }
    for root in &mut root_list {
        *root = root.canonicalize().map_err(|error| format!("{}: {error}", root.display()))?;
        if !root.is_dir() {
            return Err(format!("{} is not a directory", root.display()));
        }
    }
    root_list.dedup();
    Ok(root_list)
}
