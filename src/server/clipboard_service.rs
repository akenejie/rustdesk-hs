use super::*;
#[cfg(not(target_os = "android"))]
use crate::clipboard::clipboard_listener;
#[cfg(not(target_os = "android"))]
pub use crate::clipboard::{ClipboardContext, ClipboardSide};
pub use crate::clipboard::{CLIPBOARD_INTERVAL as INTERVAL, CLIPBOARD_NAME as NAME};
#[cfg(feature = "unix-file-copy-paste")]
pub use crate::{
    clipboard::{check_clipboard_files, FILE_CLIPBOARD_NAME as FILE_NAME},
    clipboard_file::unix_file_clip,
};
#[cfg(all(feature = "unix-file-copy-paste", target_os = "linux"))]
use clipboard::platform::unix::fuse::{init_fuse_context, uninit_fuse_context};
#[cfg(not(target_os = "android"))]
use clipboard_master::CallbackResult;
#[cfg(target_os = "android")]
use hbb_common::config::{keys, option2bool};
#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    io,
    sync::mpsc::{channel, RecvTimeoutError},
    time::Duration,
};

#[cfg(target_os = "android")]
static CLIPBOARD_SERVICE_OK: AtomicBool = AtomicBool::new(false);

#[cfg(not(target_os = "android"))]
struct Handler {
    ctx: Option<ClipboardContext>,
}

#[cfg(target_os = "android")]
pub fn is_clipboard_service_ok() -> bool {
    CLIPBOARD_SERVICE_OK.load(Ordering::SeqCst)
}

pub fn new(name: String) -> GenericService {
    let svc = EmptyExtraFieldService::new(name, false);
    GenericService::run(&svc.clone(), run);
    svc.sp
}

#[cfg(not(target_os = "android"))]
fn run(sp: EmptyExtraFieldService) -> ResultType<()> {
    #[cfg(all(feature = "unix-file-copy-paste", target_os = "linux"))]
    let _fuse_call_on_ret = {
        if sp.name() == FILE_NAME {
            Some(init_fuse_context(false).map(|_| crate::SimpleCallOnReturn {
                b: true,
                f: Box::new(|| {
                    uninit_fuse_context(false);
                }),
            }))
        } else {
            None
        }
    };

    let (tx_cb_result, rx_cb_result) = channel();
    let ctx = Some(ClipboardContext::new().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?);
    clipboard_listener::subscribe(sp.name(), tx_cb_result)?;
    let mut handler = Handler { ctx };

    while sp.ok() {
        match rx_cb_result.recv_timeout(Duration::from_millis(INTERVAL)) {
            Ok(CallbackResult::Next) => {
                #[cfg(feature = "unix-file-copy-paste")]
                if sp.name() == FILE_NAME {
                    handler.check_clipboard_file();
                    continue;
                }
                if let Some(msg) = handler.get_clipboard_msg() {
                    sp.send(msg);
                }
            }
            Ok(CallbackResult::Stop) => {
                log::debug!("Clipboard listener stopped");
                break;
            }
            Ok(CallbackResult::StopWithError(err)) => {
                bail!("Clipboard listener stopped with error: {}", err);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                log::error!("Clipboard listener disconnected");
                break;
            }
        }
    }

    clipboard_listener::unsubscribe(&sp.name());

    Ok(())
}

#[cfg(target_os = "linux")]
const WAYLAND_CLIPBOARD_SKIP_CHECK_MAX_UTF8_BYTES: usize =
    super::input_service::WAYLAND_CLIPBOARD_INPUT_MAX_TEXT_CHARS * 4;

#[cfg(target_os = "linux")]
fn decode_utf8_prefix(bytes: &[u8]) -> Option<String> {
    let end = bytes.len().min(WAYLAND_CLIPBOARD_SKIP_CHECK_MAX_UTF8_BYTES);
    let slice = &bytes[..end];
    match std::str::from_utf8(slice) {
        Ok(text) => Some(text.to_owned()),
        Err(e) => {
            if e.error_len().is_some() {
                return None;
            }
            let valid_up_to = e.valid_up_to();
            std::str::from_utf8(&slice[..valid_up_to])
                .ok()
                .map(ToOwned::to_owned)
        }
    }
}

#[cfg(target_os = "linux")]
fn decode_text_clipboard(clipboard: &Clipboard) -> Option<String> {
    if clipboard.format.enum_value() != Ok(ClipboardFormat::Text) {
        return None;
    }
    if clipboard.compress {
        let bytes = hbb_common::compress::decompress(&clipboard.content);
        return decode_utf8_prefix(&bytes);
    }
    decode_utf8_prefix(&clipboard.content)
}

#[cfg(target_os = "linux")]
fn should_skip_wayland_clipboard_sync(msg: &Message) -> bool {
    if crate::platform::linux::is_x11() {
        return false;
    }
    let is_recent_wayland_input = |clipboard: &Clipboard| -> bool {
        let Some(text) = decode_text_clipboard(clipboard) else {
            return false;
        };
        super::input_service::is_recent_wayland_clipboard_input(&text)
    };

    match &msg.union {
        Some(message::Union::Clipboard(clipboard)) => is_recent_wayland_input(clipboard),
        Some(message::Union::MultiClipboards(multi_clipboards)) => multi_clipboards
            .clipboards
            .iter()
            .any(is_recent_wayland_input),
        _ => false,
    }
}

#[cfg(not(target_os = "android"))]
impl Handler {
    #[cfg(feature = "unix-file-copy-paste")]
    fn check_clipboard_file(&mut self) {
        if let Some(urls) = check_clipboard_files(&mut self.ctx, ClipboardSide::Host, false) {
            if !urls.is_empty() {
                #[cfg(target_os = "macos")]
                if crate::clipboard::is_file_url_set_by_rustdesk(&urls) {
                    return;
                }
                match clipboard::platform::unix::serv_files::sync_files(&urls) {
                    Ok(()) => {
                        // Use `send_data()` here to reuse `handle_file_clip()` in `connection.rs`.
                        hbb_common::allow_err!(clipboard::send_data(
                            0,
                            unix_file_clip::get_format_list()
                        ));
                    }
                    Err(e) => {
                        log::error!("Failed to sync clipboard files: {}", e);
                    }
                }
            }
        }
    }

    fn get_clipboard_msg(&mut self) -> Option<Message> {
        let msg = crate::clipboard::peek_clipboard(&mut self.ctx, ClipboardSide::Host, false)?;
        if should_skip_wayland_clipboard_sync(&msg) {
            log::debug!("Skip clipboard sync for recent Wayland keyboard injection");
            return None;
        }
        Some(msg)
    }
}

#[cfg(target_os = "android")]
fn run(sp: EmptyExtraFieldService) -> ResultType<()> {
    CLIPBOARD_SERVICE_OK.store(sp.ok(), Ordering::SeqCst);
    while sp.ok() {
        if let Some(msg) = crate::clipboard::get_clipboards_msg(false) {
            sp.send(msg);
        }
        std::thread::sleep(Duration::from_millis(INTERVAL));
    }
    CLIPBOARD_SERVICE_OK.store(false, Ordering::SeqCst);
    Ok(())
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::{decode_utf8_prefix, WAYLAND_CLIPBOARD_SKIP_CHECK_MAX_UTF8_BYTES};

    #[test]
    fn decode_utf8_prefix_returns_text_for_valid_utf8() {
        let text = "hello-مرحبا";
        assert_eq!(decode_utf8_prefix(text.as_bytes()), Some(text.to_owned()));
    }

    #[test]
    fn decode_utf8_prefix_returns_none_for_invalid_utf8_sequence() {
        let bytes = b"ab\xffcd";
        assert_eq!(decode_utf8_prefix(bytes), None);
    }

    #[test]
    fn decode_utf8_prefix_trims_incomplete_utf8_suffix() {
        let bytes = vec![b'a', 0xE4, 0xB8];
        assert_eq!(decode_utf8_prefix(&bytes), Some("a".to_owned()));
    }

    #[test]
    fn decode_utf8_prefix_applies_max_bytes_limit() {
        let bytes = vec![b'a'; WAYLAND_CLIPBOARD_SKIP_CHECK_MAX_UTF8_BYTES + 8];
        let result = decode_utf8_prefix(&bytes).expect("expected decoded prefix");
        assert_eq!(result.len(), WAYLAND_CLIPBOARD_SKIP_CHECK_MAX_UTF8_BYTES);
    }

    #[test]
    fn decode_utf8_prefix_keeps_utf8_boundary_when_limited() {
        let mut bytes = vec![b'a'; WAYLAND_CLIPBOARD_SKIP_CHECK_MAX_UTF8_BYTES - 1];
        bytes.extend_from_slice("ا".as_bytes());
        let result = decode_utf8_prefix(&bytes).expect("expected decoded prefix");
        assert_eq!(
            result.len(),
            WAYLAND_CLIPBOARD_SKIP_CHECK_MAX_UTF8_BYTES - 1
        );
        assert!(result.chars().all(|c| c == 'a'));
    }
}
