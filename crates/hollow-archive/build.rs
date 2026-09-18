fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "Hollow Archive");
        res.set("FileDescription", "Zenless Zone Zero inventory exporter");
        res.compile().expect("windows resource");
    }
}
