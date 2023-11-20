use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=VITASDK");
    match env::var("VITASDK") {
        Ok(vitasdk) => {
            let sysroot = Path::new(&vitasdk).join("arm-vita-eabi");

            assert!(
                sysroot.exists(),
                "VITASDK's sysroot does not exist, please install or update vitasdk first"
            );

            let lib = sysroot.join("lib");
            assert!(lib.exists(), "VITASDK's `lib` directory does not exist");
            println!("cargo:rustc-link-search=native={}", lib.display());
            sysroot
        }
        Err(env::VarError::NotPresent) => {
            panic!("VITASDK env var is not set")
        }
        Err(env::VarError::NotUnicode(s)) => {
            panic!("VITASDK env var is not a valid unicode but got: {s:?}")
        }
    };
}
