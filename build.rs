//! Windows: embed the app icon and version information in netlix.exe, so Explorer,
//! the taskbar and the installer show them.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/netlix.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/netlix.ico")
            .set("ProductName", "netlix")
            .set("FileDescription", "netlix — network simulator")
            .set("CompanyName", "Helix")
            .set("LegalCopyright", "Helix contributors");
        res.compile().expect("embed Windows resources");
    }
}
