fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_icon_png)");
    let icon_path = std::path::Path::new("assets/icon.png");
    if icon_path.exists() {
        println!("cargo:rustc-cfg=has_icon_png");
        println!("cargo:rerun-if-changed=assets/icon.png");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
