pub fn align_up(n: u32, align: u32) -> u32 {
    (n + align - 1) & !(align - 1)
}
