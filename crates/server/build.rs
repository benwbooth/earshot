// LAME (libmp3lame), for the relay's MP3 stream, comes from the system: found
// with pkg-config (Nix provides it). Without it the server still builds, it
// just doesn't offer the relay.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(earshot_lame)");
    println!("cargo::rerun-if-env-changed=PKG_CONFIG_PATH");
    if pkg_config::Config::new().atleast_version("3.99").probe("lame").is_ok() {
        println!("cargo::rustc-cfg=earshot_lame");
    }
}
