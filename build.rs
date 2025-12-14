use bindgen::Formatter;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};
use vitabuild::{create_bindgen_builder, create_c_build, create_cc_build, get_out_path, get_profile_name, is_host_linux, is_profiling, is_target_vita};

fn generate_linux_imgui_bindings() {
    let bindings_file = get_out_path().join("imgui_bindings.rs");
    let bindings = create_bindgen_builder()
        .header("imgui_wrapper.h")
        .clang_args(["-x", "c++"])
        .clang_arg("-std=c++17")
        .clang_arg("-Iimgui")
        .formatter(Formatter::Prettyplease)
        .allowlist_item("ImGui.*")
        .opaque_type("std::.*")
        .use_core()
        .enable_cxx_namespaces()
        .trust_clang_mangling(true);
    bindings.rust_target(bindgen::RustTarget::nightly()).generate().unwrap().write_to_file(bindings_file).unwrap();
}

fn main() {
    println!("cargo::rustc-check-cfg=cfg(profiling)");
    println!("cargo:rerun-if-env-changed=OUT_DIR");
    let out_path = get_out_path();

    let build_profile_name_file = out_path.join("build_profile_name");
    File::create(build_profile_name_file).unwrap().write_all(get_profile_name().as_bytes()).unwrap();

    if !is_target_vita() {
        println!("cargo:rerun-if-env-changed=SYSROOT");
        if let Ok(sysroot) = env::var("DSVITA_SYSROOT") {
            println!("cargo:rustc-link-arg=--sysroot={sysroot}");
        }
        let mut cache_build = create_c_build();
        cache_build.compiler("clang");
        // Running IDE on anything other than linux will fail, so ignore compile error
        let _ = cache_build.file("builtins/cache.c").try_compile("cache").ok();
    }

    let vitasdk_path = env::var("VITASDK").map(PathBuf::from);
    println!("cargo:rerun-if-env-changed=VITASDK");

    {
        const MATH_NEON_FILES: [&str; 31] = [
            "math_acosf.c",
            "math_asinf.c",
            "math_atan2f.c",
            "math_atanf.c",
            "math_ceilf.c",
            "math_cosf.c",
            "math_coshf.c",
            "math_expf.c",
            // "math_fabsf.c",
            "math_floorf.c",
            "math_fmodf.c",
            "math_invsqrtf.c",
            "math_ldexpf.c",
            "math_log10f.c",
            "math_logf.c",
            "math_mat2.c",
            "math_mat3.c",
            "math_mat4.c",
            "math_modf.c",
            "math_powf.c",
            "math_runfast.c",
            "math_sincosf.c",
            "math_sinf.c",
            "math_sinfv.c",
            "math_sinhf.c",
            "math_sqrtf.c",
            "math_sqrtfv.c",
            "math_tanf.c",
            "math_tanhf.c",
            "math_vec2.c",
            "math_vec3.c",
            "math_vec4.c",
        ];

        let math_neon_path = Path::new("math-neon/source");

        if !is_target_vita() {
            let mut math_neon_build = create_c_build();
            for file in MATH_NEON_FILES {
                let path = math_neon_path.join(file);
                println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
                math_neon_build.file(path);
            }
            math_neon_build.compile("mathneon");
        }

        let math_neon_bindgen = create_bindgen_builder();

        let bindings_file = out_path.join("math_neon.rs");
        math_neon_bindgen
            .header(math_neon_path.join("math_neon.h").to_str().unwrap())
            .formatter(Formatter::Prettyplease)
            .generate_comments(true)
            .layout_tests(false)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .use_core()
            .trust_clang_mangling(true)
            .generate()
            .unwrap()
            .write_to_file(bindings_file)
            .unwrap();
    }

    {
        const SOUNDTOUCH_FILES: [&str; 14] = [
            "AAFilter.cpp",
            "BPMDetect.cpp",
            "cpu_detect_x86.cpp",
            "FIFOSampleBuffer.cpp",
            "FIRFilter.cpp",
            "InterpolateCubic.cpp",
            "InterpolateLinear.cpp",
            "InterpolateShannon.cpp",
            "mmx_optimized.cpp",
            "PeakFinder.cpp",
            "RateTransposer.cpp",
            "SoundTouch.cpp",
            "sse_optimized.cpp",
            "TDStretch.cpp",
        ];

        let soundtouch_path = Path::new("soundtouch");
        let soundtouch_includes = vec![
            soundtouch_path.join("include").to_str().unwrap().to_string(),
            soundtouch_path.join("source").join("SoundTouch").to_str().unwrap().to_string(),
        ];

        let soundtouch_flags = vec![
            "-DSOUNDTOUCH_INTEGER_SAMPLES=1".to_string(),
            "-std=c++17".to_string(),
            "-DST_NO_EXCEPTION_HANDLING=1".to_string(),
            "-DM_PI=3.14159265358979323846".to_string(),
        ];

        let mut soundtouch_build = create_cc_build();
        soundtouch_build.cpp(true);
        for file in SOUNDTOUCH_FILES {
            let path = soundtouch_path.join("source").join("SoundTouch").join(file);
            println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
            soundtouch_build.file(path);
        }
        for include in &soundtouch_includes {
            println!("cargo:rerun-if-changed={include}");
            soundtouch_build.include(include);
        }
        for flag in &soundtouch_flags {
            soundtouch_build.flag(flag);
        }

        let soundtouch_wrapper_path = out_path.join("soundtouch_wrapper.cpp");
        let mut soundtouch_wrapper = File::create(&soundtouch_wrapper_path).unwrap();
        writeln!(soundtouch_wrapper, "#include <SoundTouch.h>").unwrap();
        writeln!(soundtouch_wrapper, "namespace soundtouch {{").unwrap();
        writeln!(
            soundtouch_wrapper,
            "uint FIFOProcessor_numSamples(const FIFOProcessor* processor) {{ return processor->numSamples(); }}"
        )
        .unwrap();
        writeln!(soundtouch_wrapper, "}}").unwrap();
        soundtouch_build.file(soundtouch_wrapper_path);
        soundtouch_build.compile("soundtouch");

        let bindings_file = out_path.join("soundtouch_bindings.rs");

        let mut soundtouch_bindgen = create_bindgen_builder().clang_args(["-x", "c++"]);
        for include in &soundtouch_includes {
            soundtouch_bindgen = soundtouch_bindgen.clang_arg(format!("-I{include}"));
        }
        for flag in &soundtouch_flags {
            soundtouch_bindgen = soundtouch_bindgen.clang_arg(flag);
        }
        soundtouch_bindgen
            .header("soundtouch_wrapper.h")
            .formatter(Formatter::Prettyplease)
            .generate_comments(true)
            .layout_tests(false)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .constified_enum_module("*")
            .allowlist_type("soundtouch::SoundTouch")
            .allowlist_type("soundtouch::FIFOProcessor")
            .allowlist_type("soundtouch::FIFOSamplePipe")
            .allowlist_type("soundtouch::SAMPLETYPE")
            .allowlist_type("soundtouch::BPMDetect")
            .allowlist_type("soundtouch::TDStretch")
            .allowlist_type("soundtouch::RateTransposer")
            .allowlist_function("soundtouch::FIFOProcessor_numSamples")
            .opaque_type("std::.*")
            .manually_drop_union(".*")
            .default_non_copy_union_style(bindgen::NonCopyUnionStyle::ManuallyDrop)
            .use_core()
            .enable_cxx_namespaces()
            .trust_clang_mangling(true)
            .generate()
            .unwrap()
            .write_to_file(bindings_file)
            .unwrap();
    }

    if !is_target_vita() && is_host_linux() {
        const IMGUI_FILES: &[&str] = &["imgui.cpp", "imgui_draw.cpp"];

        let imgui_path = PathBuf::from("imgui");

        let mut imgui_build = create_cc_build();
        imgui_build
            .cpp(true)
            .include(&imgui_path)
            .include(imgui_path.join("examples").join("sdl_opengl3_example"))
            .file("imgui_impl_sdl_gl3.cpp");

        println!("cargo:rerun-if-changed=imgui_impl_sdl_gl3.cpp");
        for file in IMGUI_FILES {
            let path = imgui_path.join(file);
            println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
            imgui_build.file(path);
        }

        imgui_build.compile("imgui");

        generate_linux_imgui_bindings();

        println!("cargo:rustc-link-lib=GL");
    }

    if vitasdk_path.is_err() {
        return;
    }
    let vitasdk_path = vitasdk_path.unwrap();
    let vitasdk_sysroot = vitasdk_path.join("arm-vita-eabi");
    let vitasdk_include_path = vitasdk_sysroot.join("include");
    let vitasdk_lib_path = vitasdk_sysroot.join("lib");

    let kubridge_path = PathBuf::from("kubridge");

    if is_profiling() {
        println!("cargo:rustc-link-arg=-pg");
        println!("cargo::rustc-env=CC=-pg");
        println!("cargo::rustc-cfg=profiling");
    }

    if is_target_vita() {
        let bindings_file = out_path.join("imgui_bindings.rs");

        const IMGUI_HEADERS: [&str; 3] = ["imgui.h", "imgui_internal.h", "imgui_impl_vitagl.h"];
        let mut bindings = create_bindgen_builder()
            .clang_args(["-x", "c++"])
            .clang_arg("-std=c++17")
            .formatter(Formatter::Prettyplease)
            .allowlist_item("ImGui.*")
            .opaque_type("std::.*")
            .use_core()
            .enable_cxx_namespaces()
            .trust_clang_mangling(true);
        for header in IMGUI_HEADERS {
            let header_path = vitasdk_include_path.join(header);
            println!("cargo:rerun-if-changed={}", header_path.to_str().unwrap());
            bindings = bindings.header(header_path.to_str().unwrap());
        }
        bindings.rust_target(bindgen::RustTarget::nightly()).generate().unwrap().write_to_file(bindings_file).unwrap();

        println!("cargo:rustc-link-search=native={}", vitasdk_lib_path.to_str().unwrap());
        println!("cargo:rustc-link-lib=static=imgui");
    } else if !is_host_linux() {
        generate_linux_imgui_bindings();
    }

    if !is_target_vita() {
        return;
    }

    {
        let bindings_file = out_path.join("kubridge_bindings.rs");

        const KUBRIDGE_HEADERS: [&str; 1] = ["kubridge.h"];
        let mut bindings = create_bindgen_builder()
            .clang_args(["-I", kubridge_path.to_str().unwrap()])
            .clang_args(["-x", "c++"])
            .clang_arg("-std=c++17")
            .formatter(Formatter::Prettyplease);
        for header in KUBRIDGE_HEADERS {
            let header_path = kubridge_path.join(header);
            println!("cargo:rerun-if-changed={}", header_path.to_str().unwrap());
            bindings = bindings.header(header_path.to_str().unwrap());
        }
        bindings.rust_target(bindgen::RustTarget::nightly()).generate().unwrap().write_to_file(bindings_file).unwrap();
    }

    {
        let kubridge_out = out_path.join("kubridge");
        Command::new("cmake")
            .arg("-DCMAKE_POLICY_VERSION_MINIMUM=3.5")
            .arg("-B")
            .arg(&kubridge_out)
            .arg("-S")
            .arg(&kubridge_path)
            .status()
            .unwrap();
        Command::new("cmake").arg("--build").arg(&kubridge_out).status().unwrap();
        let kubridge_lib_path = kubridge_out.join("libkubridge_stub.a");
        let kubridge_lib_new_path = kubridge_out.join("libkubridge_stub_dsvita.a");
        fs::rename(kubridge_lib_path, kubridge_lib_new_path).unwrap();

        println!("cargo:rerun-if-changed={}", kubridge_path.to_str().unwrap());
        println!("cargo:rustc-link-search=native={}", fs::canonicalize(kubridge_out).unwrap().to_str().unwrap());
        println!("cargo:rustc-link-lib=static=kubridge_stub_dsvita");
    }

    if is_profiling() {
        let gprof_out = out_path.join("vita-gprof");
        Command::new("cmake")
            .arg("-DCMAKE_POLICY_VERSION_MINIMUM=3.5")
            .arg("-B")
            .arg(&gprof_out)
            .arg("-S")
            .arg("vita-gprof")
            .status()
            .unwrap();
        Command::new("cmake").arg("--build").arg(&gprof_out).status().unwrap();

        println!("cargo:rerun-if-changed=vita-gprof");
        println!("cargo:rustc-link-search=native={}", fs::canonicalize(gprof_out).unwrap().to_str().unwrap());
        println!("cargo:rustc-link-lib=static=vitagprof");
    }
}
