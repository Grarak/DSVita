use bindgen::Formatter;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use vitabuild::{create_bindgen_builder, create_cc_build, get_out_path, get_vitasdk_path, is_debug, is_host_linux, is_target_vita};

fn main() {
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
    ];
    if is_debug() {
        vixl_flags.push("-DVIXL_DEBUG=1".to_string());
    }
    if let Some(vitasdk_path) = get_vitasdk_path() {
        if is_target_vita() || !is_host_linux() {
            vixl_flags.push(format!("--sysroot={}", vitasdk_path.join("arm-vita-eabi").to_str().unwrap()));
        }
    }

    let out_path = get_out_path();
    let vixl_path = Path::new("vixl_src");
    println!("cargo:rerun-if-changed={}", vixl_path.to_str().unwrap());

    let create_vixl_build = |src_files: &[&str]| {
        let mut vixl_build = create_cc_build();
        vixl_build.include(vixl_path.join("src")).cpp(true);

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

    let mut vixl_expand_build = create_vixl_build(&["aarch32/macro-assembler-aarch32.cc"]);
    vixl_expand_build.opt_level(0);

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

    let mut bindings = create_bindgen_builder()
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

    let vixl_build = create_vixl_build(vixl_files);
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
