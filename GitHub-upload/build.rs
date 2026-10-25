//! Compiles the vendored Rubber Band Library (single-file build) used for pitch shifting.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rerun-if-env-changed=MUTAFORM_SKIP_CPP");
    if std::env::var_os("MUTAFORM_SKIP_CPP").is_some() {
        // Only used for `cargo check` cross-checks where no C++ toolchain is available.
        return;
    }
    let mut b = cc::Build::new();
    b.cpp(true)
        .file("third_party/rubberband/single/RubberBandSingle.cpp")
        .include("third_party/rubberband")
        .opt_level(3)
        .warnings(false)
        .cpp_link_stdlib(None);
    if !target.contains("msvc") {
        b.flag("-ffast-math").flag("-fno-finite-math-only").flag("-std=c++14");
    }
    let compiler = b.get_compiler();
    b.compile("rubberband");
    if target.contains("windows-gnu") {
        // Link the C++ runtime statically so the DLL has no libstdc++/libgcc dependencies.
        if let Ok(out) = std::process::Command::new(compiler.path()).arg("-print-file-name=libstdc++.a").output() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(dir) = std::path::Path::new(&p).parent() {
                println!("cargo:rustc-link-search=native={}", dir.display());
            }
        }
        println!("cargo:rustc-link-lib=static=stdc++");
    } else if target.contains("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    } else if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
        // The single-file Rubber Band build uses Apple's vDSP FFT on macOS.
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }
    println!("cargo:rerun-if-changed=third_party/rubberband");
    println!("cargo:rerun-if-changed=build.rs");
}
