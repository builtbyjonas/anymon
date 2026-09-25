fn main() {
    // Exposes the target triple so `anymon update` can pick the matching
    // release archive (e.g. the musl build on Alpine).
    let target = std::env::var("TARGET").expect("cargo sets TARGET");
    println!("cargo:rustc-env=ANYMON_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
}
