use hbb_common::config::{keys::*, Config, LocalConfig};
use hbb_common::directories_next;
#[cfg(windows)]
use hbb_common::log;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::ipc;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

lazy_static::lazy_static! {
    static ref OPTIONS : Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(Config::get_options()));
}

#[inline]
pub fn get_option<T: AsRef<str>>(key: T) -> String {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let map = OPTIONS.lock().unwrap();
        if let Some(v) = map.get(key.as_ref()) {
            v.to_owned()
        } else {
            "".to_owned()
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Config::get_option(key.as_ref())
    }
}

// Only macOS queries the texture render flag (see `get_dst_align_rgba`).
#[cfg(target_os = "macos")]
#[inline]
pub fn use_texture_render() -> bool {
    false
}

#[inline]
pub fn get_local_option(key: String) -> String {
    crate::get_local_option(&key)
}

#[inline]
pub fn set_local_option(key: String, value: String) {
    LocalConfig::set_option(key.clone(), value);
}

#[inline]
pub fn set_option(key: String, value: String) {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let mut options = OPTIONS.lock().unwrap();
        if value.is_empty() {
            options.remove(&key);
        } else {
            options.insert(key.clone(), value.clone());
        }
        ipc::set_options(options.clone()).ok();
    }
}

#[cfg(windows)]
#[inline]
pub fn is_installed() -> bool {
    crate::platform::is_installed()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn is_installed() -> bool {
    false
}

fn trim_video_save_directory(value: &str) -> Option<&str> {
    let value = value.trim();
    if !value.is_empty() {
        Some(value)
    } else {
        None
    }
}

// A Windows service typically runs with System32 as its working directory, so
// require an absolute path to avoid resolving recordings there unexpectedly.
#[cfg(windows)]
fn validate_windows_service_video_save_directory(value: &str) -> Option<&str> {
    let value = trim_video_save_directory(value)?;
    if std::path::Path::new(value).is_absolute() {
        Some(value)
    } else {
        None
    }
}

#[inline]
pub fn video_save_directory(root: bool) -> String {
    let appname = crate::get_app_name();
    // ui process can show it correctly Once vidoe process created it.
    let try_create = |path: &std::path::Path| {
        if !path.exists() {
            std::fs::create_dir_all(path).ok();
        }
        if path.exists() {
            path.to_string_lossy().to_string()
        } else {
            "".to_string()
        }
    };

    if root {
        // Currently, only installed windows run as root
        #[cfg(windows)]
        {
            let dir = Config::get_option(OPTION_WINDOWS_SERVICE_VIDEO_SAVE_DIRECTORY);
            if let Some(dir) = validate_windows_service_video_save_directory(&dir) {
                return dir.to_owned();
            }
            if !dir.trim().is_empty() {
                log::warn!(
                    "Ignoring {OPTION_WINDOWS_SERVICE_VIDEO_SAVE_DIRECTORY}: path must be absolute"
                );
            }
            let drive = std::env::var("SystemDrive").unwrap_or("C:".to_owned());
            let dir =
                std::path::PathBuf::from(format!("{drive}\\ProgramData\\{appname}\\recording",));
            return dir.to_string_lossy().to_string();
        }
    }
    // Get directory from config file otherwise --server will use the old value from global var.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let dir = LocalConfig::get_option_from_file(OPTION_VIDEO_SAVE_DIRECTORY);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let dir = LocalConfig::get_option(OPTION_VIDEO_SAVE_DIRECTORY);
    if let Some(dir) = trim_video_save_directory(&dir) {
        return dir.to_owned();
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if let Ok(home) = config::APP_HOME_DIR.read() {
        let mut path = home.to_owned();
        path.push_str(format!("/{appname}/ScreenRecord").as_str());
        let dir = try_create(&std::path::Path::new(&path));
        if !dir.is_empty() {
            return dir;
        }
    }

    if let Some(user) = directories_next::UserDirs::new() {
        if let Some(video_dir) = user.video_dir() {
            let dir = try_create(&video_dir.join(&appname));
            if !dir.is_empty() {
                return dir;
            }
            if video_dir.exists() {
                return video_dir.to_string_lossy().to_string();
            }
        }
        if let Some(desktop_dir) = user.desktop_dir() {
            if desktop_dir.exists() {
                return desktop_dir.to_string_lossy().to_string();
            }
        }
        let home = user.home_dir();
        if home.exists() {
            return home.to_string_lossy().to_string();
        }
    }

    // same order as above
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if let Some(home) = crate::platform::get_active_user_home() {
        let name = if cfg!(target_os = "macos") {
            "Movies"
        } else {
            "Videos"
        };
        let video_dir = home.join(name);
        let dir = try_create(&video_dir.join(&appname));
        if !dir.is_empty() {
            return dir;
        }
        if video_dir.exists() {
            return video_dir.to_string_lossy().to_string();
        }
        let desktop_dir = home.join("Desktop");
        if desktop_dir.exists() {
            return desktop_dir.to_string_lossy().to_string();
        }
        if home.exists() {
            return home.to_string_lossy().to_string();
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let dir = try_create(&parent.join("videos"));
            if !dir.is_empty() {
                return dir;
            }
            // basically exist
            return parent.to_string_lossy().to_string();
        }
    }
    Default::default()
}
