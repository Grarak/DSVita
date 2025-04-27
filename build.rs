use bindgen::Formatter;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

fn main() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-env-changed=OUT_DIR");

    let build_profile_name = out_path.to_str().unwrap().split(std::path::MAIN_SEPARATOR).nth_back(3).unwrap();
    let build_profile_name_file = out_path.join("build_profile_name");
    File::create(build_profile_name_file).unwrap().write_all(build_profile_name.as_bytes()).unwrap();

    let target = env::var("TARGET").unwrap();
    if target != "armv7-sony-vita-newlibeabihf" {
        println!("cargo:rerun-if-env-changed=SYSROOT");
        if let Ok(sysroot) = env::var("SYSROOT") {
            println!("cargo:rustc-link-arg=--sysroot={sysroot}");
        }
        let mut cache_build = cc::Build::new();
        cache_build.compiler("clang");
        // Running IDE on anything other than linux will fail, so ignore compile error
        let _ = cache_build.file("builtins/cache.c").try_compile("cache").ok();
    }

    let num_jobs = env::var("NUM_JOBS").unwrap();

    let vitasdk_path = env::var("VITASDK").map(PathBuf::from);
    println!("cargo:rerun-if-env-changed=VITASDK");
    if vitasdk_path.is_err() {
        return;
    }
    let vitasdk_path = vitasdk_path.unwrap();
    let vitasdk_sysroot = vitasdk_path.join("arm-vita-eabi");
    let vitasdk_include_path = vitasdk_sysroot.join("include");
    let vitasdk_lib_path = vitasdk_sysroot.join("lib");

    let kubridge_path = PathBuf::from("kubridge");

    {
        let bindings_file = out_path.join("imgui_bindings.rs");

        const IMGUI_HEADERS: [&str; 3] = ["imgui.h", "imgui_internal.h", "imgui_impl_vitagl.h"];
        let mut bindings = bindgen::Builder::default()
            .clang_args(["-x", "c++"])
            .clang_args(["-std=c++17"])
            .clang_args(["-target", "armv7-unknown-linux-gnueabihf"])
            .clang_args(["--sysroot", vitasdk_sysroot.to_str().unwrap()])
            .formatter(Formatter::Prettyplease);
        for header in IMGUI_HEADERS {
            let header_path = vitasdk_include_path.join(header);
            println!("cargo:rerun-if-changed={header_path:?}");
            bindings = bindings.header(header_path.to_str().unwrap());
        }
        bindings.rust_target(bindgen::RustTarget::Nightly).generate().unwrap().write_to_file(bindings_file).unwrap();

        println!("cargo:rustc-link-search=native={vitasdk_lib_path:?}");
        if target == "armv7-sony-vita-newlibeabihf" {
            println!("cargo:rustc-link-lib=static=imgui");
        }
    }

    {
        let bindings_file = out_path.join("kubridge_bindings.rs");

        const KUBRIDGE_HEADERS: [&str; 1] = ["kubridge.h"];
        let mut bindings = bindgen::Builder::default()
            .clang_args(["-I", kubridge_path.to_str().unwrap()])
            .clang_args(["-x", "c++"])
            .clang_args(["-std=c++17"])
            .clang_args(["-target", "armv7-unknown-linux-gnueabihf"])
            .clang_args(["--sysroot", vitasdk_sysroot.to_str().unwrap()])
            .formatter(Formatter::Prettyplease);
        for header in KUBRIDGE_HEADERS {
            let header_path = kubridge_path.join(header);
            println!("cargo:rerun-if-changed={header_path:?}");
            bindings = bindings.header(header_path.to_str().unwrap());
        }
        bindings.rust_target(bindgen::RustTarget::Nightly).generate().unwrap().write_to_file(bindings_file).unwrap();
    }

    if target != "armv7-sony-vita-newlibeabihf" {
        return;
    }

    {
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

    {
        let kubridge_dst_path = cmake::Config::new(&kubridge_path).build_target("libkubridge_stub.a").build().join("build");
        let kubridge_lib_path = kubridge_dst_path.join("libkubridge_stub.a");
        let kubridge_lib_new_path = kubridge_dst_path.join("libkubridge_stub_dsvita.a");
        fs::rename(kubridge_lib_path, kubridge_lib_new_path).unwrap();

        println!("cargo:rustc-link-search=native={}", fs::canonicalize(kubridge_dst_path).unwrap().to_str().unwrap());
        println!("cargo:rustc-link-lib=static=kubridge_stub_dsvita");
    }
}
