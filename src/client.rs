/// Shared login/error message constants used by the host server.
///
/// The name of this module is a legacy of upstream RustDesk where these
/// constants lived in the desktop client. This project is a headless server,
/// so the operator-side client code has been removed; only the constants and
/// helpers still needed by the host are kept here.

#[cfg(target_os = "linux")]
pub const LOGIN_MSG_DESKTOP_NOT_INITED: &str = "Desktop env is not inited";
pub const LOGIN_MSG_DESKTOP_SESSION_NOT_READY: &str = "Desktop session not ready";
pub const LOGIN_MSG_DESKTOP_XSESSION_FAILED: &str = "Desktop xsession failed";
pub const LOGIN_MSG_DESKTOP_SESSION_ANOTHER_USER: &str = "Desktop session another user login";
pub const LOGIN_MSG_DESKTOP_XORG_NOT_FOUND: &str = "Desktop xorg not found";
// ls /usr/share/xsessions/
pub const LOGIN_MSG_DESKTOP_NO_DESKTOP: &str = "Desktop none";
pub const LOGIN_MSG_DESKTOP_SESSION_NOT_READY_PASSWORD_EMPTY: &str =
    "Desktop session not ready, password empty";
pub const LOGIN_MSG_DESKTOP_SESSION_NOT_READY_PASSWORD_WRONG: &str =
    "Desktop session not ready, password wrong";
pub const LOGIN_MSG_PASSWORD_WRONG: &str = "Wrong Password";
pub const LOGIN_MSG_2FA_WRONG: &str = "Wrong 2FA Code";
pub const REQUIRE_2FA: &'static str = "2FA Required";
pub const LOGIN_MSG_OFFLINE: &str = "Offline";
pub const LOGIN_SCREEN_WAYLAND: &str = "Wayland login screen is not supported";
#[cfg(target_os = "linux")]
pub const SCRAP_UBUNTU_HIGHER_REQUIRED: &str = "ubuntu-21-04-required";
#[cfg(target_os = "linux")]
pub const SCRAP_OTHER_VERSION_OR_X11_REQUIRED: &str = "wayland-requires-higher-linux-version";
#[cfg(target_os = "linux")]
pub const SCRAP_XDP_PORTAL_UNAVAILABLE: &str = "xdp-portal-unavailable";
pub const SCRAP_X11_REQUIRED: &str = "x11 expected";
