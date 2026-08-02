use hbb_common::{allow_err, config::Config, log, sleep, tokio};

use crate::server::{check_zombie, new as new_server, ConnectionMeta, ServerPtr};

fn get_direct_port() -> i32 {
    let mut port = Config::get_option("direct-access-port")
        .parse::<i32>()
        .unwrap_or(0);
    if port <= 0 {
        port = 21118;
    }
    port
}

async fn direct_server(server: ServerPtr) {
    let mut listener = None;
    let mut port = 0;
    loop {
        if listener.is_none() {
            port = get_direct_port();
            match hbb_common::tcp::listen_any(port as _).await {
                Ok(l) => {
                    listener = Some(l);
                    log::info!(
                        "Direct server listening on: {:?}",
                        listener.as_ref().map(|l| l.local_addr())
                    );
                }
                Err(err) => {
                    log::error!(
                        "Failed to start direct server on port: {}, error: {}",
                        port,
                        err
                    );
                    loop {
                        if port != get_direct_port() {
                            break;
                        }
                        sleep(1.).await;
                    }
                }
            }
        }
        if let Some(l) = listener.as_mut() {
            if port != get_direct_port() {
                log::info!("Exit direct access listen");
                listener = None;
                continue;
            }
            if let Ok(Ok((stream, addr))) = hbb_common::timeout(1000, l.accept()).await {
                stream.set_nodelay(true).ok();
                log::info!("direct access from {}", addr);
                let local_addr = stream
                    .local_addr()
                    .unwrap_or(Config::get_any_listen_addr(true));
                let server = server.clone();
                tokio::spawn(async move {
                    allow_err!(
                        crate::server::create_tcp_connection(
                            server,
                            hbb_common::Stream::from(stream, local_addr),
                            addr,
                            false,
                            ConnectionMeta::default(),
                        )
                        .await
                    );
                });
            } else {
                sleep(0.1).await;
            }
        } else {
            sleep(1.).await;
        }
    }
}

pub async fn start_all() {
    check_zombie();
    let server = new_server();
    let server_cloned = server.clone();
    tokio::spawn(async move {
        direct_server(server_cloned).await;
    });
    #[cfg(target_os = "linux")]
    if crate::is_server() {
        crate::platform::linux_desktop_manager::start_xdesktop();
    }
    scrap::codec::test_av1();
    let port = get_direct_port();
    log::info!(
        "Direct-connect server started on port {}",
        if port > 0 {
            port.to_string()
        } else {
            "default (21118)".to_string()
        }
    );
    loop {
        sleep(1.).await;
    }
}
