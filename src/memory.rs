use crate::{generated::Engine, simd::V128};
impl Engine {
    #[inline(always)]
    pub(crate) fn i32_load(&self, p: usize) -> u32 {
        u32::from_le_bytes(self.memory[p..p + 4].try_into().unwrap())
    }
    #[inline(always)]
    pub(crate) fn i64_load(&self, p: usize) -> u64 {
        u64::from_le_bytes(self.memory[p..p + 8].try_into().unwrap())
    }
    #[inline(always)]
    pub(crate) fn i32_load8_u(&self, p: usize) -> u32 {
        self.memory[p] as u32
    }
    #[inline(always)]
    pub(crate) fn i32_load16_u(&self, p: usize) -> u32 {
        u16::from_le_bytes(self.memory[p..p + 2].try_into().unwrap()) as u32
    }
    #[inline(always)]
    pub(crate) fn i32_load16_s(&self, p: usize) -> u32 {
        i16::from_le_bytes(self.memory[p..p + 2].try_into().unwrap()) as i32 as u32
    }
    #[inline(always)]
    pub(crate) fn i32_store(&mut self, p: usize, v: u32) {
        self.memory[p..p + 4].copy_from_slice(&v.to_le_bytes());
    }
    #[inline(always)]
    pub(crate) fn i64_store(&mut self, p: usize, v: u64) {
        self.memory[p..p + 8].copy_from_slice(&v.to_le_bytes());
    }
    #[inline(always)]
    pub(crate) fn i32_store8(&mut self, p: usize, v: u32) {
        self.memory[p] = v as u8;
    }
    #[inline(always)]
    pub(crate) fn i32_store16(&mut self, p: usize, v: u32) {
        self.memory[p..p + 2].copy_from_slice(&(v as u16).to_le_bytes());
    }
    #[inline(always)]
    pub(crate) fn v128_load(&self, p: usize) -> V128 {
        V128(self.memory[p..p + 16].try_into().unwrap())
    }
    #[inline(always)]
    pub(crate) fn v128_store(&mut self, p: usize, v: V128) {
        self.memory[p..p + 16].copy_from_slice(&v.0);
    }
    #[inline]
    pub(crate) fn copy(&mut self, dst: u32, src: u32, n: u32) {
        self.memory
            .copy_within(src as usize..src as usize + n as usize, dst as usize);
    }
    #[inline]
    pub(crate) fn fill(&mut self, dst: u32, val: u32, n: u32) {
        self.memory[dst as usize..dst as usize + n as usize].fill(val as u8);
    }
    pub(crate) fn grow(&mut self, pages: u32) -> u32 {
        let old = self.memory.len() / 65536;
        let Some(new) = old.checked_add(pages as usize).filter(|&n| n < 65536) else {
            return u32::MAX;
        };
        let bytes = new * 65536;
        if self.memory.try_reserve(bytes - self.memory.len()).is_err() {
            return u32::MAX;
        }
        self.memory.resize(bytes, 0);
        old as u32
    }
    fn id_class(&self, ptr: u32, len: u32, ranges: &[(u32, u32)]) -> u32 {
        let p = ptr as usize;
        let b = &self.memory[p..p + 4];
        let cp = match len {
            2 => ((b[0] as u32 & 31) << 6) | (b[1] as u32 & 63),
            3 => ((b[0] as u32 & 15) << 12) | ((b[1] as u32 & 63) << 6) | (b[2] as u32 & 63),
            _ => {
                ((b[0] as u32 & 7) << 18)
                    | ((b[1] as u32 & 63) << 12)
                    | ((b[2] as u32 & 63) << 6)
                    | (b[3] as u32 & 63)
            }
        };
        let i = ranges.partition_point(|&(_, end)| end < cp);
        (i < ranges.len() && ranges[i].0 <= cp) as u32
    }
    #[inline]
    pub(crate) fn w_is_id_start(&mut self, p: u32, n: u32) -> u32 {
        self.id_class(p, n, crate::unicode::START)
    }
    #[inline]
    pub(crate) fn w_is_id_continue(&mut self, p: u32, n: u32) -> u32 {
        self.id_class(p, n, crate::unicode::CONTINUE)
    }
}
