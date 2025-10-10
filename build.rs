fn main() {
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/app-icon.ico")
            .set("ProductName", "PhotoEnhancer")
            .set("FileDescription", "PhotoEnhancer — local photo enhancement")
            .set(
                "OriginalFilename",
                &format!("PhotoEnhancer-v{}.exe", env!("CARGO_PKG_VERSION")),
            )
            .set(
                "LegalCopyright",
                "Copyright © 2026 PhotoEnhancer contributors",
            );
        resource
            .compile()
            .expect("compile Windows application icon and version resources");
    }
}
