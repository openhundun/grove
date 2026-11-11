//! Lucide action glyphs and offline Material file and folder images.
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::*;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

#[derive(rust_embed::RustEmbed)]
#[folder = "$GROVE_MATERIAL_ICON_DIR"]
#[include = "**/*.svg"]
#[prefix = "material/icons/"]
struct Embedded;

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(asset) = Embedded::get(path) {
            return Ok(Some(asset.data));
        }
        gpui_kit::assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut name_list: Vec<SharedString> = Embedded::iter().filter(|name| name.starts_with(path)).map(Into::into).collect();
        name_list.extend(gpui_kit::assets::AllAssets.list(path)?);
        Ok(name_list)
    }
}

pub fn view(name: IconName, size: f32, color: u32) -> AnyElement {
    Icon::new(name).size(px(size)).text_color(rgb(color)).into_any_element()
}

struct Theme {
    file: SharedString,
    folder: SharedString,
    expanded: SharedString,
    root: SharedString,
    root_expanded: SharedString,
    file_name_map: HashMap<String, SharedString>,
    extension_map: HashMap<String, SharedString>,
    folder_map: HashMap<String, SharedString>,
    expanded_map: HashMap<String, SharedString>,
}

static THEME: LazyLock<Theme> = LazyLock::new(|| {
    let manifest: Value = serde_json::from_str(include_str!(env!("GROVE_MATERIAL_MANIFEST_PATH"))).expect("bundled icon manifest is valid");
    let definition_map = manifest["iconDefinitions"].as_object().expect("icon definitions exist");
    let resolve = |id: &str| -> SharedString {
        let source = definition_map[id]["iconPath"].as_str().expect("SVG icon has a path");
        let filename = source.rsplit('/').next().expect("SVG filename");
        format!("material/icons/{filename}").into()
    };
    let associations = |name: &str| {
        let mut map = HashMap::new();
        for layer in [&manifest, &manifest["light"]] {
            if let Some(associations) = layer[name].as_object() {
                for (key, value) in associations {
                    if let Some(id) = value.as_str() {
                        map.insert(key.to_ascii_lowercase(), resolve(id));
                    }
                }
            }
        }
        map
    };
    Theme {
        file: resolve(manifest["file"].as_str().unwrap()),
        folder: resolve(manifest["folder"].as_str().unwrap()),
        expanded: resolve(manifest["folderExpanded"].as_str().unwrap()),
        root: resolve(manifest["rootFolder"].as_str().unwrap()),
        root_expanded: resolve(manifest["rootFolderExpanded"].as_str().unwrap()),
        file_name_map: associations("fileNames"),
        extension_map: associations("fileExtensions"),
        folder_map: associations("folderNames"),
        expanded_map: associations("folderNamesExpanded"),
    }
});

fn association<'a>(map: &'a HashMap<String, SharedString>, name: &str, parent: &str) -> Option<&'a SharedString> {
    map.get(&format!("{parent}/{name}")).or_else(|| map.get(name))
}

pub fn file(path: &Path, size: f32) -> AnyElement {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let parent = path.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let theme = &*THEME;
    let mut icon = association(&theme.file_name_map, &name, &parent);
    if icon.is_none() {
        for (offset, _) in name.match_indices('.') {
            let extension = &name[offset + 1..];
            if let Some(matched) = association(&theme.extension_map, extension, &parent) {
                icon = Some(matched);
                break;
            }
        }
    }
    image(icon.unwrap_or(&theme.file).clone(), size)
}

pub fn folder(path: &Path, open: bool, root: bool, size: f32) -> AnyElement {
    let theme = &*THEME;
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let parent = path.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let default = match (root, open) {
        (true, true) => &theme.root_expanded,
        (true, false) => &theme.root,
        (false, true) => &theme.expanded,
        (false, false) => &theme.folder,
    };
    let map = if open { &theme.expanded_map } else { &theme.folder_map };
    image(association(map, &name, &parent).unwrap_or(default).clone(), size)
}

fn image(path: SharedString, size: f32) -> AnyElement {
    img(path).size(px(size)).flex_shrink_0().into_any_element()
}
