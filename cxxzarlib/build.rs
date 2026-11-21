fn main() {
    cxx_build::bridge("src/lib.rs")
        .file("src/stream.cpp")
        .include("include")
        .include("../czarlib/include")
        .flag_if_supported("-std=c++14")
        .warnings(true)
        .compile("cxxzarlib");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/stream.cpp");
    println!("cargo:rerun-if-changed=include/cxxzarlib/stream.hpp");
    println!("cargo:rerun-if-changed=../czarlib/include/czarlib.h");
}
