use std::path::Path;
use vitabuild::{create_bindgen_builder, create_c_build, get_out_path};

fn main() {
    const C_FILES: &[&str] = &[
        "src/rapi/rc_api_common.c",
        "src/rapi/rc_api_editor.c",
        "src/rapi/rc_api_info.c",
        "src/rapi/rc_api_runtime.c",
        "src/rapi/rc_api_user.c",
        "src/rc_client_external.c",
        "src/rc_client.c",
        "src/rc_compat.c",
        "src/rc_util.c",
        "src/rc_version.c",
        "src/rcheevos/alloc.c",
        "src/rcheevos/condition.c",
        "src/rcheevos/condset.c",
        "src/rcheevos/consoleinfo.c",
        "src/rcheevos/format.c",
        "src/rcheevos/lboard.c",
        "src/rcheevos/memref.c",
        "src/rcheevos/operand.c",
        "src/rcheevos/rc_validate.c",
        "src/rcheevos/richpresence.c",
        "src/rcheevos/runtime_progress.c",
        "src/rcheevos/runtime.c",
        "src/rcheevos/trigger.c",
        "src/rcheevos/value.c",
        "src/rhash/aes.c",
        "src/rhash/cdreader.c",
        "src/rhash/hash_disc.c",
        "src/rhash/hash_encrypted.c",
        "src/rhash/hash_rom.c",
        "src/rhash/hash_zip.c",
        "src/rhash/hash.c",
        "src/rhash/md5.c",
    ];

    const HEADER_FILES: &[&str] = &[
        "include/rc_api_editor.h",
        "include/rc_api_info.h",
        "include/rc_api_request.h",
        "include/rc_api_runtime.h",
        "include/rc_api_user.h",
        "include/rc_client.h",
        "include/rc_consoles.h",
        "include/rc_error.h",
        "include/rc_export.h",
        "include/rc_hash.h",
        "include/rc_runtime_types.h",
        "include/rc_runtime.h",
        "include/rc_util.h",
        "include/rcheevos.h",
        "src/rapi/rc_api_common.h",
        "src/rc_client_external_versions.h",
        "src/rc_client_external.h",
        "src/rc_client_internal.h",
        "src/rc_compat.h",
        "src/rc_version.h",
        "src/rcheevos/rc_internal.h",
        "src/rcheevos/rc_validate.h",
        "src/rhash/aes.h",
        "src/rhash/md5.h",
        "src/rhash/rc_hash_internal.h",
    ];

    const FLAGS: &[&str] = &["-DRC_CLIENT_SUPPORTS_EXTERNAL", "-DRC_CLIENT_SUPPORTS_HASH"];

    let path = Path::new("rcheevos_src");

    for file in HEADER_FILES {
        let file = path.join(file);
        println!("cargo:rerun-if-changed={}", file.to_str().unwrap());
    }

    let mut build = create_c_build();
    for file in C_FILES {
        let file = path.join(file);
        println!("cargo:rerun-if-changed={}", file.to_str().unwrap());
        build.file(file);
    }
    for flag in FLAGS {
        build.flag(flag);
    }
    build.include(path.join("include"));

    build.compile("rcheevos");

    let out_path = get_out_path();
    let bindings_file = out_path.join("rcheevos_bindings.rs");

    create_bindgen_builder()
        .header(path.join("include/rc_client.h").to_str().unwrap())
        .header(path.join("include/rc_consoles.h").to_str().unwrap())
        .clang_args(FLAGS)
        .allowlist_function("rc_.*")
        .allowlist_var("RC_.*")
        .generate()
        .unwrap()
        .write_to_file(bindings_file)
        .unwrap();
}
