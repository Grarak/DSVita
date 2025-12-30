use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};
use vitabuild::{get_out_path, is_debug};

fn get_source_files(path: &Path, files: &mut Vec<PathBuf>) {
    let dir = path.read_dir().unwrap();
    for entry in dir {
        if let Ok(entry) = entry {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(extension) = entry.path().extension() {
                        if extension == "h" || extension == "c" {
                            files.push(entry.path());
                        }
                    }
                } else if file_type.is_dir() {
                    get_source_files(entry.path().as_path(), files);
                }
            }
        }
    }
}

fn main() {
    let vita_gl_path = PathBuf::from("vitaGL_src");
    let vita_gl_source_path = vita_gl_path.join("source");
    let mut source_files = Vec::new();
    get_source_files(&vita_gl_source_path, &mut source_files);

    for file in source_files {
        println!("cargo:rerun-if-changed={}", file.to_str().unwrap());
    }

    let mut vita_gl_envs = vec![
        ("HAVE_UNFLIPPED_FBOS", "1"),
        ("NO_TEX_COMBINER", "1"),
        ("HAVE_SHADER_CACHE", "1"),
        ("SINGLE_THREADED_GC", "1"),
        ("BUFFERS_SPEEDHACK", "1"),
        ("DRAW_SPEEDHACK", "2"),
        ("INDICES_SPEEDHACK", "1"),
    ];

    if !is_debug() {
        vita_gl_envs.push(("NO_DEBUG", "1"));
    } else {
        vita_gl_envs.push(("HAVE_SHARK_LOG", "1"));
        vita_gl_envs.push(("LOG_ERRORS", "1"));
        vita_gl_envs.push(("DEBUG_GC", "1"));
    }
    // vita_gl_envs.push(("HAVE_DEVKIT", "1"));
    // vita_gl_envs.push(("HAVE_CPU_TRACER", "1"));

    let vita_gl_lib_path = vita_gl_path.join("libvitaGL.a");
    let vita_gl_lib_new_path = vita_gl_path.join("libvitaGL_dsvita.a");

    let num_jobs = env::var("NUM_JOBS").unwrap();
    Command::new("make").current_dir(&vita_gl_path).arg("clean").status().unwrap();
    Command::new("make").current_dir(&vita_gl_path).args(["-j", &num_jobs]).envs(vita_gl_envs).status().unwrap();

    fs::rename(vita_gl_lib_path, &vita_gl_lib_new_path).unwrap();
    println!("cargo:rustc-link-search=native={}", fs::canonicalize(&vita_gl_path).unwrap().to_str().unwrap());
    println!("cargo:rustc-link-lib=static=vitaGL_dsvita");

    let git_hash = Command::new("git").current_dir(&vita_gl_path).args(["rev-parse", "HEAD"]).output().unwrap().stdout;
    let out_path = get_out_path();
    File::create(out_path.join("vita_gl_version")).unwrap().write_all(&git_hash).unwrap();
}
