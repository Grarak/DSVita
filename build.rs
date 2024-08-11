use bindgen::Formatter;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

fn main() {
    let target = env::var("TARGET").unwrap();
    if target != "armv7-sony-vita-newlibeabihf" {
        // Running IDE on anything other than linux will fail, so ignore compile error
        let _ = cc::Build::new().file("builtins/cache.c").try_compile("cache").ok();
        return;
    }

    let num_jobs = env::var("NUM_JOBS").unwrap();

    let vitasdk_path = PathBuf::from(env::var("VITASDK").unwrap());
    let vitasdk_include_path = vitasdk_path.join("arm-vita-eabi").join("include");
    let vitasdk_lib_path = vitasdk_path.join("arm-vita-eabi").join("lib");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bindings_file = out_path.join("imgui_bindings.rs");

    const IMGUI_HEADERS: [&str; 3] = ["imgui.h", "imgui_internal.h", "imgui_impl_vitagl.h"];
    let mut bindings = bindgen::Builder::default()
        .clang_args(["-I", vitasdk_include_path.to_str().unwrap()])
        .clang_args(["-x", "c++"])
        .clang_args(["-std=c++17"])
        .clang_args(["-target", "armv7a-none-eabihf"])
        .formatter(Formatter::Prettyplease);
    for header in IMGUI_HEADERS {
        let header_path = vitasdk_include_path.join(header);
        println!("cargo:rerun-if-changed={header_path:?}");
        bindings = bindings.header(header_path.to_str().unwrap());
    }
    bindings.rust_target(bindgen::RustTarget::Nightly).generate().unwrap().write_to_file(bindings_file).unwrap();

    println!("cargo:rustc-link-search=native={vitasdk_lib_path:?}");
    println!("cargo:rustc-link-lib=static=imgui");

    let vita_gl_path = PathBuf::from("vitaGL");
    let vita_gl_lib_path = vita_gl_path.join("libvitaGL.a");
    let vita_gl_lib_new_path = vita_gl_path.join("libvitaGL_dsvita.a");

    Command::new("make")
        .current_dir("vitaGL")
        .args(["-j", &num_jobs])
        .envs([
            ("HAVE_UNFLIPPED_FBOS", "1"),
            ("NO_TEX_COMBINER", "1"),
            ("NO_DEBUG", "1"),
            ("SHADER_COMPILER_SPEEDHACK", "1"),
            ("MATH_SPEEDHACK", "1"),
            ("HAVE_SHADER_CACHE", "1"),
            // ("HAVE_SHARK_LOG", "1"),
            // ("LOG_ERRORS", "1"),
            // ("HAVE_RAZOR", "1"),
        ])
        .status()
        .unwrap();

    fs::rename(vita_gl_lib_path, vita_gl_lib_new_path).unwrap();
    println!("cargo:rustc-link-search=native={}", fs::canonicalize(vita_gl_path).unwrap().to_str().unwrap());
    println!("cargo:rustc-link-lib=static=vitaGL_dsvita");
}
