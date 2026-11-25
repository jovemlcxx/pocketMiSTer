// O linker acha o memory.x (mapa de memória da RP2350) na pasta do crate.
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rerun-if-changed=memory.x");
}
