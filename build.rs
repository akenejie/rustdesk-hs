// Generates src/version.rs from Cargo.toml. Inlined from hbb_common so the build script
// does not depend on hbb_common/sodiumoxide (which would compile libsodium for the host
// and pick up the target's SODIUM_LIB_DIR, breaking cross-compiles to windows arm64).
fn gen_version() {
    use std::io::prelude::*;
    println!("cargo:rerun-if-changed=Cargo.toml");
    let mut file = std::fs::File::create("./src/version.rs").unwrap();
    let lines: Vec<String> = read_lines("Cargo.toml").unwrap().flatten().collect();
    let mut version = "1.4.9".to_owned();
    let mut found = false;
    let mut in_meta = false;
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_meta = trimmed.starts_with("[package.metadata.rustdesk]");
        } else if in_meta {
            let ab: Vec<&str> = line.split('=').map(|x| x.trim()).collect();
            if ab.len() == 2 && ab[0] == "protocol-version" {
                version = ab[1].trim_matches('"').to_owned();
                found = true;
            }
        }
    }
    if !found {
        for line in &lines {
            let ab: Vec<&str> = line.split('=').map(|x| x.trim()).collect();
            if ab.len() == 2 && ab[0] == "version" {
                version = ab[1].trim_matches('"').to_owned();
            }
        }
    }
    file.write_all(format!("pub const VERSION: &str = \"{}\";\n", version).as_bytes())
        .ok();
    // generate build date
    let build_date = format!("{}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    file.write_all(
        format!("#[allow(dead_code)]\npub const BUILD_DATE: &str = \"{build_date}\";\n").as_bytes(),
    )
    .ok();
    file.sync_all().ok();
}

fn read_lines<P>(filename: P) -> std::io::Result<std::io::Lines<std::io::BufReader<std::fs::File>>>
where
    P: AsRef<std::path::Path>,
{
    use std::io::BufRead;
    let file = std::fs::File::open(filename)?;
    Ok(std::io::BufReader::new(file).lines())
}
#[cfg(windows)]
fn build_windows() {
    let file = "src/platform/windows.cc";
    let file2 = "src/platform/windows_delete_test_cert.cc";
    cc::Build::new().file(file).file(file2).compile("windows");
    println!("cargo:rustc-link-lib=WtsApi32");
    println!("cargo:rerun-if-changed={}", file);
    println!("cargo:rerun-if-changed={}", file2);
}

#[cfg(target_os = "macos")]
fn build_mac() {
    let file = "src/platform/macos.mm";
    let mut b = cc::Build::new();
    if let Ok(os_version::OsVersion::MacOS(v)) = os_version::detect() {
        let v = v.version;
        if v.contains("10.14") {
            b.flag("-DNO_InputMonitoringAuthStatus=1");
        }
    }
    b.flag("-std=c++17").file(file).compile("macos");
    println!("cargo:rerun-if-changed={}", file);
}

#[cfg(all(windows, feature = "inline"))]
fn build_manifest() {
    use std::io::Write;
    if std::env::var("PROFILE").unwrap() == "release" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("res/icon.ico")
            .set_language(winapi::um::winnt::MAKELANGID(
                winapi::um::winnt::LANG_ENGLISH,
                winapi::um::winnt::SUBLANG_ENGLISH_US,
            ))
            .set_manifest_file("res/manifest.xml");
        match res.compile() {
            Err(e) => {
                write!(std::io::stderr(), "{}", e).unwrap();
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
}

fn install_android_deps() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os != "android" {
        return;
    }
    let mut target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "x86_64" {
        target_arch = "x64".to_owned();
    } else if target_arch == "x86" {
        target_arch = "x86".to_owned();
    } else if target_arch == "aarch64" {
        target_arch = "arm64".to_owned();
    } else {
        target_arch = "arm".to_owned();
    }
    let target = format!("{}-android", target_arch);
    let vcpkg_root = std::env::var("VCPKG_ROOT").unwrap();
    let mut path: std::path::PathBuf = vcpkg_root.into();
    if let Ok(vcpkg_root) = std::env::var("VCPKG_INSTALLED_ROOT") {
        path = vcpkg_root.into();
    } else {
        path.push("installed");
    }
    path.push(target);
    println!(
        "cargo:rustc-link-search={}",
        path.join("lib").to_str().unwrap()
    );
    println!("cargo:rustc-link-lib=ndk_compat");
    println!("cargo:rustc-link-lib=oboe");
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=OpenSLES");
}

fn main() {
    gen_version();
    install_android_deps();
    #[cfg(all(windows, feature = "inline"))]
    build_manifest();
    #[cfg(windows)]
    build_windows();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "macos" {
        #[cfg(target_os = "macos")]
        build_mac();
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
