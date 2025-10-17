use riscv_transpiler::vm::RAM;

#[cfg(feature = "evaluate")]
pub mod evaluate;

pub(crate) mod usize_slice_iterator;

pub(crate) trait U32Memory {
    fn read_word(&self, address: u32) -> u32;
}

impl U32Memory for risc_v_simulator::abstractions::memory::VectorMemoryImpl {
    fn read_word(&self, address: u32) -> u32 {
        <Self as risc_v_simulator::abstractions::memory::MemorySource>::get_noexcept(self, address as u64)
    }
}

impl U32Memory for risc_v_simulator::abstractions::memory::VectorMemoryImplWithRom {
    fn read_word(&self, address: u32) -> u32 {
        <Self as risc_v_simulator::abstractions::memory::MemorySource>::get_noexcept(self, address as u64)
    }
}

impl<const ROM_BOUND_SECOND_WORD_BITS: usize> U32Memory for riscv_transpiler::vm::RamWithRomRegion<ROM_BOUND_SECOND_WORD_BITS> {
    fn read_word(&self, address: u32) -> u32 {
        self.peek_word(address)
    }
}