use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex, RwLock, Weak},
    time::Duration,
};

use bytes::Bytes;

pub use connection::*;
use hbb_common::tcp::{self, new_listener};
use hbb_common::{
    allow_err,
    anyhow::Context,
    bail,
    config::{Config, CONNECT_TIMEOUT, RELAY_PORT},
    log,
    message_proto::*,
    protobuf::{Enum, Message as _},
    rendezvous_proto::*,
    socket_client,
    sodiumoxide::crypto::{box_, sign},
    timeout, tokio, ResultType, Stream,
};
use scrap::camera;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use service::ServiceTmpl;
use service::{EmptyExtraFieldService, GenericService, Service, Subscriber};
use video_service::VideoSource;

use crate::ipc::Data;

#[cfg(target_os = "windows")]
pub mod terminal_helper;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod terminal_service;
cfg_if::cfg_if! {
if #[cfg(not(target_os = "ios"))] {
mod clipboard_service;
#[cfg(target_os = "android")]
pub use clipboard_service::is_clipboard_service_ok;
#[cfg(target_os = "linux")]
pub(crate) mod wayland;
#[cfg(target_os = "linux")]
pub mod uinput;
#[cfg(target_os = "linux")]
pub mod rdp_input;
#[cfg(target_os = "linux")]
pub mod dbus;
#[cfg(not(target_os = "android"))]
pub mod input_service;
} else {
mod clipboard_service {
pub const NAME: &'static str = "";
}
}
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod input_service {
    pub const NAME_CURSOR: &'static str = "";
    pub const NAME_POS: &'static str = "";
    pub const NAME_WINDOW_FOCUS: &'static str = "";
}

mod connection;
mod login_failure_check;
pub mod display_service;
#[cfg(windows)]
pub mod portable_service;
mod service;
mod video_qos;
pub mod video_service;

pub type Childs = Arc<Mutex<Vec<std::process::Child>>>;
type ConnMap = HashMap<i32, ConnInner>;

#[derive(Clone, Default)]
pub struct ConnectionMeta {
    pub control_permissions: Option<ControlPermissions>,
    pub controlled_context: Option<ControlledContext>,
}

lazy_static::lazy_static! {
    pub static ref CHILD_PROCESS: Childs = Default::default();
    // A client server used to provide local services(audio, video, clipboard, etc.)
    // for all initiative connections.
    //
    // [Note]
    // ugly
    // Now we use this [`CLIENT_SERVER`] to do following operations:
    // - record local audio, and send to remote
    pub static ref CLIENT_SERVER: ServerPtr = new();
}

pub struct Server {
    connections: ConnMap,
    services: HashMap<String, Box<dyn Service>>,
    id_count: i32,
}

pub type ServerPtr = Arc<RwLock<Server>>;
pub type ServerPtrWeak = Weak<RwLock<Server>>;

pub fn new() -> ServerPtr {
    let mut server = Server {
        connections: HashMap::new(),
        services: HashMap::new(),
        id_count: hbb_common::rand::random::<i32>() % 1000 + 1000, // ensure positive
    };
    #[cfg(not(target_os = "ios"))]
    {
        server.add_service(Box::new(display_service::new()));
        server.add_service(Box::new(clipboard_service::new(
            clipboard_service::NAME.to_owned(),
        )));
        #[cfg(feature = "unix-file-copy-paste")]
        server.add_service(Box::new(clipboard_service::new(
            clipboard_service::FILE_NAME.to_owned(),
        )));
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if !display_service::capture_cursor_embedded() {
            server.add_service(Box::new(input_service::new_cursor()));
            server.add_service(Box::new(input_service::new_pos()));
            #[cfg(target_os = "linux")]
            if scrap::is_x11() {
                // wayland does not support multiple displays currently
                server.add_service(Box::new(input_service::new_window_focus()));
            }
            #[cfg(not(target_os = "linux"))]
            server.add_service(Box::new(input_service::new_window_focus()));
        }
    }
    // Terminal service is created per connection, not globally
    Arc::new(RwLock::new(server))
}

async fn accept_connection_(
    server: ServerPtr,
    socket: Stream,
    secure: bool,
    meta: ConnectionMeta,
) -> ResultType<()> {
    let local_addr = socket.local_addr();
    drop(socket);
    // even we drop socket, below still may fail if not use reuse_addr,
    // there is TIME_WAIT before socket really released, so sometimes we
    // see "Only one usage of each socket address is normally permitted" on windows sometimes,
    let listener = new_listener(local_addr, true).await?;
    log::info!("Server listening on: {}", &listener.local_addr()?);
    if let Ok((stream, addr)) = timeout(CONNECT_TIMEOUT, listener.accept()).await? {
        stream.set_nodelay(true).ok();
        let stream_addr = stream.local_addr()?;
        create_tcp_connection(
            server,
            Stream::from(stream, stream_addr),
            addr,
            secure,
            meta,
        )
        .await?;
    }
    Ok(())
}

pub async fn create_tcp_connection(
    server: ServerPtr,
    stream: Stream,
    addr: SocketAddr,
    secure: bool,
    meta: ConnectionMeta,
) -> ResultType<()> {
    let mut stream = stream;
    let id = server.write().unwrap().get_new_id();
    let (sk, pk) = Config::get_key_pair();
    if secure && pk.len() == sign::PUBLICKEYBYTES && sk.len() == sign::SECRETKEYBYTES {
        let mut sk_ = [0u8; sign::SECRETKEYBYTES];
        sk_[..].copy_from_slice(&sk);
        let sk = sign::SecretKey(sk_);
        let mut msg_out = Message::new();
        let (our_pk_b, our_sk_b) = box_::gen_keypair();
        msg_out.set_signed_id(SignedId {
            id: sign::sign(
                &IdPk {
                    id: Config::get_id(),
                    pk: Bytes::from(our_pk_b.0.to_vec()),
                    ..Default::default()
                }
                .write_to_bytes()
                .unwrap_or_default(),
                &sk,
            )
            .into(),
            ..Default::default()
        });
        timeout(CONNECT_TIMEOUT, stream.send(&msg_out)).await??;
        match timeout(CONNECT_TIMEOUT, stream.next()).await? {
            Some(res) => {
                let bytes = res?;
                if let Ok(msg_in) = Message::parse_from_bytes(&bytes) {
                    if let Some(message::Union::PublicKey(pk)) = msg_in.union {
                        if pk.asymmetric_value.len() == box_::PUBLICKEYBYTES {
                            stream.set_key(tcp::Encrypt::decode(
                                &pk.symmetric_value,
                                &pk.asymmetric_value,
                                &our_sk_b,
                            )?);
                        } else if pk.asymmetric_value.is_empty() {
                            Config::set_key_confirmed(false);
                            log::info!("Force to update pk");
                        } else {
                            bail!("Handshake failed: invalid public sign key length from peer");
                        }
                    } else {
                        log::error!("Handshake failed: invalid message type");
                    }
                } else {
                    bail!("Handshake failed: invalid message format");
                }
            }
            None => {
                bail!("Failed to receive public key");
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(task) = Command::new("/usr/bin/caffeinate")
            .arg("-u")
            .arg("-t 5")
            .spawn()
        {
            super::CHILD_PROCESS.lock().unwrap().push(task);
        }
        log::info!("wake up macos");
    }
    Connection::start(addr, stream, id, Arc::downgrade(&server), meta).await;
    Ok(())
}

pub async fn accept_connection(
    server: ServerPtr,
    socket: Stream,
    peer_addr: SocketAddr,
    secure: bool,
    meta: ConnectionMeta,
) {
    if let Err(err) = accept_connection_(server, socket, secure, meta).await {
        log::warn!("Failed to accept connection from {}: {}", peer_addr, err);
    }
}

pub async fn create_relay_connection(
    server: ServerPtr,
    relay_server: String,
    uuid: String,
    peer_addr: SocketAddr,
    secure: bool,
    ipv4: bool,
    meta: ConnectionMeta,
) {
    if let Err(err) = create_relay_connection_(
        server,
        relay_server,
        uuid.clone(),
        peer_addr,
        secure,
        ipv4,
        meta,
    )
    .await
    {
        log::error!(
            "Failed to create relay connection for {} with uuid {}: {}",
            peer_addr,
            uuid,
            err
        );
    }
}

async fn create_relay_connection_(
    server: ServerPtr,
    relay_server: String,
    uuid: String,
    peer_addr: SocketAddr,
    secure: bool,
    ipv4: bool,
    meta: ConnectionMeta,
) -> ResultType<()> {
    let mut stream = socket_client::connect_tcp(
        socket_client::ipv4_to_ipv6(crate::check_port(relay_server, RELAY_PORT), ipv4),
        CONNECT_TIMEOUT,
    )
    .await?;
    let mut msg_out = RendezvousMessage::new();
    let licence_key = crate::get_key(true).await;
    msg_out.set_request_relay(RequestRelay {
        licence_key,
        uuid,
        ..Default::default()
    });
    stream.send(&msg_out).await?;
    create_tcp_connection(server, stream, peer_addr, secure, meta).await?;
    Ok(())
}

impl Server {
    fn is_video_service_name(name: &str) -> bool {
        name.starts_with(VideoSource::Monitor.service_name_prefix())
            || name.starts_with(VideoSource::Camera.service_name_prefix())
    }

    pub fn try_add_primary_camera_service(&mut self) {
        if !camera::primary_camera_exists() {
            return;
        }
        let primary_camera_name =
            video_service::get_service_name(VideoSource::Camera, camera::PRIMARY_CAMERA_IDX);
        if !self.contains(&primary_camera_name) {
            self.add_service(Box::new(video_service::new(
                VideoSource::Camera,
                camera::PRIMARY_CAMERA_IDX,
            )));
        }
    }

    pub fn try_add_monitor_service(&mut self, display_idx: usize) {
        let monitor_service_name =
            video_service::get_service_name(VideoSource::Monitor, display_idx);
        if !self.contains(&monitor_service_name) {
            self.add_service(Box::new(video_service::new(
                VideoSource::Monitor,
                display_idx,
            )));
        }
    }

    pub fn add_camera_connection(&mut self, conn: ConnInner) {
        if camera::primary_camera_exists() {
            let primary_camera_name =
                video_service::get_service_name(VideoSource::Camera, camera::PRIMARY_CAMERA_IDX);
            if let Some(s) = self.services.get(&primary_camera_name) {
                s.on_subscribe(conn.clone());
            }
        }
        self.connections.insert(conn.id(), conn);
    }

    pub fn add_monitor_connection(
        &mut self,
        conn: ConnInner,
        noperms: &Vec<&'static str>,
        display_idx: usize,
    ) {
        let monitor_service_name =
            video_service::get_service_name(VideoSource::Monitor, display_idx);
        for s in self.services.values() {
            let name = s.name();
            if Self::is_video_service_name(&name) && name != monitor_service_name {
                continue;
            }
            if !noperms.contains(&(&name as _)) {
                s.on_subscribe(conn.clone());
            }
        }
        #[cfg(target_os = "macos")]
        self.update_enable_retina();
        self.connections.insert(conn.id(), conn);
    }

    pub fn remove_connection(&mut self, conn: &ConnInner) {
        for s in self.services.values() {
            s.on_unsubscribe(conn.id());
        }
        self.connections.remove(&conn.id());
        #[cfg(target_os = "macos")]
        self.update_enable_retina();
    }

    pub fn close_connections(&mut self) {
        let conn_inners: Vec<_> = self.connections.values_mut().collect();
        for c in conn_inners {
            let mut misc = Misc::new();
            misc.set_stop_service(true);
            let mut msg = Message::new();
            msg.set_misc(misc);
            c.send(Arc::new(msg));
        }
    }

    fn add_service(&mut self, service: Box<dyn Service>) {
        let name = service.name();
        self.services.insert(name, service);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.services.contains_key(name)
    }

    pub fn subscribe(&mut self, name: &str, conn: ConnInner, sub: bool) {
        if let Some(s) = self.services.get(name) {
            if s.is_subed(conn.id()) == sub {
                return;
            }
            if sub {
                s.on_subscribe(conn.clone());
            } else {
                s.on_unsubscribe(conn.id());
            }
            #[cfg(target_os = "macos")]
            self.update_enable_retina();
        }
    }

    // get a new unique id
    pub fn get_new_id(&mut self) -> i32 {
        self.id_count += 1;
        self.id_count
    }

    pub fn set_video_service_opt(
        &self,
        display: Option<(VideoSource, usize)>,
        opt: &str,
        value: &str,
    ) {
        for (k, v) in self.services.iter() {
            if let Some((source, display)) = display {
                if k != &video_service::get_service_name(source, display) {
                    continue;
                }
            }

            if Self::is_video_service_name(k) {
                v.set_option(opt, value);
            }
        }
    }

    fn get_subbed_displays_count(&self, conn_id: i32) -> usize {
        self.services
            .keys()
            .filter(|k| {
                Self::is_video_service_name(k)
                    && self
                        .services
                        .get(*k)
                        .map(|s| s.is_subed(conn_id))
                        .unwrap_or(false)
            })
            .count()
    }

    fn capture_displays(
        &mut self,
        conn: ConnInner,
        source: VideoSource,
        displays: &[usize],
        include: bool,
        exclude: bool,
    ) {
        let displays = displays
            .iter()
            .map(|d| video_service::get_service_name(source, *d))
            .collect::<Vec<_>>();
        let keys = self.services.keys().cloned().collect::<Vec<_>>();
        for name in keys.iter() {
            if Self::is_video_service_name(&name) {
                if displays.contains(&name) {
                    if include {
                        self.subscribe(&name, conn.clone(), true);
                    }
                } else {
                    if exclude {
                        self.subscribe(&name, conn.clone(), false);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn update_enable_retina(&self) {
        let mut video_service_count = 0;
        for (name, service) in self.services.iter() {
            if Self::is_video_service_name(&name) && service.ok() {
                video_service_count += 1;
            }
        }
        *scrap::quartz::ENABLE_RETINA.lock().unwrap() = video_service_count < 2;
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        for s in self.services.values() {
            s.join();
        }
        #[cfg(target_os = "linux")]
        wayland::clear();
    }
}

pub fn check_zombie() {
    std::thread::spawn(|| loop {
        let mut lock = CHILD_PROCESS.lock().unwrap();
        let mut i = 0;
        while i != lock.len() {
            let c = &mut (*lock)[i];
            if let Ok(Some(_)) = c.try_wait() {
                lock.remove(i);
            } else {
                i += 1;
            }
        }
        drop(lock);
        std::thread::sleep(Duration::from_millis(100));
    });
}

/// Start the host server that allows the remote peer to control the current machine.
///
/// # Arguments
///
/// * `is_server` - Whether the current client is definitely the server.
/// If true, the server will be started.
/// Otherwise, client will check if there's already a server and start one if not.
#[tokio::main]
pub async fn start_server() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[cfg(target_os = "linux")]
        {
            log::info!("DISPLAY={:?}", std::env::var("DISPLAY"));
            log::info!("XAUTHORITY={:?}", std::env::var("XAUTHORITY"));
        }
        #[cfg(windows)]
        hbb_common::platform::windows::start_cpu_performance_monitor();
    });

    crate::common::set_server_running(true);
    input_service::fix_key_down_timeout_loop();
    #[cfg(target_os = "linux")]
    if input_service::wayland_use_uinput() {
        allow_err!(input_service::setup_uinput(0, 1920, 0, 1080).await);
    }
    #[cfg(target_os = "windows")]
    crate::platform::try_kill_broker();
    #[cfg(feature = "hwcodec")]
    scrap::hwcodec::start_check_process();
    crate::rendezvous_mediator::start_all().await;
}


