#[cfg(not(debug_assertions))]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::platform::breakdown_callback;
#[cfg(not(debug_assertions))]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use hbb_common::platform::register_breakdown_handler;
use hbb_common::config;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn core_main() -> Option<Vec<String>> {
    if !crate::common::global_init() {
        return None;
    }
    crate::load_custom_client();
    #[cfg(windows)]
    if !crate::platform::windows::bootstrap() {
        // return None to terminate the process
        return None;
    }
    let mut args = Vec::new();
    let mut direct_port: u16 = 0;
    let mut password = String::new();
    let env_args: Vec<String> = std::env::args().collect();
    let mut j = 0;
    while j < env_args.len() {
        let arg = &env_args[j];
        if j > 0 {
            if arg == "--port" {
                if j + 1 < env_args.len() {
                    direct_port = env_args[j + 1].parse().unwrap_or(0);
                    j += 1;
                }
            } else if arg == "--password" {
                if j + 1 < env_args.len() {
                    password = env_args[j + 1].clone();
                    j += 1;
                }
            } else {
                args.push(arg.clone());
            }
        }
        j += 1;
    }
    #[cfg(not(debug_assertions))]
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    register_breakdown_handler(breakdown_callback);
    if let Some(cmd) = args.first().map(String::as_str) {
        match cmd {
            "--version" => {
                println!("{}", crate::VERSION);
                return None;
            }
            "--build-date" => {
                println!("{}", crate::BUILD_DATE);
                return None;
            }
            "--help" | "-h" => {
                print_usage();
                return None;
            }
            _ => {
                eprintln!("Unknown option: {cmd}");
                eprintln!("Run `rustdesk --help` for usage.");
                std::process::exit(2);
            }
        }
    }
    if direct_port > 0 {
        config::Config::set_option(
            config::keys::OPTION_DIRECT_ACCESS_PORT.to_string(),
            direct_port.to_string(),
        );
    }
    if !password.is_empty() {
        config::Config::set_permanent_password(&password);
    }
    #[cfg(windows)]
    hbb_common::config::PeerConfig::preload_peers();
    hbb_common::init_log(false, "");
    crate::start_server();
    None
}

fn print_usage() {
    println!(
        "RustDesk headless host server {}\n\
         \n\
         Usage: rustdesk [options]\n\
         \n\
         Hosts the desktop over the RustDesk protocol on a single port.\n\
         Connect with the official RustDesk client by entering the IP\n\
         (e.g. `192.168.1.10`) in the ID field; append `:port` if you use\n\
         a non-default port.\n\
         \n\
         Options:\n\
             --port <n>       Listening port (default: 21118)\n\
             --password <pwd> Set the permanent access password. When\n\
                              omitted, the server accepts connections\n\
                              without a password.\n\
             --version        Print version and exit\n\
             --build-date     Print build date and exit\n\
             -h, --help       Show this help and exit",
        crate::VERSION
    );
}
