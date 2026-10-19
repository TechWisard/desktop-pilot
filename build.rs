fn main() {
    println!("cargo:rerun-if-changed=assets/desktop-pilot.ico");
    println!("cargo:rerun-if-changed=assets/desktop-pilot.manifest");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/desktop-pilot.ico")
            .set_manifest_file("assets/desktop-pilot.manifest")
            .set("ProductName", "Desktop Pilot")
            .set("FileDescription", "Desktop Pilot")
            .set("OriginalFilename", "desktop-pilot.exe")
            .compile()
            .expect("Windows application resources could not be compiled");
    }
}
