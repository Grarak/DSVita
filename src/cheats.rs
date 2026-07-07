use crate::core::emu::Emu;
use crate::core::CpuType::ARM7;
use crate::logging::debug_println;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub struct Cheat {
    pub name: String,
    // Action Replay DS bytecode, pairs of u32 per code line
    pub code: Vec<u32>,
    pub enabled: bool,
}

// The cheat list lives in a global so the ui thread (pause menu toggles) and the cpu
// thread (per-frame apply at the vblank hook) share it without threading a reference
// through the presenter; same pattern as the savestate request channel
static CHEATS: Mutex<Vec<Cheat>> = Mutex::new(Vec::new());
// Per-frame gate so frames without any enabled cheat skip the mutex entirely
static ANY_ENABLED: AtomicBool = AtomicBool::new(false);

fn update_any_enabled(cheats: &[Cheat]) {
    ANY_ENABLED.store(cheats.iter().any(|cheat| cheat.enabled), Ordering::Relaxed);
}

pub fn any_enabled() -> bool {
    ANY_ENABLED.load(Ordering::Relaxed)
}

pub fn cheats_path(rom_path: &Path) -> PathBuf {
    let stem = rom_path.file_stem().unwrap_or_default().to_string_lossy();
    rom_path.parent().unwrap_or(Path::new(".")).join("cheats").join(format!("{stem}.cht"))
}

fn is_cheat_header(line: &str) -> bool {
    line.starts_with('[')
}

// Load the cheat list for a game, replacing whatever the previous game left behind.
// NooDS-compatible .cht format: "[name]+" ('+' enabled, '-' disabled) starts a cheat,
// followed by "XXXXXXXX XXXXXXXX" code lines; anything unparsable is skipped
pub fn load(rom_path: &Path) {
    let cheats = match std::fs::read_to_string(cheats_path(rom_path)) {
        Ok(content) => parse(&content),
        Err(_) => Vec::new(),
    };
    update_any_enabled(&cheats);
    *CHEATS.lock().unwrap() = cheats;
}

fn serialize(cheats: &[Cheat]) -> String {
    let mut out = String::new();
    for cheat in cheats {
        out += &format!("[{}]{}\n", cheat.name, if cheat.enabled { '+' } else { '-' });
        for line in cheat.code.chunks(2) {
            out += &format!("{:08X} {:08X}\n", line[0], line.get(1).copied().unwrap_or(0));
        }
        out += "\n";
    }
    out
}

// Write the list back in the same format load parses
pub fn save(rom_path: &Path) -> std::io::Result<()> {
    let path = cheats_path(rom_path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = serialize(&CHEATS.lock().unwrap());
    std::fs::write(path, out)
}

pub fn names_and_states() -> Vec<(String, bool)> {
    CHEATS.lock().unwrap().iter().map(|cheat| (cheat.name.clone(), cheat.enabled)).collect()
}

pub fn set_enabled(index: usize, enabled: bool) {
    let mut cheats = CHEATS.lock().unwrap();
    if let Some(cheat) = cheats.get_mut(index) {
        cheat.enabled = enabled;
    }
    update_any_enabled(&cheats);
}

// Run all enabled cheats, called once per frame at vblank
pub fn apply(emu: &mut Emu) {
    if !any_enabled() {
        return;
    }
    let cheats = CHEATS.lock().unwrap();
    for cheat in cheats.iter().filter(|cheat| cheat.enabled) {
        run_cheat(emu, &cheat.code);
    }
}

// Parsing split from fs so tests can drive it with in-memory strings
fn parse(content: &str) -> Vec<Cheat> {
    let mut cheats = Vec::new();
    for line in content.lines() {
        let line = line.trim_end();
        if is_cheat_header(line) {
            let Some(end) = line.rfind(']') else { continue };
            cheats.push(Cheat {
                // Names come from a user-edited file and reach CString in the ui
                name: line[1..end].replace('\0', ""),
                code: Vec::new(),
                enabled: line[end..].ends_with('+'),
            });
        } else if let Some(cheat) = cheats.last_mut() {
            for word in line.split_whitespace() {
                if let Ok(value) = u32::from_str_radix(word, 16) {
                    cheat.code.push(value);
                }
            }
        }
    }
    cheats
}

// Action Replay DS bytecode interpreter, ported from NooDS (action_replay.cpp).
// Memory goes through the arm7 map: cheats target main ram and (shared) wram, which
// the arm7 view covers without the arm9 tcm windows shadowing anything
fn run_cheat(emu: &mut Emu, code: &[u32]) {
    let mut offset = 0u32;
    let mut data_reg = 0u32;
    let mut counter = 0u32;
    let mut loop_count = 0u32;
    let mut loop_address = 0u64;
    let mut cond_flag = false;

    // u64 so the parameter-copy skips of malformed codes can't overflow the index
    let mut addr = 0u64;
    while addr + 1 < code.len() as u64 {
        let op0 = code[addr as usize];
        let op1 = code[addr as usize + 1];

        if cond_flag {
            // Handle adjustments that happen regardless of the condition
            let op = op0 >> 24;
            if (op >> 4) == 0xE {
                // Parameter copy
                addr += ((op1 as u64 + 0x7) & !0x7) >> 2;
            } else if op == 0xC5 {
                // If counter
                counter = counter.wrapping_add(1);
            }

            // Skip non-control opcodes while the flag is set
            if op != 0xD0 && op != 0xD1 && op != 0xD2 {
                addr += 2;
                continue;
            }
        }

        match op0 >> 28 {
            // Write word/half/byte
            0x0 => emu.mem_write::<{ ARM7 }, u32>((op0 & 0xFFFFFFF).wrapping_add(offset), op1),
            0x1 => emu.mem_write::<{ ARM7 }, u16>((op0 & 0xFFFFFFF).wrapping_add(offset), op1 as u16),
            0x2 => emu.mem_write::<{ ARM7 }, u8>((op0 & 0xFFFFFFF).wrapping_add(offset), op1 as u8),
            // Word conditions: set the flag when the condition does NOT hold
            0x3..=0x6 => {
                let cond_addr = if op0 & 0xFFFFFFF != 0 { op0 & 0xFFFFFFF } else { offset };
                let mem = emu.mem_read::<{ ARM7 }, u32>(cond_addr);
                cond_flag = match op0 >> 28 {
                    0x3 => op1 <= mem, // If greater than word
                    0x4 => op1 >= mem, // If less than word
                    0x5 => op1 != mem, // If equal to word
                    _ => op1 == mem,   // If not equal to word
                };
            }
            // Half conditions, with the upper op1 half masking the memory value
            0x7..=0xA => {
                let cond_addr = if op0 & 0xFFFFFFF != 0 { op0 & 0xFFFFFFF } else { offset };
                let mem = emu.mem_read::<{ ARM7 }, u16>(cond_addr) as u32 & !(op1 >> 16);
                let value = op1 & 0xFFFF;
                cond_flag = match op0 >> 28 {
                    0x7 => value <= mem, // If greater than half
                    0x8 => value >= mem, // If less than half
                    0x9 => value != mem, // If equal to half
                    _ => value == mem,   // If not equal to half
                };
            }
            // Load offset
            0xB => offset = emu.mem_read::<{ ARM7 }, u32>((op0 & 0xFFFFFFF).wrapping_add(offset)),
            0xC => match op0 >> 24 {
                // For loop
                0xC0 => {
                    loop_count = op1;
                    loop_address = addr;
                }
                // If counter: flag when the masked counter isn't equal to the half
                0xC5 => {
                    counter = counter.wrapping_add(1);
                    cond_flag = (counter & op1 & 0xFFFF) != (op1 >> 16);
                }
                // Write offset
                0xC6 => emu.mem_write::<{ ARM7 }, u32>(op1, offset),
                _ => debug_println!("Invalid AR code: {op0:08X} {op1:08X}"),
            },
            0xD => match op0 >> 24 {
                // End if
                0xD0 => cond_flag = false,
                // Next loop
                0xD1 => {
                    if loop_count != 0 {
                        loop_count -= 1;
                        addr = loop_address;
                    } else {
                        cond_flag = false;
                    }
                }
                // Next loop and flush
                0xD2 => {
                    if loop_count != 0 {
                        loop_count -= 1;
                        addr = loop_address;
                    } else {
                        offset = 0;
                        data_reg = 0;
                        cond_flag = false;
                    }
                }
                // Set offset
                0xD3 => offset = op1,
                // Add data
                0xD4 => data_reg = data_reg.wrapping_add(op1),
                // Set data
                0xD5 => data_reg = op1,
                // Write data word/half/byte, post-incrementing the offset
                0xD6 => {
                    emu.mem_write::<{ ARM7 }, u32>(op1.wrapping_add(offset), data_reg);
                    offset = offset.wrapping_add(4);
                }
                0xD7 => {
                    emu.mem_write::<{ ARM7 }, u16>(op1.wrapping_add(offset), data_reg as u16);
                    offset = offset.wrapping_add(2);
                }
                0xD8 => {
                    emu.mem_write::<{ ARM7 }, u8>(op1.wrapping_add(offset), data_reg as u8);
                    offset = offset.wrapping_add(1);
                }
                // Read data word/half/byte
                0xD9 => data_reg = emu.mem_read::<{ ARM7 }, u32>(op1.wrapping_add(offset)),
                0xDA => data_reg = emu.mem_read::<{ ARM7 }, u16>(op1.wrapping_add(offset)) as u32,
                0xDB => data_reg = emu.mem_read::<{ ARM7 }, u8>(op1.wrapping_add(offset)) as u32,
                // Add offset
                0xDC => offset = offset.wrapping_add(op1),
                _ => debug_println!("Invalid AR code: {op0:08X} {op1:08X}"),
            },
            // Parameter copy: write op1 bytes of inline params, then skip them
            0xE => {
                for j in 0..op1 {
                    let value = (code.get(addr as usize + 2 + (j >> 2) as usize).copied().unwrap_or(0) >> ((j & 0x3) * 8)) as u8;
                    emu.mem_write::<{ ARM7 }, u8>((op0 & 0xFFFFFFF).wrapping_add(offset).wrapping_add(j), value);
                }
                addr += ((op1 as u64 + 0x7) & !0x7) >> 2;
            }
            // Memory copy: op1 bytes from [offset] to the immediate address
            // (`_` is exactly 0xF: a u32 >> 28 can't exceed it)
            _ => {
                for j in 0..op1 {
                    let value = emu.mem_read::<{ ARM7 }, u8>(offset.wrapping_add(j));
                    emu.mem_write::<{ ARM7 }, u8>((op0 & 0xFFFFFFF).wrapping_add(j), value);
                }
            }
        }

        addr += 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_noods_format() {
        let cheats = parse("[Max money]+\n0223CC44 000F423F\n1223CC48 0000270F\n\n[Walk anywhere]-\n923FDFFA 00000002\nD2000000 00000000\n\n");
        assert_eq!(cheats.len(), 2);
        assert_eq!(cheats[0].name, "Max money");
        assert!(cheats[0].enabled);
        assert_eq!(cheats[0].code, vec![0x0223CC44, 0x000F423F, 0x1223CC48, 0x0000270F]);
        assert_eq!(cheats[1].name, "Walk anywhere");
        assert!(!cheats[1].enabled);
        assert_eq!(cheats[1].code, vec![0x923FDFFA, 0x00000002, 0xD2000000, 0x00000000]);
    }

    #[test]
    fn parse_tolerates_junk() {
        // Codes before any header, unparsable hex and stray text are skipped
        let cheats = parse("02000000 00000001\nhello\n[A]+\nnothex 12345678\n0200000C 00000002\n");
        assert_eq!(cheats.len(), 1);
        assert_eq!(cheats[0].code, vec![0x12345678, 0x0200000C, 0x00000002]);
    }

    #[test]
    fn serialize_parse_roundtrip() {
        let original = vec![
            Cheat {
                name: "First".to_string(),
                code: vec![0x020F0000, 0xDEADBEEF],
                enabled: true,
            },
            Cheat {
                name: "Second [hard] mode".to_string(),
                code: vec![0xD3000000, 0x02100000, 0xD6000000, 0x00000000],
                enabled: false,
            },
        ];
        let reparsed = parse(&serialize(&original));
        assert_eq!(reparsed.len(), original.len());
        for (a, b) in reparsed.iter().zip(&original) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.code, b.code);
            assert_eq!(a.enabled, b.enabled);
        }
    }
}
