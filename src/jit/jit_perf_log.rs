use crate::mmap::PAGE_SIZE;
use libc::{clock_gettime, mmap, CLOCK_MONOTONIC, MAP_FAILED, MAP_PRIVATE, PROT_EXEC, PROT_READ};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::ops::Sub;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::thread::Thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{mem, process, ptr, thread};

const JIT_HEADER_MAGIC: u32 = 0x4A695444;
const JIT_VERSION: u32 = 1;

fn get_timestamp() -> u64 {
    unsafe {
        let mut ts = mem::zeroed();
        if clock_gettime(CLOCK_MONOTONIC, &mut ts) != 0 {
            0
        } else {
            ts.tv_sec as u64 * 1000000000 + ts.tv_nsec as u64
        }
    }
}

#[repr(C)]
struct JitHeader {
    magic: u32,
    version: u32,
    total_size: u32,
    elf_mach: u32,
    pad1: u32,
    pid: u32,
    timestamp: u64,
    flags: u64,
}

#[repr(u32)]
enum JitRecordType {
    CodeLoad = 0,
    CodeMove = 1,
    DebugInfo = 2,
    Close = 3,
}

#[repr(C)]
struct JitRecord {
    id: u32,
    total_size: u32,
    timestamp: u64,
}

impl JitRecord {
    fn new<T>(id: JitRecordType) -> Self {
        JitRecord {
            id: id as u32,
            total_size: size_of::<T>() as u32,
            timestamp: get_timestamp(),
        }
    }
}

#[repr(C)]
struct JitCodeLoad {
    record: JitRecord,
    pid: u32,
    tid: u32,
    vma: u64,
    code_addr: u64,
    code_size: u64,
    code_index: u64,
}

impl JitCodeLoad {
    fn new(addr: usize, size: usize, index: u64) -> Self {
        JitCodeLoad {
            record: JitRecord::new::<Self>(JitRecordType::CodeLoad),
            pid: process::id(),
            tid: thread::current().id().as_u64().get() as u32,
            vma: addr as u64,
            code_addr: addr as u64,
            code_size: size as u64,
            code_index: index,
        }
    }
}

impl JitHeader {
    fn new() -> Self {
        JitHeader {
            magic: JIT_HEADER_MAGIC,
            version: JIT_VERSION,
            total_size: size_of::<JitHeader>() as u32,
            elf_mach: 0x28,
            pad1: 0,
            pid: process::id(),
            timestamp: get_timestamp(),
            flags: 0,
        }
    }
}

pub struct JitPerfLog {
    file: File,
    index: u64,
}

impl JitPerfLog {
    pub fn new(dir: PathBuf) -> Self {
        let header = JitHeader::new();
        let mut file = OpenOptions::new().create_new(true).read(true).write(true).open(dir.join(format!("jit-{}.dump", header.pid))).unwrap();

        let ptr = unsafe { mmap(ptr::null_mut(), PAGE_SIZE as _, PROT_READ | PROT_EXEC, MAP_PRIVATE, file.as_raw_fd() as _, 0) };
        assert_ne!(ptr, MAP_FAILED);

        let header: [u8; size_of::<JitHeader>()] = unsafe { mem::transmute(header) };
        file.write_all(&header).unwrap();
        file.flush().unwrap();

        JitPerfLog { file, index: 0 }
    }

    pub fn load(&mut self, name: &str, code: &[u8], thumb: bool) {
        let mut code_load = JitCodeLoad::new((code.as_ptr() as usize) | (thumb as usize), code.len(), self.index);
        self.index += 1;

        code_load.record.total_size += name.len() as u32 + 1 + code.len() as u32;
        let code_load: [u8; size_of::<JitCodeLoad>()] = unsafe { mem::transmute(code_load) };

        self.file.write_all(&code_load).unwrap();
        self.file.write_all(name.as_bytes()).unwrap();
        self.file.write_all(&[0]).unwrap();
        self.file.write_all(code).unwrap();
        self.file.flush().unwrap();
    }
}
