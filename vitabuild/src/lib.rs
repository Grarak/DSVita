use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{env, fs};

// armv7-only flags: cortex-a9 tuning, thumb2, and the tpidruro tls read apply to the two
// 32-bit arm targets; other host arches only get the warning suppression.
const ARM32_C_FLAGS: &[&str] = &[
    "-mcpu=cortex-a9",
    "-mfpu=neon",
    "-mthumb",
    "-Wno-invalid-constexpr",
    "-Xclang",
    "-target-feature",
    "-Xclang",
    "-read-tp-tpidruro",
];

const PORTABLE_C_FLAGS: &[&str] = &["-Wno-invalid-constexpr"];

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

pub fn is_target_arm32() -> bool {
    let target = env::var("TARGET").unwrap();
    target.starts_with("thumbv7") || target.starts_with("armv7")
}

pub const ANDROID_API: u32 = 30;

pub fn is_target_android() -> bool {
    env::var("TARGET").unwrap() == "aarch64-linux-android"
}

// The Android build uses the host clang-21 with only the NDK's sysroot (the official
// NDK ships x86_64 host binaries — useless on the aarch64 dev box). ANDROID_NDK_HOME
// points at the NDK root, mirroring the DSVITA_SYSROOT convention of the armhf cross.
pub fn get_android_ndk_prebuilt() -> PathBuf {
    let ndk = env::var("ANDROID_NDK_HOME").expect("ANDROID_NDK_HOME must point at an NDK (r27+) for aarch64-linux-android builds");
    PathBuf::from(ndk).join("toolchains/llvm/prebuilt/linux-x86_64")
}

pub fn get_android_sysroot() -> PathBuf {
    get_android_ndk_prebuilt().join("sysroot")
}

// The NDK's clang resource dir; handed to the *link* step so -rtlib=compiler-rt finds
// libclang_rt.builtins-aarch64-android.a (the host clang's own resource dir lacks it).
pub fn get_android_resource_dir() -> PathBuf {
    let clang_dir = get_android_ndk_prebuilt().join("lib/clang");
    let version = clang_dir.read_dir().unwrap().next().unwrap().unwrap();
    version.path()
}

pub fn get_common_c_flags() -> Vec<String> {
    let base = if is_target_arm32() { ARM32_C_FLAGS } else { PORTABLE_C_FLAGS };
    let mut flags = base.to_vec().iter().map(|flag| flag.to_string()).collect::<Vec<_>>();
    if is_target_android() {
        // The API level rides in the triple; the plain rustc TARGET has none.
        flags.push(format!("--target=aarch64-linux-android{ANDROID_API}"));
        flags.push(format!("--sysroot={}", get_android_sysroot().to_str().unwrap()));
    } else if !is_target_vita() {
        flags.push(format!("--target={}", env::var("TARGET").unwrap()));
        // DSVITA_SYSROOT is the armhf cross sysroot; native hosts use their own.
        if is_target_arm32() {
            if let Ok(sysroot) = env::var("DSVITA_SYSROOT") {
                flags.push(format!("--sysroot={sysroot}"));
            }
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
    build.compiler("clang-21").archiver("llvm-ar-21").pic(is_target_android());

    for flag in get_common_c_flags() {
        build.flag(flag);
    }

    // No cross-language LTO into the Android .so: the archives would carry clang-21
    // bitcode while the final link may also see NDK-flavored inputs — keep it plain.
    if !is_debug() && is_opt_build() && !is_target_android() {
        build.flag("-flto=full");
    }
    build
}

pub fn create_cc_build() -> cc::Build {
    let mut build = cc::Build::new();
    build.cpp(true);
    build.compiler("clang++-21").archiver("llvm-ar-21").pic(is_target_android());
    if is_target_android() {
        // Static libc++: no libc++_shared.so to bundle into the APK. libc++abi is added
        // separately in build.rs (NDK splits them).
        build.cpp_link_stdlib("c++_static");
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

    if !is_debug() && is_opt_build() && !is_target_android() {
        build.flag("-flto=full");
    }

    for flag in get_common_c_flags() {
        build.flag(flag);
    }
    build
}

pub fn create_bindgen_builder() -> bindgen::Builder {
    let mut bindgen = bindgen::Builder::default();
    // Both 32-bit arm targets share the thumbv7 layout; other arches bind with their own
    // triple so struct layouts (pointer width, long) come out right.
    if is_target_arm32() {
        bindgen = bindgen.clang_arg("--target=thumbv7neon-unknown-linux-gnueabihf");
    } else if is_target_android() {
        bindgen = bindgen.clang_arg(format!("--target=aarch64-linux-android{ANDROID_API}"));
        bindgen = bindgen.clang_arg(format!("--sysroot={}", get_android_sysroot().to_str().unwrap()));
    } else {
        bindgen = bindgen.clang_arg(format!("--target={}", env::var("TARGET").unwrap()));
    }
    if !is_target_vita() && is_target_arm32() {
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
