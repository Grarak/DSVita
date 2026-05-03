use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{env, fs};

const COMMON_C_FLAGS: &[&str] = &[
    "-mcpu=cortex-a9",
    "-mfpu=neon",
    "-mthumb",
    "-Wno-invalid-constexpr",
    "-Xclang",
    "-target-feature",
    "-Xclang",
    "-read-tp-tpidruro",
];

pub fn get_profile_name() -> String {
    get_out_path().to_str().unwrap().split(std::path::MAIN_SEPARATOR).nth_back(3).unwrap().to_string()
}

pub fn is_profiling() -> bool {
    get_profile_name() == "release-profiling"
}

pub fn get_out_path() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").unwrap())
}

pub fn is_opt_build() -> bool {
    env::var("OPT_LEVEL").unwrap_or("0".to_string()) == "3"
}

pub fn is_debug() -> bool {
    env::var("DEBUG").unwrap_or("false".to_string()) == "true"
}

pub fn get_vitasdk_path() -> Option<PathBuf> {
    env::var("VITASDK").ok().map(PathBuf::from)
}

pub fn is_host_linux() -> bool {
    cfg!(unix) && fs::exists("/proc").unwrap()
}

pub fn is_target_vita() -> bool {
    let target = env::var("TARGET").unwrap();
    target == "armv7-sony-vita-newlibeabihf"
}

pub fn get_common_c_flags() -> Vec<String> {
    let mut flags = COMMON_C_FLAGS.to_vec().iter().map(|flag| flag.to_string()).collect::<Vec<_>>();
    if !is_target_vita() {
        flags.push(format!("--target={}", env::var("TARGET").unwrap()));
        if let Ok(sysroot) = env::var("DSVITA_SYSROOT") {
            flags.push(format!("--sysroot={sysroot}"));
        }
    }
    if is_profiling() {
        flags.push("-pg".to_string());
    }
    if let Some(vitasdk_path) = get_vitasdk_path() {
        if is_target_vita() || !is_host_linux() {
            flags.push(format!("--sysroot={}", vitasdk_path.join("arm-vita-eabi").to_str().unwrap()))
        }
    }
    flags
}

pub fn create_c_build() -> cc::Build {
    let mut build = cc::Build::new();
    build.compiler("clang-21").archiver("llvm-ar-21").pic(false);

    for flag in get_common_c_flags() {
        build.flag(flag);
    }

    if !is_debug() && is_opt_build() {
        build.flag("-flto=full");
    }
    build
}

pub fn create_cc_build() -> cc::Build {
    let mut build = cc::Build::new();
    build.cpp(true);
    build.compiler("clang++-21").archiver("llvm-ar-21").pic(false);

    if let Some(vitasdk_path) = get_vitasdk_path() {
        if is_target_vita() || !is_host_linux() {
            let cpp_include_path = vitasdk_path.join("arm-vita-eabi").join("include/c++");
            let dir = fs::read_dir(cpp_include_path).unwrap();
            let version = dir.into_iter().next().unwrap().unwrap();
            let cpp_include_path = version.path();

            build.include(cpp_include_path.to_str().unwrap()).include(cpp_include_path.join("arm-vita-eabi").to_str().unwrap());
        }
    }

    if !is_debug() && is_opt_build() {
        build.flag("-flto=full");
    }

    for flag in get_common_c_flags() {
        build.flag(flag);
    }
    build
}

pub fn create_bindgen_builder() -> bindgen::Builder {
    let mut bindgen = bindgen::Builder::default();
    bindgen = bindgen.clang_arg("--target=thumbv7neon-unknown-linux-gnueabihf");
    if !is_target_vita() {
        if let Ok(sysroot) = env::var("DSVITA_SYSROOT") {
            bindgen = bindgen.clang_arg(format!("--sysroot={sysroot}"));
        }
    }
    if let Some(vitasdk_path) = get_vitasdk_path() {
        if is_target_vita() || !is_host_linux() {
            bindgen = bindgen.clang_arg(format!("--sysroot={}", vitasdk_path.join("arm-vita-eabi").to_str().unwrap()));
        }
    }
    bindgen
}

pub fn bindgen_generate_to_file(builder: bindgen::Builder, file: impl AsRef<Path>) {
    let bindings = builder.generate().unwrap().to_string();
    let bindings = bindings.replace("#[link_name = \"\\u{1}_", "#[link_name = \"_");
    File::create(file.as_ref()).unwrap().write_all(bindings.as_bytes()).unwrap();
}
