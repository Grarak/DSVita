use std::path::PathBuf;
use std::{env, fs};

const COMMON_C_FLAGS: &[&str] = &["-mtune=cortex-a9", "-mfloat-abi=hard", "-mfpu=neon", "-mthumb"];

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
        flags.push("--target=armv7-unknown-linux-gnueabihf".to_string());
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
    if is_target_vita() {
        if let Some(vitasdk_path) = get_vitasdk_path() {
            build
                .compiler(vitasdk_path.join("bin").join("arm-vita-eabi-gcc"))
                .archiver(vitasdk_path.join("bin").join("arm-vita-eabi-gcc-ar"))
                .ranlib(vitasdk_path.join("bin").join("arm-vita-eabi-gcc-ranlib"))
                .pic(false);
        }
    } else {
        build.compiler("clang");
    }

    for flag in get_common_c_flags() {
        build.flag(flag);
    }

    if !is_debug() && is_opt_build() {
        build.flag("-flto").opt_level_str("fast");
        if is_target_vita() {
            build.flag("-ffat-lto-objects");
        }
    }
    build
}

pub fn create_cc_build() -> cc::Build {
    let mut build = cc::Build::new();
    if is_target_vita() {
        if let Some(vitasdk_path) = get_vitasdk_path() {
            build
                .compiler(vitasdk_path.join("bin").join("arm-vita-eabi-g++"))
                .archiver(vitasdk_path.join("bin").join("arm-vita-eabi-gcc-ar"))
                .ranlib(vitasdk_path.join("bin").join("arm-vita-eabi-gcc-ranlib"))
                .pic(false);
        }
    } else {
        build.compiler("clang++");
    }

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
        build.flag("-flto").opt_level_str("fast");
        if is_target_vita() {
            build.flag("-ffat-lto-objects");
        }
    }

    for flag in get_common_c_flags() {
        build.flag(flag);
    }
    build
}

pub fn create_bindgen_builder() -> bindgen::Builder {
    let mut bindgen = bindgen::Builder::default();
    bindgen = bindgen.clang_arg("--target=armv7-unknown-linux-gnueabihf");
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
