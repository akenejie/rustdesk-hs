#[cfg(any(target_os = "android", target_os = "ios"))]
use hbb_common::password_security;
use hbb_common::{
    allow_err,
    bytes::Bytes,
    config::{self, keys::*, Config, LocalConfig, PeerConfig, CONNECT_TIMEOUT, RENDEZVOUS_PORT},
    directories_next,
    futures::future::join_all,
    log,
    rendezvous_proto::*,
    tokio,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use hbb_common::{
    sleep,
    tokio::{sync::mpsc, time},
};
use serde_derive::Serialize;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::process::Child;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::common::SOFTWARE_UPDATE_URL;
#[cfg(not(any(target_os = "ios")))]
use crate::ipc;

type Message = RendezvousMessage;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub type Children = Arc<Mutex<(bool, HashMap<(String, String), Child>)>>;

#[derive(Clone, Debug, Serialize)]
pub struct UiStatus {
    pub status_num: i32,
    pub key_confirmed: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub mouse_time: i64,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginDeviceInfo {
    pub os: String,
    pub r#type: String,
    pub name: String,
}

lazy_static::lazy_static! {
    static ref UI_STATUS : Arc<Mutex<UiStatus>> = Arc::new(Mutex::new(UiStatus{
        status_num: 0,
        key_confirmed: false,
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        mouse_time: 0,
        id: "".to_owned(),
    }));
    static ref ASYNC_JOB_STATUS : Arc<Mutex<String>> = Default::default();
    static ref ASYNC_HTTP_STATUS : Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    static ref IS_REMOTE_MODIFY_ENABLED_BY_CONTROL_PERMISSIONS : Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
lazy_static::lazy_static! {
    static ref OPTION_SYNCED: Arc<Mutex<bool>> = Default::default();
    static ref OPTIONS : Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(Config::get_options()));
    pub static ref SENDER : Mutex<mpsc::UnboundedSender<ipc::Data>> = Mutex::new(check_connect_status(true));
    static ref CHILDREN : Children = Default::default();
}

#[cfg(target_os = "windows")]
lazy_static::lazy_static! {
    pub static ref IS_FILE_TRANSFER_ENABLED: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
}

const INIT_ASYNC_JOB_STATUS: &str = " ";

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn get_id() -> String {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return Config::get_id();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return ipc::get_id();
}

#[inline]
pub fn goto_install() {
    allow_err!(crate::run_me(vec!["--install"]));
    std::process::exit(0);
}

#[inline]
pub fn run_without_install() {
    crate::run_me(vec!["--noinstall"]).ok();
    std::process::exit(0);
}

#[inline]
pub fn show_run_without_install() -> bool {
    let mut it = std::env::args();
    if let Some(tmp) = it.next() {
        if crate::is_setup(&tmp) {
            return it.next() == None;
        }
    }
    false
}

#[inline]
pub fn refresh_options() {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        *OPTIONS.lock().unwrap() = Config::get_options();
    }
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

#[inline]
pub fn use_texture_render() -> bool {
    #[cfg(target_os = "android")]
    return false;
    #[cfg(target_os = "ios")]
    return false;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return false;
}

#[inline]
pub fn is_option_fixed(key: &str) -> bool {
    config::OVERWRITE_DISPLAY_SETTINGS
        .read()
        .unwrap()
        .contains_key(key)
        || config::OVERWRITE_LOCAL_SETTINGS
            .read()
            .unwrap()
            .contains_key(key)
        || config::OVERWRITE_SETTINGS.read().unwrap().contains_key(key)
}

#[inline]
pub fn get_local_option(key: String) -> String {
    crate::get_local_option(&key)
}

#[inline]
pub fn get_builtin_option(key: &str) -> String {
    crate::get_builtin_option(key)
}

#[inline]
pub fn set_local_option(key: String, value: String) {
    LocalConfig::set_option(key.clone(), value);
}

/// Resolve relative avatar path (e.g. "/avatar/xxx") to absolute URL
/// by prepending the API server address.
pub fn resolve_avatar_url(avatar: String) -> String {
    let avatar = avatar.trim().to_owned();
    if avatar.starts_with('/') {
        let api_server = get_api_server();
        if !api_server.is_empty() {
            return format!("{}{}", api_server.trim_end_matches('/'), avatar);
        }
    }
    avatar
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

#[inline]
pub fn install_path() -> String {
    #[cfg(windows)]
    return crate::platform::windows::get_install_info().1;
    #[cfg(not(windows))]
    return "".to_owned();
}

#[inline]
pub fn get_socks() -> Vec<String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let s = ipc::get_socks();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let s = Config::get_socks();
    match s {
        None => Vec::new(),
        Some(s) => {
            let mut v = Vec::new();
            v.push(s.proxy);
            v.push(s.username);
            v.push(s.password);
            v
        }
    }
}

#[inline]
pub fn set_socks(proxy: String, username: String, password: String) {
    let socks = config::Socks5Server {
        proxy,
        username,
        password,
    };
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    ipc::set_socks(socks).ok();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _nat = crate::CheckTestNatType::new();
        if socks.proxy.is_empty() {
            Config::set_socks(None);
        } else {
            Config::set_socks(Some(socks));
        }
        log::info!("socks updated");
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[inline]
pub fn is_installed() -> bool {
    crate::platform::is_installed()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn is_installed() -> bool {
    false
}

#[inline]
pub fn is_share_rdp() -> bool {
    #[cfg(windows)]
    return crate::platform::windows::is_share_rdp();
    #[cfg(not(windows))]
    return false;
}

#[inline]
pub fn set_share_rdp(_enable: bool) {
    #[cfg(windows)]
    crate::platform::windows::set_share_rdp(_enable);
}

#[inline]
pub fn is_installed_lower_version() -> bool {
    #[cfg(not(windows))]
    return false;
    #[cfg(windows)]
    {
        let b = crate::platform::windows::get_reg("BuildDate");
        return crate::BUILD_DATE.cmp(&b).is_gt();
    }
}

#[inline]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn get_mouse_time() -> f64 {
    UI_STATUS.lock().unwrap().mouse_time as f64
}

#[inline]
pub fn check_mouse_time() {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let sender = SENDER.lock().unwrap();
        allow_err!(sender.send(ipc::Data::MouseMoveTime(0)));
    }
}

#[inline]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn get_connect_status() -> UiStatus {
    UI_STATUS.lock().unwrap().clone()
}

#[inline]
pub fn get_peer(id: String) -> PeerConfig {
    PeerConfig::load(&id)
}

#[inline]
pub fn get_fav() -> Vec<String> {
    LocalConfig::get_fav()
}

#[inline]
pub fn store_fav(fav: Vec<String>) {
    LocalConfig::set_fav(fav);
}

#[inline]
pub fn is_process_trusted(_prompt: bool) -> bool {
    #[cfg(target_os = "macos")]
    return crate::platform::macos::is_process_trusted(_prompt);
    #[cfg(not(target_os = "macos"))]
    return true;
}

#[inline]
pub fn is_can_screen_recording(_prompt: bool) -> bool {
    #[cfg(target_os = "macos")]
    return crate::platform::macos::is_can_screen_recording(_prompt);
    #[cfg(not(target_os = "macos"))]
    return true;
}

#[inline]
pub fn get_error() -> String {
    #[cfg(target_os = "linux")]
    {
        let dtype = crate::platform::linux::get_display_server();
        if crate::platform::linux::DISPLAY_SERVER_WAYLAND == dtype {
            return crate::server::wayland::common_get_error();
        }
        if dtype != crate::platform::linux::DISPLAY_SERVER_X11 {
            return format!(
                "{} {}, {}",
                crate::client::translate("Unsupported display server".to_owned()),
                dtype,
                crate::client::translate("x11 expected".to_owned()),
            );
        }
    }
    String::new()
}

#[inline]
pub fn is_login_wayland() -> bool {
    #[cfg(target_os = "linux")]
    return crate::platform::linux::is_login_wayland();
    #[cfg(not(target_os = "linux"))]
    return false;
}

#[inline]
pub fn current_is_wayland() -> bool {
    #[cfg(target_os = "linux")]
    return crate::platform::linux::current_is_wayland();
    #[cfg(not(target_os = "linux"))]
    return false;
}

#[inline]
pub fn get_new_version() -> String {
    (*SOFTWARE_UPDATE_URL
        .lock()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap_or(""))
    .to_string()
}

#[inline]
pub fn get_version() -> String {
    crate::VERSION.to_owned()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn get_app_name() -> String {
    crate::get_app_name()
}

#[cfg(windows)]
#[inline]
pub fn create_shortcut(_id: String) {
    crate::platform::windows::create_shortcut(&_id).ok();
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn discover() {
    log::warn!("LAN discovery disabled: no outgoing network calls");
}

#[inline]
pub fn get_lan_peers() -> Vec<HashMap<&'static str, String>> {
    config::LanPeers::load()
        .peers
        .iter()
        .map(|peer| {
            HashMap::<&str, String>::from_iter([
                ("id", peer.id.clone()),
                ("username", peer.username.clone()),
                ("hostname", peer.hostname.clone()),
                ("platform", peer.platform.clone()),
            ])
        })
        .collect()
}

#[inline]
pub fn remove_discovered(id: String) {
    let mut peers = config::LanPeers::load().peers;
    peers.retain(|x| x.id != id);
    config::LanPeers::store(&peers);
}

#[inline]
pub fn get_uuid() -> String {
    crate::encode64(hbb_common::get_uuid())
}

#[inline]
pub fn get_init_async_job_status() -> String {
    INIT_ASYNC_JOB_STATUS.to_string()
}

#[inline]
pub fn reset_async_job_status() {
    *ASYNC_JOB_STATUS.lock().unwrap() = get_init_async_job_status();
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn change_id(id: String) {
    reset_async_job_status();
    let old_id = get_id();
    std::thread::spawn(move || {
        change_id_shared(id, old_id);
    });
}

#[inline]
pub fn http_request(_url: String, _method: String, _body: Option<String>, _header: String) {
    log::warn!("http_request disabled: no outgoing network calls");
}

#[inline]
pub fn get_async_http_status(url: String) -> Option<String> {
    match ASYNC_HTTP_STATUS.lock().unwrap().get(&url) {
        None => None,
        Some(_str) => Some(_str.to_string()),
    }
}

#[inline]
pub fn post_request(_url: String, _body: String, _header: String) {
    log::warn!("post_request disabled: no outgoing network calls");
}

#[inline]

pub fn get_langs() -> String {
    use serde_json::json;
    let hide_cjk = crate::lang::cjk_ui_unavailable();
    let mut x: Vec<(&str, String)> = crate::lang::LANGS
        .iter()
        .filter(|a| !hide_cjk || !crate::lang::is_cjk_lang(a.0))
        .map(|a| (a.0, format!("{} ({})", a.1, a.0)))
        .collect();
    x.sort_by(|a, b| a.0.cmp(b.0));
    json!(x).to_string()
}

// Preserve relative paths for existing configurations and only remove accidental
// surrounding whitespace. Config values are not shell-expanded (for example, `~`).
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
#[cfg(any(windows, test))]
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

#[inline]
pub fn get_api_server() -> String {
    crate::get_api_server(
        get_option("api-server"),
        get_option("custom-rendezvous-server"),
    )
}

#[inline]
pub fn has_hwcodec() -> bool {
    // Has real hardware codec using gpu
    (cfg!(feature = "hwcodec") && cfg!(not(target_os = "ios"))) || cfg!(feature = "mediacodec")
}

#[inline]
pub fn has_vram() -> bool {
    cfg!(feature = "vram")
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[inline]
pub fn is_root() -> bool {
    crate::platform::is_root()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn is_root() -> bool {
    false
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn check_super_user_permission() -> bool {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    return crate::platform::check_super_user_permission().unwrap_or(false);
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    return true;
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn check_zombie() {
    let mut deads = Vec::new();
    loop {
        let mut lock = CHILDREN.lock().unwrap();
        let mut n = 0;
        for (id, c) in lock.1.iter_mut() {
            if let Ok(Some(_)) = c.try_wait() {
                deads.push(id.clone());
                n += 1;
            }
        }
        for ref id in deads.drain(..) {
            lock.1.remove(id);
        }
        if n > 0 {
            lock.0 = true;
        }
        drop(lock);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[inline]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn recent_sessions_updated() -> bool {
    let mut children = CHILDREN.lock().unwrap();
    if children.0 {
        children.0 = false;
        true
    } else {
        false
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn new_remote(id: String, remote_type: String, force_relay: bool) {
    let mut lock = CHILDREN.lock().unwrap();
    let mut args = vec![format!("--{}", remote_type), id.clone()];
    if force_relay {
        args.push("".to_string()); // password
        args.push("--relay".to_string());
    }
    let key = (id.clone(), remote_type.clone());
    if let Some(c) = lock.1.get_mut(&key) {
        if let Ok(Some(_)) = c.try_wait() {
            lock.1.remove(&key);
        } else {
            if remote_type == "rdp" {
                allow_err!(c.kill());
                std::thread::sleep(std::time::Duration::from_millis(30));
                c.try_wait().ok();
                lock.1.remove(&key);
            } else {
                return;
            }
        }
    }
    match crate::run_me(args) {
        Ok(child) => {
            lock.1.insert(key, child);
        }
        Err(err) => {
            log::error!("Failed to spawn remote: {}", err);
        }
    }
}

// Make sure `SENDER` is inited here.
#[inline]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn start_option_status_sync() {
    let _sender = SENDER.lock().unwrap();
}

// not call directly
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn check_connect_status(reconnect: bool) -> mpsc::UnboundedSender<ipc::Data> {
    let (tx, rx) = mpsc::unbounded_channel::<ipc::Data>();
    std::thread::spawn(move || check_connect_status_(reconnect, rx));
    tx
}

pub fn get_fingerprint() -> String {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if Config::get_key_confirmed() {
        return crate::common::pk_to_fingerprint(Config::get_key_pair().1);
    } else {
        return "".to_owned();
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return ipc::get_fingerprint();
}

#[inline]
pub fn get_login_device_info() -> LoginDeviceInfo {
    LoginDeviceInfo {
        // std::env::consts::OS is better than whoami::platform() here.
        os: std::env::consts::OS.to_owned(),
        r#type: "client".to_owned(),
        name: crate::common::hostname(),
    }
}

#[inline]
pub fn get_login_device_info_json() -> String {
    serde_json::to_string(&get_login_device_info()).unwrap_or("{}".to_string())
}

// notice: avoiding create ipc connection repeatedly,
// because windows named pipe has serious memory leak issue.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::main(flavor = "current_thread")]
async fn check_connect_status_(reconnect: bool, rx: mpsc::UnboundedReceiver<ipc::Data>) {
    let mut key_confirmed = false;
    let mut rx = rx;
    let mut mouse_time = 0;
    let mut id = "".to_owned();
    let is_cm = crate::common::is_cm();

    loop {
        if let Ok(mut c) = ipc::connect(1000, "").await {
            let mut timer = crate::rustdesk_interval(time::interval(time::Duration::from_secs(1)));
            loop {
                tokio::select! {
                    res = c.next() => {
                        match res {
                            Err(err) => {
                                log::error!("ipc connection closed: {}", err);
                                if is_cm {
                                    crate::ui_cm_interface::quit_cm();
                                }
                                break;
                            }
                            #[cfg(not(any(target_os = "android", target_os = "ios")))]
                            Ok(Some(ipc::Data::MouseMoveTime(v))) => {
                                mouse_time = v;
                                UI_STATUS.lock().unwrap().mouse_time = v;
                            }
                            Ok(Some(ipc::Data::Options(Some(v)))) => {
                                *OPTIONS.lock().unwrap() = v;
                                *OPTION_SYNCED.lock().unwrap() = true;
                            }
                            Ok(Some(ipc::Data::Config((name, Some(value))))) => {
                                if name == "id" {
                                    id = value;
                                }
                            }
                            Ok(Some(ipc::Data::OnlineStatus(Some((mut x, _c))))) => {
                                if x > 0 {
                                    x = 1
                                }
                                {
                                    key_confirmed = _c;
                                }
                                *UI_STATUS.lock().unwrap() = UiStatus {
                                    status_num: x as _,
                                    key_confirmed: _c,
                                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                                    mouse_time,
                                    id: id.clone(),
                                };
                            }
                            Ok(Some(ipc::Data::ControlPermissionsRemoteModify(v))) => {
                                *IS_REMOTE_MODIFY_ENABLED_BY_CONTROL_PERMISSIONS.lock().unwrap() = v;
                            }
                            #[cfg(target_os = "windows")]
                            Ok(Some(ipc::Data::FileTransferEnabledState(v))) => {
                                if let Some(enabled) = v {
                                    let mut lock = IS_FILE_TRANSFER_ENABLED.lock().unwrap();
                                    if *lock != v {
                                        clipboard::ContextSend::enable(enabled);
                                        *lock = v;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Some(data) = rx.recv() => {
                        allow_err!(c.send(&data).await);
                    }
                    _ = timer.tick() => {
                        c.send(&ipc::Data::OnlineStatus(None)).await.ok();
                        c.send(&ipc::Data::Options(None)).await.ok();
                        c.send(&ipc::Data::Config(("id".to_owned(), None))).await.ok();
                        c.send(&ipc::Data::Config(("temporary-password".to_owned(), None))).await.ok();
                        c.send(&ipc::Data::ControlPermissionsRemoteModify(None)).await.ok();
                        #[cfg(target_os = "windows")]
                        c.send(&ipc::Data::FileTransferEnabledState(None)).await.ok();
                    }
                }
            }
        }
        if !reconnect {
            OPTIONS
                .lock()
                .unwrap()
                .insert("ipc-closed".to_owned(), "Y".to_owned());
            break;
        }
        *UI_STATUS.lock().unwrap() = UiStatus {
            status_num: -1,
            key_confirmed,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            mouse_time,
            id: id.clone(),
        };
        sleep(1.).await;
    }
}

#[allow(dead_code)]
pub fn option_synced() -> bool {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        OPTION_SYNCED.lock().unwrap().clone()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        true
    }
}

#[cfg(target_os = "android")]
#[cfg(not(any(target_os = "ios")))]
#[tokio::main(flavor = "current_thread")]
pub(crate) async fn send_to_cm(data: &ipc::Data) {
    if let Ok(mut c) = ipc::connect(1000, "_cm").await {
        c.send(data).await.ok();
    }
}

const INVALID_FORMAT: &'static str = "Invalid format";
const UNKNOWN_ERROR: &'static str = "Unknown error";

#[inline]
#[tokio::main(flavor = "current_thread")]
pub async fn change_id_shared(id: String, old_id: String) -> String {
    let res = change_id_shared_(id, old_id).await.to_owned();
    *ASYNC_JOB_STATUS.lock().unwrap() = res.clone();
    res
}

pub async fn change_id_shared_(id: String, old_id: String) -> &'static str {
    if !hbb_common::is_valid_custom_id(&id) {
        log::debug!(
            "debugging invalid id: \"{id}\", len: {}, base64: \"{}\"",
            id.len(),
            crate::encode64(&id)
        );
        let bom = id.trim_start_matches('\u{FEFF}');
        log::debug!("bom: {}", hbb_common::is_valid_custom_id(&bom));
        return INVALID_FORMAT;
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let uuid = Bytes::from(
        hbb_common::machine_uid::get()
            .unwrap_or("".to_owned())
            .as_bytes()
            .to_vec(),
    );
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let uuid = Bytes::from(hbb_common::get_uuid());

    if uuid.is_empty() {
        log::error!("Failed to change id, uuid is_empty");
        return UNKNOWN_ERROR;
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let rendezvous_servers = crate::ipc::get_rendezvous_servers(1_000).await;
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let rendezvous_servers = Config::get_rendezvous_servers();

    let mut futs = Vec::new();
    let err: Arc<Mutex<&str>> = Default::default();
    for rendezvous_server in rendezvous_servers {
        let err = err.clone();
        let id = id.to_owned();
        let uuid = uuid.clone();
        let old_id = old_id.clone();
        futs.push(tokio::spawn(async move {
            let tmp = check_id(rendezvous_server, old_id, id, uuid).await;
            if !tmp.is_empty() {
                *err.lock().unwrap() = tmp;
            }
        }));
    }
    join_all(futs).await;
    let err = *err.lock().unwrap();
    if err.is_empty() {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        crate::ipc::set_config_async("id", id.to_owned()).await.ok();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            Config::set_key_confirmed(false);
            Config::set_id(&id);
        }
    }
    err
}

async fn check_id(
    rendezvous_server: String,
    old_id: String,
    id: String,
    uuid: Bytes,
) -> &'static str {
    if let Ok(mut socket) = hbb_common::socket_client::connect_tcp(
        crate::check_port(rendezvous_server, RENDEZVOUS_PORT),
        CONNECT_TIMEOUT,
    )
    .await
    {
        let mut msg_out = Message::new();
        msg_out.set_register_pk(RegisterPk {
            old_id,
            id,
            uuid,
            ..Default::default()
        });
        let mut ok = false;
        if socket.send(&msg_out).await.is_ok() {
            if let Some(msg_in) =
                crate::common::get_next_nonkeyexchange_msg(&mut socket, None).await
            {
                match msg_in.union {
                    Some(rendezvous_message::Union::RegisterPkResponse(rpr)) => {
                        match rpr.result.enum_value() {
                            Ok(register_pk_response::Result::OK) => {
                                ok = true;
                            }
                            Ok(register_pk_response::Result::ID_EXISTS) => {
                                return "Not available";
                            }
                            Ok(register_pk_response::Result::TOO_FREQUENT) => {
                                return "Too frequent";
                            }
                            Ok(register_pk_response::Result::NOT_SUPPORT) => {
                                return "server_not_support";
                            }
                            Ok(register_pk_response::Result::SERVER_ERROR) => {
                                return "Server error";
                            }
                            Ok(register_pk_response::Result::INVALID_ID_FORMAT) => {
                                return INVALID_FORMAT;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        if !ok {
            return UNKNOWN_ERROR;
        }
    } else {
        return "Failed to connect to rendezvous server";
    }
    ""
}

// if it's relay id, return id processed, otherwise return original id
pub fn handle_relay_id(id: &str) -> &str {
    if id.ends_with(r"\r") || id.ends_with(r"/r") {
        &id[0..id.len() - 2]
    } else {
        id
    }
}

pub fn support_remove_wallpaper() -> bool {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    return crate::platform::WallPaperRemover::support();
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    return false;
}

pub fn has_valid_2fa() -> bool {
    let raw = get_option("2fa");
    crate::auth_2fa::get_2fa(Some(raw)).is_some()
}

pub fn generate2fa() -> String {
    crate::auth_2fa::generate2fa()
}

pub fn verify2fa(code: String) -> bool {
    let res = crate::auth_2fa::verify2fa(code);
    if res {
        refresh_options();
    }
    res
}

pub fn has_valid_bot() -> bool {
    crate::auth_2fa::TelegramBot::get().map_or(false, |bot| bot.is_some())
}

pub fn verify_bot(_token: String) -> String {
    log::warn!("telegram bot disabled: no outgoing network calls");
    "".to_owned()
}

pub fn check_hwcodec() {
    #[cfg(feature = "hwcodec")]
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use std::sync::Once;
        static ONCE: Once = Once::new();

        ONCE.call_once(|| {
            if crate::platform::is_installed() {
                ipc::notify_server_to_check_hwcodec().ok();
                ipc::client_get_hwcodec_config_thread(3);
            } else {
                scrap::hwcodec::start_check_process();
            }
        })
    }
}

pub fn is_remote_modify_enabled_by_control_permissions() -> Option<bool> {
    *IS_REMOTE_MODIFY_ENABLED_BY_CONTROL_PERMISSIONS
        .lock()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::{trim_video_save_directory, validate_windows_service_video_save_directory};

    #[test]
    fn trim_configured_video_save_directory() {
        assert_eq!(
            trim_video_save_directory("  relative/recordings  "),
            Some("relative/recordings")
        );
        assert_eq!(trim_video_save_directory("  "), None);
    }

    #[test]
    fn validate_service_video_save_directory() {
        let absolute = if cfg!(windows) {
            r"C:\recordings"
        } else {
            "/recordings"
        };
        let padded = format!("  {absolute}  ");

        assert_eq!(
            validate_windows_service_video_save_directory(&padded),
            Some(absolute)
        );
        assert_eq!(
            validate_windows_service_video_save_directory("recordings"),
            None
        );
        assert_eq!(
            validate_windows_service_video_save_directory(&format!("\"{absolute}\"")),
            None
        );
        assert_eq!(validate_windows_service_video_save_directory("  "), None);
    }
}
