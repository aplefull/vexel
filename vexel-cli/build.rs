fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        println!("cargo:rustc-link-lib=static:+whole-archive,-bundle=stdc++");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../icon/vexel.ico");
        res.compile().unwrap();
    }
}
