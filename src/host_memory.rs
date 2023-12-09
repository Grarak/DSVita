use crate::hle::memory::regions::MemoryRegion;
use crate::mmap::Mmap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::{io, slice};

pub struct VMmap<'a> {
    vm_begin_addr: *const u8,
    offset: u32,
    size: usize,
    phantom_data: PhantomData<&'a u8>,
}

impl<'a> VMmap<'a> {
    fn new(vm_begin_addr: *const u8, offset: u32, size: usize) -> Self {
        VMmap {
            vm_begin_addr,
            offset,
            size,
            phantom_data: PhantomData,
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        (self.vm_begin_addr as u32 - self.offset) as _
    }

    pub fn as_ptr(&self) -> *const u8 {
        (self.vm_begin_addr as u32 - self.offset) as _
    }

    pub fn len(&self) -> usize {
        self.size + self.offset as usize
    }
}

impl<'a> Deref for VMmap<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl<'a> DerefMut for VMmap<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

impl<'a> AsRef<[u8]> for VMmap<'a> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl<'a> AsMut<[u8]> for VMmap<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

pub struct VmManager {
    regions: &'static [&'static MemoryRegion],
    pub vm: Mmap,
}

impl VmManager {
    pub fn new(name: &str, regions: &'static [&'static MemoryRegion]) -> io::Result<Self> {
        let vm = Mmap::new(
            name,
            false,
            regions[regions.len() - 1].offset + regions[regions.len() - 1].size - regions[0].offset,
        )?;

        Ok(VmManager { regions, vm })
    }

    pub fn offset(&self) -> u32 {
        self.regions[0].offset
    }

    pub fn vm_begin_addr(&self) -> *const u8 {
        self.vm.as_ptr()
    }

    pub fn get_vm_mapping(&self) -> VMmap<'_> {
        VMmap::new(self.vm_begin_addr(), self.offset(), self.vm.len())
    }
}
