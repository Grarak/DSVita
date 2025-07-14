use bindgen::Formatter;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

fn main() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-env-changed=OUT_DIR");

    let build_profile_name = out_path.to_str().unwrap().split(std::path::MAIN_SEPARATOR).nth_back(3).unwrap();
    let build_profile_name_file = out_path.join("build_profile_name");
    File::create(build_profile_name_file).unwrap().write_all(build_profile_name.as_bytes()).unwrap();

    let target = env::var("TARGET").unwrap();
    let is_target_vita = target == "armv7-sony-vita-newlibeabihf";
    if !is_target_vita {
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

    let is_host_linux = cfg!(unix) && fs::exists("/proc").unwrap();

    let vitasdk_path = env::var("VITASDK").map(PathBuf::from);
    println!("cargo:rerun-if-env-changed=VITASDK");

    {
        let mut vixl_flags = vec![
            "-Wall".to_string(),
            "-fdiagnostics-show-option".to_string(),
            "-Wextra".to_string(),
            "-Wredundant-decls".to_string(),
            "-pedantic".to_string(),
            "-Wwrite-strings".to_string(),
            "-Wunused".to_string(),
            "-Wshadow".to_string(),
            "-Wno-missing-noreturn".to_string(),
            "-DVIXL_CODE_BUFFER_MALLOC=1".to_string(),
            "-DVIXL_INCLUDE_TARGET_A32=1".to_string(),
            "-DVIXL_INCLUDE_TARGET_T32=1".to_string(),
            "-std=c++17".to_string(),
            "-mtune=cortex-a9".to_string(),
            "-mfpu=neon".to_string(),
        ];
        if build_profile_name != "release" {
            vixl_flags.push("-DVIXL_DEBUG=1".to_string());
        }
        if !is_target_vita {
            vixl_flags.push("--target=armv7-unknown-linux-gnueabihf".to_string());
        }
        if let Ok(vitasdk_path) = &vitasdk_path {
            if is_target_vita || !is_host_linux {
                vixl_flags.push(format!("--sysroot={}", vitasdk_path.join("arm-vita-eabi").to_str().unwrap()));
            }
        }

        let vixl_path = Path::new("vixl");
        println!("cargo:rerun-if-changed={}", vixl_path.to_str().unwrap());

        let create_vixl_build = |src_files: &[&str]| {
            let mut vixl_build = cc::Build::new();
            vixl_build.include(vixl_path.join("src")).cpp(true);
            if is_target_vita {
                vixl_build
                    .compiler(vitasdk_path.as_ref().unwrap().join("bin").join("arm-vita-eabi-g++"))
                    .archiver(vitasdk_path.as_ref().unwrap().join("bin").join("arm-vita-eabi-gcc-ar"))
                    .ranlib(vitasdk_path.as_ref().unwrap().join("bin").join("arm-vita-eabi-gcc-ranlib"))
                    .pic(false);
            } else {
                vixl_build.compiler("clang++");
            }

            if let Ok(vitasdk_path) = &vitasdk_path {
                if is_target_vita || !is_host_linux {
                    let cpp_include_path = vitasdk_path.join("arm-vita-eabi").join("include/c++");
                    let dir = fs::read_dir(cpp_include_path).unwrap();
                    let version = dir.into_iter().next().unwrap().unwrap();
                    let cpp_include_path = version.path();

                    vixl_build.include(cpp_include_path.to_str().unwrap()).include(cpp_include_path.join("arm-vita-eabi").to_str().unwrap());
                }
            }

            for flag in &vixl_flags {
                vixl_build.flag(flag);
            }

            for file in src_files {
                if file.starts_with("/") {
                    vixl_build.file(file);
                } else {
                    let file_path = vixl_path.join("src").join(file);
                    vixl_build.file(file_path.to_str().unwrap());
                }
            }

            vixl_build
        };

        let vixl_expand_build = create_vixl_build(&["aarch32/macro-assembler-aarch32.cc"]);

        let out = vixl_expand_build.expand();
        let vixl_masm_file = out_path.join("vixl_masm.cpp");
        File::create(&vixl_masm_file).unwrap().write_all(&out).unwrap();

        let clang_format_output = match Command::new("clang-format").arg("-style").arg("{ColumnLimit: 99999}").arg(vixl_masm_file).output() {
            Ok(output) => output,
            Err(err) => panic!("Failed to run clang-format: {err}"),
        };
        assert!(clang_format_output.status.success(), "{clang_format_output:?}");

        let output = String::from_utf8(clang_format_output.stdout).unwrap();
        let fun_regex = Regex::new(r"void ([A-Z][a-z]*?)\(Condition cond.*?\) \{").unwrap();
        let output_lines = output.split("\n");
        let mut functions = HashSet::new();
        output_lines.clone().for_each(|line| {
            if let Some(capture) = fun_regex.captures(line) {
                functions.insert(capture.get(1).unwrap().as_str().to_string());
            }
        });
        let fun_regex = Regex::new(r"void ([A-Z][a-z]*?)\((.*?)\) \{").unwrap();

        let vixl_bindings_impl_path = out_path.join("vixl-aarch32-bindings.cc");
        let vixl_bindings_header_path = out_path.join("vixl-aarch32-bindings.h");

        let mut vixl_bindings_impl_file = File::create(&vixl_bindings_impl_path).unwrap();
        writeln!(vixl_bindings_impl_file, "#include \"vixl-aarch32-bindings.h\"\n#include \"aarch32/wrapper-aarch32-structs.h\"\n").unwrap();

        let mut vixl_bindings_header_file = File::create(&vixl_bindings_header_path).unwrap();
        writeln!(
            vixl_bindings_header_file,
            "#ifndef VIXL_AARCH32_BINDINGS_AARCH32_H_\n#define VIXL_AARCH32_BINDINGS_AARCH32_H_\n\n#include \"aarch32/wrapper-aarch32.h\"\n"
        )
        .unwrap();

        let mut vixl_mapping = HashMap::<String, Vec<String>>::new();

        for line in output_lines {
            if let Some(capture) = fun_regex.captures(line) {
                let fun_name = capture.get(1).unwrap().as_str().to_string();
                if functions.contains(&fun_name) {
                    let params = capture.get(2).unwrap().as_str().to_string();
                    match vixl_mapping.get_mut(&fun_name) {
                        Some(vec) => vec.push(params),
                        None => {
                            vixl_mapping.insert(fun_name.to_string(), vec![params]);
                        }
                    }
                }
            }
        }

        for (fun_name, variants) in &vixl_mapping {
            'outer: for params in variants {
                let params = params.split(",").map(|v| v.trim()).filter(|v| !v.is_empty());
                let mut fun_params = "".to_string();
                let mut delegate_params = "".to_string();
                for param in params {
                    let values = param.split(" ").collect::<Vec<_>>();
                    let (prefix, t, name) = if values[0] == "const" { ("const ", values[1], values[2]) } else { ("", values[0], values[1]) };
                    let mut t = t.to_string();
                    if t == "T" {
                        continue 'outer;
                    }
                    let is_ptr = name.as_bytes()[0] == b'*' || name.as_bytes()[0] == b'&';
                    let mut delegate_name = name.to_string();
                    if is_ptr {
                        delegate_name = delegate_name[1..].to_string();
                    }

                    if t == "DOperand" || t == "QOperand" || t == "SOperand" || t == "RawLiteral" || t == "Label" {
                        t = format!("Aarch32{t}");
                    }
                    fun_params += &format!(", {prefix}{t} {name}");
                    delegate_params += &format!("{delegate_name}, ");
                }
                if !delegate_params.is_empty() {
                    delegate_params = delegate_params[..delegate_params.len() - 2].to_string();
                }
                let fun = format!("void masm_{}(Aarch32MacroAssembler *masm{fun_params})", fun_name.to_lowercase());
                writeln!(vixl_bindings_header_file, "{fun};").unwrap();
                writeln!(vixl_bindings_impl_file, "{fun} {{ masm->{fun_name}({delegate_params}); }}").unwrap();
            }
        }

        writeln!(vixl_bindings_header_file, "\n#endif").unwrap();

        let bindings_file = out_path.join("vixl_bindings.rs");

        let mut bindings = bindgen::Builder::default()
            .clang_args(["-x", "c++"])
            .clang_args(["-I", vixl_path.join("src").to_str().unwrap()])
            .clang_args(["-target", "armv7-unknown-linux-gnueabihf"])
            .formatter(Formatter::Prettyplease)
            .header(vixl_bindings_header_path.to_str().unwrap());

        for flag in &vixl_flags {
            bindings = bindings.clang_arg(flag);
        }

        bindings.rust_target(bindgen::RustTarget::nightly()).generate().unwrap().write_to_file(bindings_file).unwrap();

        let vixl_files: &[&str] = &[
            "code-buffer-vixl.cc",
            "compiler-intrinsics-vixl.cc",
            "cpu-features.cc",
            "utils-vixl.cc",
            "aarch32/assembler-aarch32.cc",
            "aarch32/constants-aarch32.cc",
            "aarch32/disasm-aarch32.cc",
            "aarch32/instructions-aarch32.cc",
            "aarch32/location-aarch32.cc",
            "aarch32/macro-assembler-aarch32.cc",
            "aarch32/operands-aarch32.cc",
            "aarch32/wrapper-aarch32.cc",
            vixl_bindings_impl_path.to_str().unwrap(),
        ];

        let mut vixl_build = create_vixl_build(vixl_files);
        if build_profile_name == "release" {
            vixl_build.flag("-flto").flag("-ffat-lto-objects").opt_level_str("fast");
        }
        vixl_build.compile("vixl");

        let vixl_inst_wrapper_path = out_path.join("vixl_inst_wrapper.rs");
        let mut vixl_inst_wrapper_file = File::create(vixl_inst_wrapper_path).unwrap();

        for (fun_name, variants) in vixl_mapping {
            let mut emitted_param_counts = HashSet::new();
            let mut variant_i = 0;
            'outer: for params in variants {
                let params = params.split(",").map(|v| v.trim()).filter(|v| !v.is_empty()).collect::<Vec<_>>();

                let mut generic_types = "".to_string();
                let mut fun_params = "".to_string();
                let mut delegate_params = "".to_string();
                for param in &params {
                    let values = param.split(" ").collect::<Vec<_>>();
                    let (prefix, t, name) = if values[0] == "const" { ("const", values[1], values[2]) } else { ("", values[0], values[1]) };
                    let is_ptr = name.as_bytes()[0] == b'*' || name.as_bytes()[0] == b'&';

                    let t = t.to_string();
                    if t == "T" {
                        continue 'outer;
                    }
                    let mut rust_type = t.clone();
                    if t == "uint32_t" {
                        rust_type = "u32".to_string();
                    } else if t == "int32_t" {
                        rust_type = "i32".to_string();
                    } else if t == "unsigned" {
                        rust_type = "u32".to_string();
                    } else if t == "uint64_t" {
                        rust_type = "u64".to_string();
                    } else if t == "float" {
                        rust_type = "f32".to_string();
                    } else if t == "double" {
                        rust_type = "f64".to_string();
                    } else if t == "Condition" {
                        rust_type = "Cond".to_string();
                    } else if t == "Register" {
                        rust_type = "Reg".to_string();
                    } else if t == "RegisterList" {
                        rust_type = "RegReserve".to_string();
                    }

                    let has_ptr_inner = t == "DOperand" || t == "QOperand" || t == "SOperand" || t == "RawLiteral" || t == "Label";

                    let mut name = name.to_string();
                    if is_ptr {
                        rust_type = if prefix == "const" { format!("&{rust_type}") } else { format!("&mut {rust_type}") };
                        name = name[1..].to_string();
                    }
                    generic_types += &format!("{rust_type}, ");
                    fun_params += &format!("{name}: {rust_type}, ");
                    if has_ptr_inner {
                        delegate_params += &format!("{name}.inner as _, ");
                    } else if t == "Condition" {
                        delegate_params += &format!("Condition::from({name}), ");
                    } else if t == "Register" {
                        delegate_params += &format!("Register::from({name}), ");
                    } else if t == "RegisterList" {
                        delegate_params += &format!("RegisterList::from({name}), ");
                    } else {
                        delegate_params += &format!("{name}, ");
                    }
                }
                if !generic_types.is_empty() {
                    generic_types = generic_types[..generic_types.len() - 2].to_string();
                    fun_params = fun_params[..fun_params.len() - 2].to_string();
                    delegate_params = delegate_params[..delegate_params.len() - 2].to_string();
                }

                if emitted_param_counts.insert(params.len()) {
                    let mut generics = "".to_string();
                    let mut generic_params = "".to_string();
                    for i in 0..params.len() {
                        generics += &format!("A{}, ", i + 1);
                        generic_params += &format!(", a{}: A{}", i + 1, i + 1);
                    }
                    if !params.is_empty() {
                        generics = generics[..generics.len() - 2].to_string();
                        writeln!(
                            vixl_inst_wrapper_file,
                            r"pub trait Masm{fun_name}{}<{generics}> {{
    #[allow(dead_code)]
    fn {}{}(&mut self{generic_params});
}}
",
                            params.len(),
                            fun_name.to_lowercase(),
                            params.len(),
                        )
                        .unwrap();
                    } else {
                        writeln!(
                            vixl_inst_wrapper_file,
                            r"pub trait Masm{fun_name} {{
    #[allow(dead_code)]
    fn {}0(&mut self{generic_params});
}}
",
                            fun_name.to_lowercase(),
                        )
                        .unwrap();
                    }
                }

                if !generic_types.is_empty() {
                    writeln!(
                        vixl_inst_wrapper_file,
                        r"impl Masm{fun_name}{}<{generic_types}> for MacroAssembler {{
    fn {}{}(&mut self, {fun_params}) {{
        unsafe {{ masm_{}{}(self.inner, {delegate_params}) }}
    }}
}}
",
                        params.len(),
                        fun_name.to_lowercase(),
                        params.len(),
                        fun_name.to_lowercase(),
                        if variant_i == 0 { "".to_string() } else { variant_i.to_string() },
                    )
                    .unwrap();
                } else {
                    writeln!(
                        vixl_inst_wrapper_file,
                        r"impl Masm{fun_name} for MacroAssembler {{
    fn {}0(&mut self) {{
        unsafe {{ masm_{}{}(self.inner) }}
    }}
}}
",
                        fun_name.to_lowercase(),
                        fun_name.to_lowercase(),
                        if variant_i == 0 { "".to_string() } else { variant_i.to_string() },
                    )
                    .unwrap();
                }

                variant_i += 1;
            }
        }
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
        let mut soundtouch_includes = vec![
            soundtouch_path.join("include").to_str().unwrap().to_string(),
            soundtouch_path.join("source").join("SoundTouch").to_str().unwrap().to_string(),
        ];

        let mut soundtouch_flags = vec![
            "-DSOUNDTOUCH_INTEGER_SAMPLES=1".to_string(),
            "-std=c++17".to_string(),
            "-DST_NO_EXCEPTION_HANDLING=1".to_string(),
            "-mtune=cortex-a9".to_string(),
            "-mfpu=neon".to_string(),
            "-DM_PI=3.14159265358979323846".to_string(),
        ];

        if !is_target_vita {
            soundtouch_flags.push("--target=armv7-unknown-linux-gnueabihf".to_string());
        }

        if let Ok(vitasdk_path) = &vitasdk_path {
            if is_target_vita || !is_host_linux {
                let cpp_include_path = vitasdk_path.join("arm-vita-eabi").join("include/c++");
                let dir = fs::read_dir(cpp_include_path).unwrap();
                let version = dir.into_iter().next().unwrap().unwrap();
                let cpp_include_path = version.path();

                soundtouch_includes.push(cpp_include_path.to_str().unwrap().to_string());
                soundtouch_includes.push(cpp_include_path.join("arm-vita-eabi").to_str().unwrap().to_string());
                soundtouch_flags.push(format!("--sysroot={}", vitasdk_path.join("arm-vita-eabi").to_str().unwrap()));
            }
        }

        let mut soundtouch_build = cc::Build::new();
        soundtouch_build.cpp(true);
        if is_target_vita {
            soundtouch_build
                .compiler(vitasdk_path.as_ref().unwrap().join("bin").join("arm-vita-eabi-g++"))
                .archiver(vitasdk_path.as_ref().unwrap().join("bin").join("arm-vita-eabi-gcc-ar"))
                .ranlib(vitasdk_path.as_ref().unwrap().join("bin").join("arm-vita-eabi-gcc-ranlib"))
                .pic(false);
        } else {
            soundtouch_build.compiler("clang++");
        }
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

        if build_profile_name == "release" {
            soundtouch_build.flag("-flto").flag("-ffat-lto-objects").opt_level_str("fast");
        }
        soundtouch_build.compile("soundtouch");

        let bindings_file = out_path.join("soundtouch_bindings.rs");

        let mut soundtouch_bindgen = bindgen::Builder::default().clang_args(["-x", "c++"]).clang_args(["-target", "armv7-unknown-linux-gnueabihf"]);
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
            println!("cargo:rerun-if-changed={}", header_path.to_str().unwrap());
            bindings = bindings.header(header_path.to_str().unwrap());
        }
        bindings.rust_target(bindgen::RustTarget::nightly()).generate().unwrap().write_to_file(bindings_file).unwrap();

        if is_target_vita {
            println!("cargo:rustc-link-search=native={}", vitasdk_lib_path.to_str().unwrap());
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
            println!("cargo:rerun-if-changed={}", header_path.to_str().unwrap());
            bindings = bindings.header(header_path.to_str().unwrap());
        }
        bindings.rust_target(bindgen::RustTarget::nightly()).generate().unwrap().write_to_file(bindings_file).unwrap();
    }

    if !is_target_vita {
        return;
    }

    {
        let mut vita_gl_envs = vec![
            ("HAVE_UNFLIPPED_FBOS", "1"),
            ("NO_TEX_COMBINER", "1"),
            ("MATH_SPEEDHACK", "1"),
            ("HAVE_SHADER_CACHE", "1"),
            ("SINGLE_THREADED_GC", "1"),
        ];

        if build_profile_name == "release" {
            vita_gl_envs.push(("NO_DEBUG", "1"));
        } else {
            vita_gl_envs.push(("HAVE_SHARK_LOG", "1"));
            vita_gl_envs.push(("LOG_ERRORS", "2"));
            vita_gl_envs.push(("HAVE_PROFILING", "1"));
            // vita_gl_envs.push(("HAVE_RAZOR", "1"));
        }

        let vita_gl_path = PathBuf::from("vitaGL");
        let vita_gl_lib_path = vita_gl_path.join("libvitaGL.a");
        let vita_gl_lib_new_path = vita_gl_path.join("libvitaGL_dsvita.a");

        Command::new("make").current_dir("vitaGL").arg("clean").status().unwrap();
        Command::new("make").current_dir("vitaGL").args(["-j", &num_jobs]).envs(vita_gl_envs).status().unwrap();

        fs::rename(vita_gl_lib_path, vita_gl_lib_new_path).unwrap();
        println!("cargo:rustc-link-search=native={}", fs::canonicalize(vita_gl_path).unwrap().to_str().unwrap());
        println!("cargo:rustc-link-lib=static=vitaGL_dsvita");
    }

    {
        let kubridge_dst_path = cmake::Config::new(&kubridge_path)
            .configure_arg("-DCMAKE_POLICY_VERSION_MINIMUM=3.5")
            .build_target("libkubridge_stub.a")
            .build()
            .join("build");
        let kubridge_lib_path = kubridge_dst_path.join("libkubridge_stub.a");
        let kubridge_lib_new_path = kubridge_dst_path.join("libkubridge_stub_dsvita.a");
        fs::rename(kubridge_lib_path, kubridge_lib_new_path).unwrap();

        println!("cargo:rerun-if-changed={}", kubridge_path.to_str().unwrap());
        println!("cargo:rustc-link-search=native={}", fs::canonicalize(kubridge_dst_path).unwrap().to_str().unwrap());
        println!("cargo:rustc-link-lib=static=kubridge_stub_dsvita");
    }
}
