use crate::core::memory::regions;
use crate::core::CpuType;
use crate::jit::jit_memory::{JitBlock, JitLookups};
use crate::utils::HeapMemU32;

const BLOCK_SHIFT: usize = 13;
const BLOCK_SIZE: usize = 1 << BLOCK_SHIFT;
const SIZE: usize = (1 << 30) / BLOCK_SIZE;
const SIZE_THUMB: usize = (1 << 31) / BLOCK_SIZE;

pub struct JitMemoryMap {
    map_arm9: HeapMemU32<SIZE>,
    map_arm9_thumb: HeapMemU32<SIZE_THUMB>,
    map_arm7: HeapMemU32<SIZE>,
    map_arm7_thumb: HeapMemU32<SIZE_THUMB>,
}

impl JitMemoryMap {
    pub fn new(lookups: &JitLookups) -> Self {
        let mut instance = JitMemoryMap {
            map_arm9: HeapMemU32::new(),
            map_arm9_thumb: HeapMemU32::new(),
            map_arm7: HeapMemU32::new(),
            map_arm7_thumb: HeapMemU32::new(),
        };

        macro_rules! get_ptr {
            ($addr:expr, $lookup:expr) => {{
                (unsafe { $lookup.as_ptr().add($addr % $lookup.len()) } as u32)
            }};
        }

        for i in 0..SIZE {
            let addr = (i << BLOCK_SHIFT) << 2;
            let arm9_ptr = &mut instance.map_arm9[i];
            let arm7_ptr = &mut instance.map_arm7[i];

            match (addr as u32) & 0xFF000000 {
                0 => {
                    *arm9_ptr = get_ptr!(addr >> 2, lookups.itcm);
                    *arm7_ptr = get_ptr!(addr >> 2, lookups.arm7_bios);
                }
                regions::INSTRUCTION_TCM_MIRROR_OFFSET => *arm9_ptr = get_ptr!(addr >> 2, lookups.itcm),
                regions::MAIN_MEMORY_OFFSET => {
                    *arm9_ptr = get_ptr!(addr >> 2, lookups.main_arm9);
                    *arm7_ptr = get_ptr!(addr >> 2, lookups.main_arm7);
                }
                regions::SHARED_WRAM_OFFSET => *arm7_ptr = get_ptr!(addr >> 2, lookups.wram),
                regions::VRAM_OFFSET => *arm7_ptr = get_ptr!(addr >> 2, lookups.vram_arm7),
                0xFF000000 => *arm9_ptr = get_ptr!(addr >> 2, lookups.arm9_bios),
                _ => {}
            }
        }

        for i in 0..SIZE_THUMB {
            let addr = (i << BLOCK_SHIFT) << 1;
            let arm9_ptr = &mut instance.map_arm9_thumb[i];
            let arm7_ptr = &mut instance.map_arm7_thumb[i];

            match (addr as u32) & 0xFF000000 {
                0 => {
                    *arm9_ptr = get_ptr!(addr >> 1, lookups.itcm_thumb);
                    *arm7_ptr = get_ptr!(addr >> 1, lookups.arm7_bios_thumb);
                }
                regions::INSTRUCTION_TCM_MIRROR_OFFSET => *arm9_ptr = get_ptr!(addr >> 1, lookups.itcm_thumb),
                regions::MAIN_MEMORY_OFFSET => {
                    *arm9_ptr = get_ptr!(addr >> 1, lookups.main_arm9_thumb);
                    *arm7_ptr = get_ptr!(addr >> 1, lookups.main_arm7_thumb);
                }
                regions::SHARED_WRAM_OFFSET => *arm7_ptr = get_ptr!(addr >> 1, lookups.wram_thumb),
                regions::VRAM_OFFSET => *arm7_ptr = get_ptr!(addr >> 1, lookups.vram_arm7_thumb),
                0xFF000000 => *arm9_ptr = get_ptr!(addr >> 1, lookups.arm9_bios_thumb),
                _ => {}
            }
        }

        instance
    }

    pub fn get_jit_block<const CPU: CpuType, const THUMB: bool>(&self, addr: u32) -> *const JitBlock {
        let addr = addr >> if THUMB { 1 } else { 2 };
        macro_rules! get_jit_block {
            ($map:expr) => {{
                unsafe { *($map[(addr >> BLOCK_SHIFT) as usize] as *const *const JitBlock).add((addr as usize) & (BLOCK_SIZE - 1)) }
            }};
        }
        match CPU {
            CpuType::ARM9 => {
                if THUMB {
                    get_jit_block!(self.map_arm9_thumb)
                } else {
                    get_jit_block!(self.map_arm9)
                }
            }
            CpuType::ARM7 => {
                if THUMB {
                    get_jit_block!(self.map_arm7_thumb)
                } else {
                    get_jit_block!(self.map_arm7)
                }
            }
        }
    }
}
