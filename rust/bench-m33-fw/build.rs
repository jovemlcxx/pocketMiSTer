// O linker acha o m33.ld (mapa de memória da RP2350) na pasta do crate.
fn main() {
    println!(
        "cargo:rustc-link-search={}",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    );
    println!("cargo:rerun-if-changed=m33.ld");
}
