fn main() {
    println!("cargo::rustc-check-cfg=cfg(hero_image)");
    println!("cargo:rerun-if-changed=assets/hero.png");
    if std::path::Path::new("assets/hero.png").exists() {
        println!("cargo:rustc-cfg=hero_image");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "Hollow Archive");
        res.set("FileDescription", "Zenless Zone Zero inventory exporter");
        res.compile().expect("windows resource");
    }
}
