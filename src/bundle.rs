use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "DesktopPet";
pub const SPRITE_FILE_NAME: &str = "pet_spritesheet.png";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePaths {
    pub sprite_sheet: PathBuf,
}

pub fn resource_paths_from_executable(executable_path: &Path) -> ResourcePaths {
    let resources = resources_dir_from_executable(executable_path);
    ResourcePaths {
        sprite_sheet: resources.join(SPRITE_FILE_NAME),
    }
}

pub fn current_resource_paths() -> std::io::Result<ResourcePaths> {
    let executable = std::env::current_exe()?;
    Ok(resource_paths_from_executable(&executable))
}

fn resources_dir_from_executable(executable_path: &Path) -> PathBuf {
    if let Some(macos_dir) = executable_path.parent() {
        if macos_dir.file_name().is_some_and(|name| name == "MacOS") {
            if let Some(contents_dir) = macos_dir.parent() {
                let has_contents_dir = contents_dir
                    .file_name()
                    .is_some_and(|name| name == "Contents");
                let has_app_bundle = contents_dir
                    .parent()
                    .and_then(Path::extension)
                    .is_some_and(|extension| extension == "app");

                if has_contents_dir && has_app_bundle {
                    return contents_dir.join("Resources");
                }
            }
        }
    }

    PathBuf::from("assets")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_binary_resolves_to_contents_resources() {
        let executable = Path::new("/Applications/DesktopPet.app/Contents/MacOS/desktop-pet");
        let paths = resource_paths_from_executable(executable);
        assert_eq!(
            paths.sprite_sheet,
            PathBuf::from("/Applications/DesktopPet.app/Contents/Resources/pet_spritesheet.png")
        );
    }

    #[test]
    fn development_binary_resolves_to_assets_directory() {
        let executable = Path::new("/repo/target/debug/desktop-pet");
        let paths = resource_paths_from_executable(executable);
        assert_eq!(
            paths.sprite_sheet,
            PathBuf::from("assets/pet_spritesheet.png")
        );
    }

    #[test]
    fn app_named_parent_outside_bundle_resolves_to_assets_directory() {
        let executable = Path::new("/tmp/Foo.app/build/target/debug/desktop-pet");
        let paths = resource_paths_from_executable(executable);
        assert_eq!(
            paths.sprite_sheet,
            PathBuf::from("assets/pet_spritesheet.png")
        );
    }
}
