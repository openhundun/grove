//! Grove 的全局配置位于 `~/.config/grove/config.json`，只使用 `HOME`，不读取 XDG 路径覆盖。
//!
//! ## 配置结构
//!
//! - `editor`、`preview`、`terminal`、`commits`、`ui`、`description` 分别设置排版。
//! - `repository_panel`、`changes_panel`、`file_panel` 同时包含排版、`indent_size`（0–48 px）和 `view_mode`（`tree` 或 `flat`）。
//! - 排版字段为 `font_family`、`font_size`（6–72 px）、`font_weight`（整数 100–900）、`line_height`（相对值 0.8–3.0）。
//! - `diff_panel.view_mode` 为 `unified` 或 `split`，`diff_panel.context_mode` 为 `collapsed` 或 `full`。
//!
//! 缺少字段时保留默认值，未知字段会报错。数据使用 Maple Mono NF CN、10 px、400、1.0；Commits 改用系统字体。
//! UI 使用系统字体、12 px、600、1.2；Description 使用系统字体、10 px、400、1.2，缺少字体时继承配置的 UI 字体。
//! 三个面板的缩进均为 16 px，Repositories 和 Files 默认 `tree`，Changes 默认 `flat`；Diff 默认 `unified`、`collapsed`。
//!
//! ## 单向迁移
//!
//! 只有新路径不存在时才读取 `~/Library/Application Support/Grove/settings.json`，迁移旧 camelCase 排版组和 `tree.indentWidth`。
//! 有效旧配置会保存到新路径一次，原文件保持不变；全局旧缩进复制给三个面板。新路径始终优先，解析失败不会回退或写文件。
//! 迁移保存失败时继续使用已加载的偏好并返回可见错误。两个文件都不存在时仅使用默认值，首次保存再创建配置文件。

use gpui_kit::App;
use gpui_kit::FontWeight;
use gpui_kit::SharedString;
use gpui_kit::StyleRefinement;
use gpui_kit::Styled;
use gpui_kit::px;
use gpui_kit::relative;
use serde_json::Map;
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::ops::RangeInclusive;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontTarget {
    Editor,
    Preview,
    Terminal,
    Changes,
    Files,
    Repositories,
    Commits,
    Ui,
    Description,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Typography {
    pub font_family: SharedString,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self { font_family: "Maple Mono NF CN".into(), font_size: 10.0, font_weight: 400, line_height: 1.0 }
    }
}

impl Typography {
    pub fn style(&self) -> StyleRefinement {
        StyleRefinement::default()
            .font_family(self.font_family.clone())
            .text_size(px(self.font_size))
            .font_weight(FontWeight(self.font_weight as f32))
            .line_height(relative(self.line_height))
    }

    pub fn row_height(&self) -> f32 {
        (self.font_size * self.line_height).max(14.0) + 2.0
    }

    fn from_json(root: &Map<String, Value>, component: &str, defaults: Self) -> Result<Self, String> {
        match json_group(root, component)? {
            Some(group) => {
                known_fields(group, component, &["font_family", "font_size", "font_weight", "line_height"])?;
                Self::from_group(group, component, defaults)
            }
            None => Ok(defaults),
        }
    }

    fn from_group(group: &Map<String, Value>, component: &str, defaults: Self) -> Result<Self, String> {
        Self::from_fields(group, component, defaults, ["font_family", "font_size", "font_weight", "line_height"])
    }

    fn from_fields(group: &Map<String, Value>, component: &str, mut defaults: Self, field_list: [&str; 4]) -> Result<Self, String> {
        let [family_field, size_field, weight_field, height_field] = field_list;
        if let Some(value) = group.get(family_field) {
            let name = value
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| format!("{component}.{family_field} must be a non-empty string"))?;
            defaults.font_family = name.to_string().into();
        }
        defaults.font_size = bounded_number(group, component, size_field, defaults.font_size, 6.0..=72.0)?;
        if let Some(value) = group.get(weight_field) {
            defaults.font_weight = value
                .as_u64()
                .filter(|weight| (100..=900).contains(weight))
                .ok_or_else(|| format!("{component}.{weight_field} must be an integer from 100 to 900"))? as u16;
        }
        defaults.line_height = bounded_number(group, component, height_field, defaults.line_height, 0.8..=3.0)?;
        Ok(defaults)
    }

    fn to_json(&self) -> Map<String, Value> {
        Map::from_iter([
            ("font_family".into(), self.font_family.as_ref().into()),
            ("font_size".into(), self.font_size.into()),
            ("font_weight".into(), self.font_weight.into()),
            ("line_height".into(), self.line_height.into()),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelViewMode {
    Tree,
    Flat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelSettings {
    pub indent_size: f32,
    pub view_mode: PanelViewMode,
}

impl Default for PanelSettings {
    fn default() -> Self {
        Self { indent_size: 16.0, view_mode: PanelViewMode::Tree }
    }
}

impl PanelSettings {
    fn from_json(root: &Map<String, Value>, component: &str, typography: Typography, mut defaults: Self) -> Result<(Typography, Self), String> {
        let Some(group) = json_group(root, component)? else {
            return Ok((typography, defaults));
        };
        known_fields(group, component, &["font_family", "font_size", "font_weight", "line_height", "indent_size", "view_mode"])?;
        defaults.indent_size = bounded_number(group, component, "indent_size", defaults.indent_size, 0.0..=48.0)?;
        if let Some(value) = group.get("view_mode") {
            defaults.view_mode = match value.as_str() {
                Some("tree") => PanelViewMode::Tree,
                Some("flat") => PanelViewMode::Flat,
                _ => return Err(format!("{component}.view_mode must be tree or flat")),
            };
        }
        Ok((Typography::from_group(group, component, typography)?, defaults))
    }

    fn to_json(self, typography: &Typography) -> Map<String, Value> {
        let mut group = typography.to_json();
        group.insert("indent_size".into(), self.indent_size.into());
        let view_mode = match self.view_mode {
            PanelViewMode::Tree => "tree",
            PanelViewMode::Flat => "flat",
        };
        group.insert("view_mode".into(), view_mode.into());
        group
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffViewMode {
    Unified,
    Split,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffContextMode {
    Collapsed,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffPanelSettings {
    pub view_mode: DiffViewMode,
    pub context_mode: DiffContextMode,
}

impl Default for DiffPanelSettings {
    fn default() -> Self {
        Self { view_mode: DiffViewMode::Unified, context_mode: DiffContextMode::Collapsed }
    }
}

impl DiffPanelSettings {
    fn from_json(root: &Map<String, Value>, mut defaults: Self) -> Result<Self, String> {
        let Some(group) = json_group(root, "diff_panel")? else {
            return Ok(defaults);
        };
        known_fields(group, "diff_panel", &["view_mode", "context_mode"])?;
        if let Some(value) = group.get("view_mode") {
            defaults.view_mode = match value.as_str() {
                Some("unified") => DiffViewMode::Unified,
                Some("split") => DiffViewMode::Split,
                _ => return Err("diff_panel.view_mode must be unified or split".into()),
            };
        }
        if let Some(value) = group.get("context_mode") {
            defaults.context_mode = match value.as_str() {
                Some("collapsed") => DiffContextMode::Collapsed,
                Some("full") => DiffContextMode::Full,
                _ => return Err("diff_panel.context_mode must be collapsed or full".into()),
            };
        }
        Ok(defaults)
    }

    fn to_json(self) -> Value {
        serde_json::json!({
            "view_mode": match self.view_mode { DiffViewMode::Unified => "unified", DiffViewMode::Split => "split" },
            "context_mode": match self.context_mode { DiffContextMode::Collapsed => "collapsed", DiffContextMode::Full => "full" },
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub editor: Typography,
    pub preview: Typography,
    pub terminal: Typography,
    pub changes: Typography,
    pub files: Typography,
    pub repositories: Typography,
    pub commits: Typography,
    pub ui: Typography,
    pub description: Typography,
    pub repository_panel: PanelSettings,
    pub changes_panel: PanelSettings,
    pub file_panel: PanelSettings,
    pub diff_panel: DiffPanelSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            editor: Typography::default(),
            preview: Typography::default(),
            terminal: Typography::default(),
            changes: Typography::default(),
            files: Typography::default(),
            repositories: Typography::default(),
            commits: Typography { font_family: grove_platform::SYSTEM_FONT.into(), ..Typography::default() },
            ui: Typography { font_family: grove_platform::SYSTEM_FONT.into(), font_size: 12.0, font_weight: 600, line_height: 1.2 },
            description: Typography { font_family: grove_platform::SYSTEM_FONT.into(), font_size: 10.0, font_weight: 400, line_height: 1.2 },
            repository_panel: PanelSettings::default(),
            changes_panel: PanelSettings { view_mode: PanelViewMode::Flat, ..PanelSettings::default() },
            file_panel: PanelSettings::default(),
            diff_panel: DiffPanelSettings::default(),
        }
    }
}

impl Settings {
    pub fn secondary_ui_style(&self) -> StyleRefinement {
        self.description.style()
    }

    pub fn load(_cx: &App) -> (Self, Option<String>) {
        let loaded = home_directory().and_then(|home| Self::load_from_paths(&config_path_for_home(&home), &legacy_path_for_home(&home)));
        match loaded {
            Ok(loaded) => loaded,
            Err(error) => (Self::default(), Some(format!("{error} Default settings are in use. No settings file has been changed."))),
        }
    }

    pub fn typography(&self, target: FontTarget) -> &Typography {
        match target {
            FontTarget::Editor => &self.editor,
            FontTarget::Preview => &self.preview,
            FontTarget::Terminal => &self.terminal,
            FontTarget::Changes => &self.changes,
            FontTarget::Files => &self.files,
            FontTarget::Repositories => &self.repositories,
            FontTarget::Commits => &self.commits,
            FontTarget::Ui => &self.ui,
            FontTarget::Description => &self.description,
        }
    }

    pub fn set_typography(&mut self, target: FontTarget, typography: Typography) {
        let current = match target {
            FontTarget::Editor => &mut self.editor,
            FontTarget::Preview => &mut self.preview,
            FontTarget::Terminal => &mut self.terminal,
            FontTarget::Changes => &mut self.changes,
            FontTarget::Files => &mut self.files,
            FontTarget::Repositories => &mut self.repositories,
            FontTarget::Commits => &mut self.commits,
            FontTarget::Ui => &mut self.ui,
            FontTarget::Description => &mut self.description,
        };
        *current = typography;
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to_path(&config_path()?)
    }

    fn save_to_path(&self, path: &Path) -> Result<(), String> {
        let value = serde_json::json!({
            "editor": self.editor.to_json(),
            "preview": self.preview.to_json(),
            "terminal": self.terminal.to_json(),
            "changes_panel": self.changes_panel.to_json(&self.changes),
            "file_panel": self.file_panel.to_json(&self.files),
            "repository_panel": self.repository_panel.to_json(&self.repositories),
            "commits": self.commits.to_json(),
            "ui": self.ui.to_json(),
            "description": self.description.to_json(),
            "diff_panel": self.diff_panel.to_json(),
        });
        let mut content = serde_json::to_vec_pretty(&value).map_err(|error| format!("Could not encode settings: {error}."))?;
        content.push(b'\n');
        let directory = path.parent().ok_or_else(|| "The settings path has no parent directory.".to_string())?;
        fs::create_dir_all(directory).map_err(|error| format!("Could not create {}: {error}.", directory.display()))?;
        fs::write(path, content).map_err(|error| format!("Could not save {}: {error}.", path.display()))
    }

    fn load_from_paths(path: &Path, legacy_path: &Path) -> Result<(Self, Option<String>), String> {
        if let Some(settings) = Self::read_from_path(path, Self::from_json)? {
            return Ok((settings, None));
        }
        let Some(settings) = Self::read_from_path(legacy_path, Self::from_legacy_json)? else {
            return Ok((Self::default(), None));
        };
        let error = settings.save_to_path(path).err().map(|error| {
            format!(
                "{error} Preferences loaded from {} remain in use. The legacy settings file has not been changed.",
                legacy_path.display()
            )
        });
        Ok((settings, error))
    }

    fn read_from_path(path: &Path, parse: fn(&[u8]) -> Result<Self, String>) -> Result<Option<Self>, String> {
        let content = match fs::read(path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Could not read {}: {error}.", path.display())),
        };
        parse(&content).map(Some).map_err(|error| format!("Could not load {}: {error}.", path.display()))
    }

    fn from_json(content: &[u8]) -> Result<Self, String> {
        let value: Value = serde_json::from_slice(content).map_err(|error| error.to_string())?;
        let root = value.as_object().ok_or_else(|| "Settings must be a JSON object".to_string())?;
        known_fields(
            root,
            "",
            &[
                "editor",
                "preview",
                "terminal",
                "commits",
                "ui",
                "description",
                "repository_panel",
                "changes_panel",
                "file_panel",
                "diff_panel",
            ],
        )?;
        let defaults = Self::default();
        let ui = Typography::from_json(root, "ui", defaults.ui)?;
        let description = Typography::from_json(root, "description", Typography { font_family: ui.font_family.clone(), ..defaults.description })?;
        let (repositories, repository_panel) = PanelSettings::from_json(root, "repository_panel", defaults.repositories, defaults.repository_panel)?;
        let (changes, changes_panel) = PanelSettings::from_json(root, "changes_panel", defaults.changes, defaults.changes_panel)?;
        let (files, file_panel) = PanelSettings::from_json(root, "file_panel", defaults.files, defaults.file_panel)?;
        Ok(Self {
            editor: Typography::from_json(root, "editor", defaults.editor)?,
            preview: Typography::from_json(root, "preview", defaults.preview)?,
            terminal: Typography::from_json(root, "terminal", defaults.terminal)?,
            changes,
            files,
            repositories,
            commits: Typography::from_json(root, "commits", defaults.commits)?,
            ui,
            description,
            repository_panel,
            changes_panel,
            file_panel,
            diff_panel: DiffPanelSettings::from_json(root, defaults.diff_panel)?,
        })
    }

    fn from_legacy_json(content: &[u8]) -> Result<Self, String> {
        let value: Value = serde_json::from_slice(content).map_err(|error| error.to_string())?;
        let root = value.as_object().ok_or_else(|| "Settings must be a JSON object".to_string())?;
        let defaults = Self::default();
        let ui = legacy_typography(root, "ui", defaults.ui)?;
        let description = Typography { font_family: ui.font_family.clone(), ..defaults.description };
        let indent_size = match json_group(root, "tree")? {
            Some(group) => bounded_number(group, "tree", "indentWidth", defaults.repository_panel.indent_size, 0.0..=48.0)?,
            None => defaults.repository_panel.indent_size,
        };
        Ok(Self {
            editor: legacy_typography(root, "editor", defaults.editor)?,
            preview: legacy_typography(root, "preview", defaults.preview)?,
            terminal: legacy_typography(root, "terminal", defaults.terminal)?,
            changes: legacy_typography(root, "changes", defaults.changes)?,
            files: legacy_typography(root, "files", defaults.files)?,
            repositories: legacy_typography(root, "repositories", defaults.repositories)?,
            commits: legacy_typography(root, "commits", defaults.commits)?,
            ui,
            description,
            repository_panel: PanelSettings { indent_size, ..defaults.repository_panel },
            changes_panel: PanelSettings { indent_size, ..defaults.changes_panel },
            file_panel: PanelSettings { indent_size, ..defaults.file_panel },
            diff_panel: defaults.diff_panel,
        })
    }
}

fn json_group<'a>(root: &'a Map<String, Value>, component: &str) -> Result<Option<&'a Map<String, Value>>, String> {
    root.get(component).map(|value| value.as_object().ok_or_else(|| format!("{component} must be an object"))).transpose()
}

fn known_fields(group: &Map<String, Value>, component: &str, field_list: &[&str]) -> Result<(), String> {
    for field in group.keys() {
        if !field_list.contains(&field.as_str()) {
            let path = if component.is_empty() { field.clone() } else { format!("{component}.{field}") };
            return Err(format!("Unknown configuration field {path}"));
        }
    }
    Ok(())
}

fn legacy_typography(root: &Map<String, Value>, component: &str, defaults: Typography) -> Result<Typography, String> {
    match json_group(root, component)? {
        Some(group) => Typography::from_fields(group, component, defaults, ["fontFamily", "fontSize", "fontWeight", "lineHeight"]),
        None => Ok(defaults),
    }
}

fn bounded_number(group: &Map<String, Value>, component: &str, field: &str, default: f32, bounds: RangeInclusive<f64>) -> Result<f32, String> {
    let Some(value) = group.get(field) else {
        return Ok(default);
    };
    value
        .as_f64()
        .filter(|number| number.is_finite() && bounds.contains(number))
        .map(|number| number as f32)
        .ok_or_else(|| format!("{component}.{field} must be a finite number from {} to {}", bounds.start(), bounds.end()))
}

pub fn config_path() -> Result<PathBuf, String> {
    Ok(config_path_for_home(&home_directory()?))
}

fn config_path_for_home(home: &Path) -> PathBuf {
    home.join(".config/grove/config.json")
}

fn legacy_path_for_home(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Grove/settings.json")
}

fn home_directory() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .ok_or_else(|| "Could not locate the user's home directory for Grove settings.".to_string())?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err("The user's home directory must be an absolute path to store Grove settings.".into());
    }
    Ok(home)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestPaths {
        home: PathBuf,
        config: PathBuf,
        legacy: PathBuf,
    }
    impl TestPaths {
        fn new() -> Self {
            static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("system clock after Unix epoch").as_nanos();
            let sequence = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let home = std::env::temp_dir().join(format!("grove-settings-{}-{unique}-{sequence}", std::process::id()));
            fs::create_dir(&home).expect("create isolated settings directory");
            Self { config: config_path_for_home(&home), legacy: legacy_path_for_home(&home), home }
        }
    }
    impl Drop for TestPaths {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.home).expect("remove isolated settings directory");
        }
    }
    fn write_fixture(path: &Path, content: &[u8]) {
        fs::create_dir_all(path.parent().expect("fixture parent directory")).expect("create fixture directory");
        fs::write(path, content).expect("write settings fixture");
    }
    #[test]
    fn canonical_partial_groups_preserve_explicit_typography_and_defaults() {
        let settings = Settings::from_json(
            br#"
            {
                "editor": {"font_family": "  Unavailable user font  ", "font_size": 17.5},
                "preview": {"font_weight": 650},
                "terminal": {},
                "changes_panel": {"font_family": "Changes family"},
                "file_panel": {"font_family": "Files family"},
                "repository_panel": {"font_family": "Repositories family"},
                "ui": {"font_family": "Custom UI", "line_height": 1.75},
                "description": {"font_size": 11.5}
            }
            "#,
        )
        .expect("valid partial canonical settings");
        assert_eq!(
            settings.editor,
            Typography { font_family: "  Unavailable user font  ".into(), font_size: 17.5, font_weight: 400, line_height: 1.0 }
        );
        assert_eq!(settings.preview, Typography { font_weight: 650, ..Typography::default() });
        assert_eq!(settings.terminal, Typography::default());
        for (target, family) in [
            (FontTarget::Changes, "Changes family"),
            (FontTarget::Files, "Files family"),
            (FontTarget::Repositories, "Repositories family"),
        ] {
            assert_eq!(settings.typography(target), &Typography { font_family: family.into(), ..Typography::default() });
        }
        assert_eq!(
            settings.commits,
            Typography { font_family: grove_platform::SYSTEM_FONT.into(), font_size: 10.0, font_weight: 400, line_height: 1.0 }
        );
        assert_eq!(settings.ui, Typography { font_family: "Custom UI".into(), font_size: 12.0, font_weight: 600, line_height: 1.75 });
        assert_eq!(
            settings.typography(FontTarget::Description),
            &Typography { font_family: "Custom UI".into(), font_size: 11.5, font_weight: 400, line_height: 1.2 }
        );
        assert_eq!(settings.repository_panel, PanelSettings { indent_size: 16.0, view_mode: PanelViewMode::Tree });
        assert_eq!(settings.changes_panel, PanelSettings { indent_size: 16.0, view_mode: PanelViewMode::Flat });
        assert_eq!(settings.file_panel, PanelSettings { indent_size: 16.0, view_mode: PanelViewMode::Tree });
        assert_eq!(settings.diff_panel, DiffPanelSettings { view_mode: DiffViewMode::Unified, context_mode: DiffContextMode::Collapsed });
        assert_eq!(
            Settings::default().description,
            Typography { font_family: grove_platform::SYSTEM_FONT.into(), font_size: 10.0, font_weight: 400, line_height: 1.2 }
        );
    }
    #[test]
    fn malformed_shapes_and_invalid_typography_or_panel_values_are_rejected() {
        let invalid_json_list = [
            "{",
            "null",
            "[]",
            r#""settings""#,
            r#"{"editor":null}"#,
            r#"{"terminal":[]}"#,
            r#"{"changes_panel":"Font family"}"#,
            r#"{"file_panel":{"font_family":""}}"#,
            r#"{"editor":{"font_family":" \t\n "}}"#,
            r#"{"terminal":{"font_family":null}}"#,
            r#"{"changes_panel":{"font_family":12}}"#,
            r#"{"file_panel":{"font_family":false}}"#,
            r#"{"editor":{"font_family":[]}}"#,
            r#"{"terminal":{"font_family":{}}}"#,
            r#"{"repository_panel":null}"#,
            r#"{"commits":{"font_family":""}}"#,
            r#"{"preview":false}"#,
            r#"{"ui":{"font_family":null}}"#,
            r#"{"description":null}"#,
            r#"{"description":{"font_family":""}}"#,
            r#"{"diff_panel":null}"#,
            r#"{"diff_panel":[]}"#,
            r#"{"editor":{"fontFamily":"Legacy family"}}"#,
            r#"{"files":{"fontFamily":"Legacy family"}}"#,
            r#"{"tree":{"indentWidth":16}}"#,
            r#"{"editor":{"tab_size":4}}"#,
            r#"{"repository_panel":{"viewMode":"flat"}}"#,
            r#"{"diff_panel":{"contextMode":"full"}}"#,
        ];
        for content in invalid_json_list {
            assert!(Settings::from_json(content.as_bytes()).is_err(), "accepted invalid settings: {content}");
        }
        for (group, field, invalid_value_list) in [
            ("preview", "font_size", &["5.999", "72.001", "-1", "1e400", "null", "\"10\"", "false", "[]"][..]),
            ("ui", "font_weight", &["99", "901", "400.5", "-100", "1e400", "null", "\"400\"", "{}"]),
            ("commits", "line_height", &["0.7999", "3.0001", "-1", "1e400", "null", "\"1\"", "false", "[]"]),
            ("repository_panel", "indent_size", &["-0.001", "48.001", "1e400", "null", "\"16\"", "false", "[]", "{}"]),
            ("changes_panel", "indent_size", &["-1", "49", "null"]),
            ("file_panel", "indent_size", &["-1", "49", "null"]),
            ("repository_panel", "view_mode", &["\"split\"", "\"TREE\"", "null", "1", "false", "[]", "{}"]),
            ("changes_panel", "view_mode", &["\"unified\"", "null"]),
            ("file_panel", "view_mode", &["\"collapsed\"", "null"]),
            ("diff_panel", "view_mode", &["\"tree\"", "\"SPLIT\"", "null", "1", "false", "[]", "{}"]),
            ("diff_panel", "context_mode", &["\"flat\"", "\"FULL\"", "null", "1", "false", "[]", "{}"]),
        ] {
            for value in invalid_value_list {
                let content = format!("{{\"{group}\":{{\"{field}\":{value}}}}}");
                assert!(Settings::from_json(content.as_bytes()).is_err(), "accepted invalid settings: {content}");
            }
        }
        for (size, weight, height, indent) in [(6.0, 100, 0.8, 0.0), (72.0, 900, 3.0, 48.0)] {
            let content = format!("{{\"preview\":{{\"font_size\":{size},\"font_weight\":{weight},\"line_height\":{height}}},\"file_panel\":{{\"indent_size\":{indent}}}}}");
            let settings = Settings::from_json(content.as_bytes()).expect("inclusive numeric boundaries");
            assert_eq!(settings.preview.font_size, size);
            assert_eq!(settings.preview.font_weight, weight);
            assert_eq!(settings.preview.line_height, height);
            assert_eq!(settings.file_panel.indent_size, indent);
        }
    }
    #[test]
    fn independent_typography_and_panel_modes_survive_file_roundtrip() {
        let initial = Settings::from_json(
            br#"
            {
                "editor": {"font_family": "Editor family", "font_size": 12.5, "font_weight": 450, "line_height": 1.1},
                "preview": {"font_family": "Preview family", "font_size": 16, "font_weight": 500, "line_height": 1.6},
                "terminal": {"font_family": "Terminal family", "font_size": 11, "font_weight": 350, "line_height": 1.2},
                "changes_panel": {"font_family": "Changes family", "font_size": 13, "font_weight": 550, "line_height": 1.3, "indent_size": 25.5, "view_mode": "tree"},
                "file_panel": {"font_family": "Files family", "font_size": 14, "font_weight": 600, "line_height": 1.4, "indent_size": 31.5, "view_mode": "flat"},
                "repository_panel": {"font_family": "Repositories family", "font_size": 15, "font_weight": 650, "line_height": 1.5, "indent_size": 19.5, "view_mode": "flat"},
                "commits": {"font_family": "Commits family", "font_size": 17, "font_weight": 700, "line_height": 1.7},
                "ui": {"font_family": "UI family", "font_size": 18, "font_weight": 800, "line_height": 1.8},
                "description": {"font_family": "Description family", "font_size": 9.5, "font_weight": 350, "line_height": 1.5},
                "diff_panel": {"view_mode": "split", "context_mode": "full"}
            }
            "#,
        )
        .expect("valid independent canonical preferences");
        let target_list = [
            FontTarget::Editor,
            FontTarget::Preview,
            FontTarget::Terminal,
            FontTarget::Changes,
            FontTarget::Files,
            FontTarget::Repositories,
            FontTarget::Commits,
            FontTarget::Ui,
            FontTarget::Description,
        ];
        let replacement = Typography { font_family: "Replacement family".into(), font_size: 21.5, font_weight: 750, line_height: 2.25 };
        let path_list = TestPaths::new();
        assert_eq!(initial.repository_panel, PanelSettings { indent_size: 19.5, view_mode: PanelViewMode::Flat });
        assert_eq!(initial.changes_panel, PanelSettings { indent_size: 25.5, view_mode: PanelViewMode::Tree });
        assert_eq!(initial.file_panel, PanelSettings { indent_size: 31.5, view_mode: PanelViewMode::Flat });
        assert_eq!(initial.diff_panel, DiffPanelSettings { view_mode: DiffViewMode::Split, context_mode: DiffContextMode::Full });
        for target in target_list {
            let mut settings = initial.clone();
            settings.set_typography(target, replacement.clone());
            settings.save_to_path(&path_list.config).expect("save independent preferences");
            let (reloaded, error) = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect("reload independent preferences");
            assert_eq!(error, None);
            assert_eq!(reloaded, settings);
            for component in target_list {
                let expected = if component == target { &replacement } else { initial.typography(component) };
                assert_eq!(reloaded.typography(component), expected, "saving {target:?} affected {component:?}");
            }
        }
        for (view_mode, indent_size, diff_view_mode, context_mode) in [
            (PanelViewMode::Tree, 0.0, DiffViewMode::Unified, DiffContextMode::Collapsed),
            (PanelViewMode::Flat, 48.0, DiffViewMode::Split, DiffContextMode::Full),
        ] {
            let mut settings = initial.clone();
            settings.repository_panel = PanelSettings { indent_size, view_mode };
            settings.changes_panel = PanelSettings { indent_size, view_mode };
            settings.file_panel = PanelSettings { indent_size, view_mode };
            settings.diff_panel = DiffPanelSettings { view_mode: diff_view_mode, context_mode };
            settings.save_to_path(&path_list.config).expect("save panel preferences");
            let (reloaded, error) = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect("reload panel preferences");
            assert_eq!(error, None);
            assert_eq!(reloaded, settings);
        }
        assert!(!path_list.legacy.exists());
    }
    #[test]
    fn legacy_migration_preserves_all_explicit_preferences_and_source() {
        let path_list = TestPaths::new();
        let content = br#"
        {
            "editor": {"fontFamily": "Editor family", "fontSize": 12.5, "fontWeight": 450, "lineHeight": 1.1},
            "preview": {"fontFamily": "Preview family", "fontSize": 16, "fontWeight": 500, "lineHeight": 1.6},
            "terminal": {"fontFamily": "Terminal family", "fontSize": 11, "fontWeight": 350, "lineHeight": 1.2},
            "changes": {"fontFamily": "Changes family", "fontSize": 13, "fontWeight": 550, "lineHeight": 1.3},
            "files": {"fontFamily": "Files family", "fontSize": 14, "fontWeight": 600, "lineHeight": 1.4},
            "repositories": {"fontFamily": "Repositories family", "fontSize": 15, "fontWeight": 650, "lineHeight": 1.5},
            "commits": {"fontFamily": "Commits family", "fontSize": 17, "fontWeight": 700, "lineHeight": 1.7},
            "ui": {"fontFamily": "UI family", "fontSize": 18, "fontWeight": 800, "lineHeight": 1.8},
            "tree": {"indentWidth": 31.5}
        }
        "#;
        write_fixture(&path_list.legacy, content);
        let (settings, error) = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect("migrate valid legacy preferences");
        assert_eq!(error, None);
        for (target, family, size, weight, height) in [
            (FontTarget::Editor, "Editor family", 12.5, 450, 1.1),
            (FontTarget::Preview, "Preview family", 16.0, 500, 1.6),
            (FontTarget::Terminal, "Terminal family", 11.0, 350, 1.2),
            (FontTarget::Changes, "Changes family", 13.0, 550, 1.3),
            (FontTarget::Files, "Files family", 14.0, 600, 1.4),
            (FontTarget::Repositories, "Repositories family", 15.0, 650, 1.5),
            (FontTarget::Commits, "Commits family", 17.0, 700, 1.7),
            (FontTarget::Ui, "UI family", 18.0, 800, 1.8),
        ] {
            assert_eq!(
                settings.typography(target),
                &Typography { font_family: family.into(), font_size: size, font_weight: weight, line_height: height }
            );
        }
        assert_eq!(
            settings.description,
            Typography { font_family: "UI family".into(), font_size: 10.0, font_weight: 400, line_height: 1.2 }
        );
        assert_eq!(settings.repository_panel, PanelSettings { indent_size: 31.5, view_mode: PanelViewMode::Tree });
        assert_eq!(settings.changes_panel, PanelSettings { indent_size: 31.5, view_mode: PanelViewMode::Flat });
        assert_eq!(settings.file_panel, PanelSettings { indent_size: 31.5, view_mode: PanelViewMode::Tree });
        assert_eq!(settings.diff_panel, DiffPanelSettings { view_mode: DiffViewMode::Unified, context_mode: DiffContextMode::Collapsed });
        assert_eq!(fs::read(&path_list.legacy).expect("read preserved legacy file"), content);
        let canonical_content = fs::read(&path_list.config).expect("read migrated canonical file");
        let canonical: Value = serde_json::from_slice(&canonical_content).expect("canonical JSON");
        assert_eq!(canonical["changes_panel"]["font_family"], "Changes family");
        assert_eq!(canonical["file_panel"]["indent_size"], 31.5);
        assert_eq!(canonical["repository_panel"]["view_mode"], "tree");
        assert!(canonical.get("changes").is_none());
        assert!(canonical.get("files").is_none());
        assert!(canonical.get("repositories").is_none());
        assert!(canonical.get("tree").is_none());
        assert!(canonical["editor"].get("fontFamily").is_none());
        let (reloaded, error) = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect("reload migrated preferences");
        assert_eq!(error, None);
        assert_eq!(reloaded, settings);
        assert_eq!(fs::read(&path_list.config).expect("read canonical file after reload"), canonical_content);
        assert_eq!(fs::read(&path_list.legacy).expect("read legacy file after reload"), content);
    }
    #[test]
    fn legacy_font_only_and_partial_groups_keep_missing_defaults() {
        let path_list = TestPaths::new();
        write_fixture(
            &path_list.legacy,
            br#"{"editor":{"fontFamily":"Editor family"},"preview":{"fontWeight":650},"terminal":{},"ui":{"fontFamily":"Custom UI","lineHeight":1.75},"tree":{}}"#,
        );
        let (settings, error) = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect("migrate partial legacy preferences");
        assert_eq!(error, None);
        assert_eq!(settings.editor, Typography { font_family: "Editor family".into(), ..Typography::default() });
        assert_eq!(settings.preview, Typography { font_weight: 650, ..Typography::default() });
        assert_eq!(settings.terminal, Typography::default());
        assert_eq!(settings.commits, Settings::default().commits);
        assert_eq!(settings.ui, Typography { font_family: "Custom UI".into(), font_size: 12.0, font_weight: 600, line_height: 1.75 });
        assert_eq!(
            settings.description,
            Typography { font_family: "Custom UI".into(), font_size: 10.0, font_weight: 400, line_height: 1.2 }
        );
        assert_eq!(settings.repository_panel.indent_size, 16.0);
        assert_eq!(settings.changes_panel.indent_size, 16.0);
        assert_eq!(settings.file_panel.indent_size, 16.0);
    }
    #[test]
    fn canonical_precedence_ignores_invalid_legacy_and_preserves_files() {
        let path_list = TestPaths::new();
        let canonical = br#"{"editor":{"font_family":"Canonical family"},"ui":{"font_family":"Canonical UI"},"diff_panel":{"view_mode":"split","context_mode":"full"}}"#;
        let legacy = br#"{"editor":{"fontFamily":"Legacy family","fontSize":2}}"#;
        write_fixture(&path_list.config, canonical);
        write_fixture(&path_list.legacy, legacy);
        let (settings, error) = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect("load canonical preferences before invalid legacy");
        assert_eq!(error, None);
        assert_eq!(settings.editor.font_family.as_ref(), "Canonical family");
        assert_eq!(settings.description.font_family.as_ref(), "Canonical UI");
        assert_eq!(settings.diff_panel, DiffPanelSettings { view_mode: DiffViewMode::Split, context_mode: DiffContextMode::Full });
        assert_eq!(fs::read(&path_list.config).expect("read preserved canonical file"), canonical);
        assert_eq!(fs::read(&path_list.legacy).expect("read untouched legacy file"), legacy);
    }
    #[test]
    fn invalid_canonical_configuration_never_falls_back_or_writes() {
        let path_list = TestPaths::new();
        let legacy = br#"{"editor":{"fontFamily":"Legacy family","fontSize":20}}"#;
        write_fixture(&path_list.legacy, legacy);
        for canonical in [
            br#"{"editor":{"font_family":"Keep me","font_size":2}}"#.as_slice(),
            br#"{"editor":{"fontFamily":"Old schema at new path"}}"#.as_slice(),
        ] {
            write_fixture(&path_list.config, canonical);
            let error = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect_err("invalid canonical preferences must stop migration");
            assert!(error.contains(&path_list.config.display().to_string()));
            assert_eq!(fs::read(&path_list.config).expect("read preserved invalid canonical file"), canonical);
            assert_eq!(fs::read(&path_list.legacy).expect("read untouched legacy file"), legacy);
        }
    }
    #[test]
    fn invalid_legacy_configuration_does_not_create_canonical_or_change_source() {
        let path_list = TestPaths::new();
        let legacy = br#"{"editor":{"fontFamily":"Keep me"},"tree":{"indentWidth":49}}"#;
        write_fixture(&path_list.legacy, legacy);
        let error = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect_err("invalid legacy preferences must not migrate");
        assert!(error.contains(&path_list.legacy.display().to_string()));
        assert!(error.contains("tree.indentWidth"));
        assert!(!path_list.config.exists());
        assert_eq!(fs::read(&path_list.legacy).expect("read preserved invalid legacy file"), legacy);
    }
    #[cfg(unix)]
    #[test]
    fn legacy_preferences_remain_loaded_when_canonical_directory_cannot_be_created() {
        let path_list = TestPaths::new();
        let legacy = br#"{"editor":{"fontFamily":"Loaded family","fontSize":21},"tree":{"indentWidth":24}}"#;
        write_fixture(&path_list.legacy, legacy);
        std::os::unix::fs::symlink("missing-config-directory", path_list.home.join(".config")).expect("create dangling configuration-directory symlink");
        let (settings, error) = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect("keep preferences when migration save fails");
        assert_eq!(settings.editor.font_family.as_ref(), "Loaded family");
        assert_eq!(settings.editor.font_size, 21.0);
        assert_eq!(settings.repository_panel.indent_size, 24.0);
        assert_eq!(settings.changes_panel.indent_size, 24.0);
        assert_eq!(settings.file_panel.indent_size, 24.0);
        let error = error.expect("surface migration save failure");
        assert!(error.contains("Could not create"));
        assert!(error.contains("remain in use"));
        assert!(!path_list.config.exists());
        assert_eq!(fs::read(&path_list.legacy).expect("read preserved legacy file"), legacy);
        assert_eq!(
            fs::read_link(path_list.home.join(".config")).expect("read preserved configuration symlink"),
            PathBuf::from("missing-config-directory")
        );
    }
    #[test]
    fn missing_configuration_uses_defaults_without_creating_files() {
        let path_list = TestPaths::new();
        let (settings, error) = Settings::load_from_paths(&path_list.config, &path_list.legacy).expect("start without settings files");
        assert_eq!(settings, Settings::default());
        assert_eq!(error, None);
        assert_eq!(path_list.config, path_list.home.join(".config/grove/config.json"));
        assert!(!path_list.config.exists());
        assert!(!path_list.legacy.exists());
        assert_eq!(fs::read_dir(&path_list.home).expect("read untouched home fixture").count(), 0);
    }
}
