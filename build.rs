use std::env;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap();
    if target != "armv7-sony-vita-newlibeabihf" {
        return;
    }

    let vitasdk_path = PathBuf::from(env::var("VITASDK").unwrap());
    let vitasdk_include_path = vitasdk_path.join("arm-vita-eabi").join("include");
    let vitasdk_lib_path = vitasdk_path.join("arm-vita-eabi").join("lib");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bindings_file = out_path.join("imgui_bindings.rs");

    const IMGUI_HEADERS: [&str; 3] = ["imgui.h", "imgui_internal.h", "imgui_impl_vitagl.h"];
    let mut bindings = bindgen::Builder::default().clang_args(["-I", vitasdk_include_path.to_str().unwrap()]).clang_args(["-x", "c++"]);
    for header in IMGUI_HEADERS {
        bindings = bindings.header(vitasdk_include_path.join(header).to_str().unwrap());
    }
    bindings.rust_target(bindgen::RustTarget::Nightly).generate().unwrap().write_to_file(bindings_file).unwrap();
    
    println!(r"cargo:rustc-link-search=native={vitasdk_lib_path:?}");
    println!("cargo:rustc-link-lib=static=imgui")
}
