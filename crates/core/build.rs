fn main() {
    // O unrar (C++) usa o registro, CryptGenRandom e OpenProcessToken; o unrar-sys não pede a lib.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
