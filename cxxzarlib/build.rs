fn main() {
    cxx_build::bridge("src/lib.rs")
        .warnings(true)
        .compile("cxxzarlib");

    println!("cargo:rerun-if-changed=src/lib.rs");
}
