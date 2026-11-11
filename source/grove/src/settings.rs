//! Independent fonts for editor content, terminal data, Changes, Files, Repositories, and Commits.
//! Each component stores its `fontFamily` in its own JSON group under the user's
//! `~/Library/Application Support/Grove/settings.json`, outside every workspace.
//! Loading never writes; an unreadable or invalid file is preserved until a font
//! is explicitly chosen and the owner saves the new settings.
//! Data labels and metadata use their region's family; window chrome, navigation,
//! section titles, and controls keep the UI font.

use gpui_kit::App;
use gpui_kit::SharedString;
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontTarget {
    Editor,
    Terminal,
    Changes,
    Files,
    Repositories,
    Commits,
}

#[derive(Clone)]
pub struct Settings {
    pub editor_font: SharedString,
    pub terminal_font: SharedString,
    pub changes_font: SharedString,
    pub files_font: SharedString,
    pub repositories_font: SharedString,
    pub commits_font: SharedString,
}

impl Settings {
    pub fn load(cx: &App) -> (Self, Option<String>) {
        let default_family: SharedString = if cx.text_system().all_font_names().iter().any(|family| family == "Maple Mono NF CN") {
            "Maple Mono NF CN".into()
        } else {
            grove_platform::MONO_FONT.into()
        };
        let loaded = settings_path().and_then(|path| Self::load_from_path(&path, &default_family));
        match loaded {
            Ok(settings) => (settings, None),
            Err(error) => (
                Self::with_default_family(default_family),
                Some(format!(
                    "{error} Default fonts are in use. The settings file has not been changed; choosing a font will save new settings."
                )),
            ),
        }
    }

    pub fn font(&self, target: FontTarget) -> SharedString {
        match target {
            FontTarget::Editor => self.editor_font.clone(),
            FontTarget::Terminal => self.terminal_font.clone(),
            FontTarget::Changes => self.changes_font.clone(),
            FontTarget::Files => self.files_font.clone(),
            FontTarget::Repositories => self.repositories_font.clone(),
            FontTarget::Commits => self.commits_font.clone(),
        }
    }

    pub fn set_font(&mut self, target: FontTarget, family: SharedString) {
        match target {
            FontTarget::Editor => self.editor_font = family,
            FontTarget::Terminal => self.terminal_font = family,
            FontTarget::Changes => self.changes_font = family,
            FontTarget::Files => self.files_font = family,
            FontTarget::Repositories => self.repositories_font = family,
            FontTarget::Commits => self.commits_font = family,
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to_path(&settings_path()?)
    }

    fn save_to_path(&self, path: &Path) -> Result<(), String> {
        let value = serde_json::json!({
            "editor": { "fontFamily": self.editor_font.as_ref() },
            "terminal": { "fontFamily": self.terminal_font.as_ref() },
            "changes": { "fontFamily": self.changes_font.as_ref() },
            "files": { "fontFamily": self.files_font.as_ref() },
            "repositories": { "fontFamily": self.repositories_font.as_ref() },
            "commits": { "fontFamily": self.commits_font.as_ref() },
        });
        let mut content = serde_json::to_vec_pretty(&value).map_err(|error| format!("Could not encode settings: {error}."))?;
        content.push(b'\n');
        let directory = path.parent().ok_or_else(|| "The settings path has no parent directory.".to_string())?;
        fs::create_dir_all(directory).map_err(|error| format!("Could not create {}: {error}.", directory.display()))?;
        fs::write(path, content).map_err(|error| format!("Could not save {}: {error}.", path.display()))
    }

    fn with_default_family(family: SharedString) -> Self {
        Self {
            editor_font: family.clone(),
            terminal_font: family.clone(),
            changes_font: family.clone(),
            files_font: family.clone(),
            repositories_font: family.clone(),
            commits_font: family,
        }
    }

    fn load_from_path(path: &Path, default_family: &SharedString) -> Result<Self, String> {
        let content = match fs::read(path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::with_default_family(default_family.clone())),
            Err(error) => return Err(format!("Could not read {}: {error}.", path.display())),
        };
        Self::from_json(&content, default_family).map_err(|error| format!("Could not load {}: {error}.", path.display()))
    }

    fn from_json(content: &[u8], default_family: &SharedString) -> Result<Self, String> {
        let value: Value = serde_json::from_slice(content).map_err(|error| error.to_string())?;
        let root = value.as_object().ok_or_else(|| "Settings must be a JSON object".to_string())?;
        let family = |component: &str| -> Result<SharedString, String> {
            let Some(group) = root.get(component) else {
                return Ok(default_family.clone());
            };
            let group = group.as_object().ok_or_else(|| format!("{component} must be an object"))?;
            let Some(value) = group.get("fontFamily") else {
                return Ok(default_family.clone());
            };
            let name = value
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| format!("{component}.fontFamily must be a non-empty string"))?;
            Ok(name.to_string().into())
        };
        Ok(Self {
            editor_font: family("editor")?,
            terminal_font: family("terminal")?,
            changes_font: family("changes")?,
            files_font: family("files")?,
            repositories_font: family("repositories")?,
            commits_font: family("commits")?,
        })
    }
}

fn settings_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .ok_or_else(|| "Could not locate the user's home directory for Grove settings.".to_string())?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err("The user's home directory must be an absolute path to store Grove settings.".into());
    }
    Ok(home.join("Library").join("Application Support").join("Grove").join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_component_settings_preserve_choices_and_default_missing_fonts() {
        let fallback: SharedString = "Fallback family".into();
        let settings = Settings::from_json(
            br#"{"editor":{"fontFamily":"Editor family"},"terminal":{"fontFamily":"Terminal family"},"changes":{"fontFamily":"Changes family"},"files":{"fontFamily":"Files family"},"repositories":{"fontFamily":"Repositories family"}}"#,
            &fallback,
        )
        .expect("valid five-component settings");

        assert_eq!(settings.editor_font.as_ref(), "Editor family");
        assert_eq!(settings.terminal_font.as_ref(), "Terminal family");
        assert_eq!(settings.changes_font.as_ref(), "Changes family");
        assert_eq!(settings.files_font.as_ref(), "Files family");
        assert_eq!(settings.repositories_font.as_ref(), "Repositories family");
        assert_eq!(settings.commits_font, fallback);

        let partial = Settings::from_json(br#"{"editor":{"fontFamily":"Editor family"},"terminal":{},"repositories":{},"commits":{}}"#, &fallback).expect("valid partial settings");
        assert_eq!(partial.editor_font.as_ref(), "Editor family");
        assert_eq!(partial.terminal_font, fallback);
        assert_eq!(partial.changes_font, fallback);
        assert_eq!(partial.files_font, fallback);
        assert_eq!(partial.repositories_font, fallback);
        assert_eq!(partial.commits_font, fallback);
    }

    #[test]
    fn invalid_settings_shapes_and_font_values_are_rejected() {
        let fallback: SharedString = "Fallback family".into();
        let invalid_json_list = [
            "{",
            "null",
            "[]",
            r#""settings""#,
            r#"{"editor":null}"#,
            r#"{"terminal":[]}"#,
            r#"{"changes":"Font family"}"#,
            r#"{"files":{"fontFamily":""}}"#,
            r#"{"editor":{"fontFamily":" \t\n "}}"#,
            r#"{"terminal":{"fontFamily":null}}"#,
            r#"{"changes":{"fontFamily":12}}"#,
            r#"{"files":{"fontFamily":false}}"#,
            r#"{"editor":{"fontFamily":[]}}"#,
            r#"{"terminal":{"fontFamily":{}}}"#,
            r#"{"repositories":null}"#,
            r#"{"repositories":{"fontFamily":""}}"#,
            r#"{"commits":null}"#,
            r#"{"commits":{"fontFamily":""}}"#,
        ];

        for content in invalid_json_list {
            assert!(Settings::from_json(content.as_bytes(), &fallback).is_err(), "accepted invalid settings: {content}");
        }
    }

    #[test]
    fn changing_a_font_only_updates_its_component() {
        let fallback: SharedString = "Fallback family".into();
        let initial = Settings::from_json(
            br#"{"editor":{"fontFamily":"Editor family"},"terminal":{"fontFamily":"Terminal family"},"changes":{"fontFamily":"Changes family"},"files":{"fontFamily":"Files family"},"repositories":{"fontFamily":"Repositories family"},"commits":{"fontFamily":"Commits family"}}"#,
            &fallback,
        )
        .expect("valid component settings");
        let target_list = [
            FontTarget::Editor,
            FontTarget::Terminal,
            FontTarget::Changes,
            FontTarget::Files,
            FontTarget::Repositories,
            FontTarget::Commits,
        ];
        let replacement: SharedString = "Replacement family".into();
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("system clock after Unix epoch").as_nanos();
        let path = std::env::temp_dir().join(format!("grove-font-settings-{}-{unique}.json", std::process::id()));

        for target in target_list {
            let mut settings = initial.clone();
            settings.set_font(target, replacement.clone());
            settings.save_to_path(&path).expect("save component settings");
            let reloaded = Settings::load_from_path(&path, &fallback).expect("reload component settings");
            for component in target_list {
                let expected = if component == target { replacement.clone() } else { initial.font(component) };
                assert_eq!(settings.font(component), expected, "changing {target:?} affected {component:?}");
                assert_eq!(reloaded.font(component), expected, "saving {target:?} affected {component:?}");
            }
        }
        fs::remove_file(path).expect("remove temporary settings");
    }
}
