// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) 2018-2021 Andre Richter <andre.o.richter@gmail.com>

//! BSP Memory Management Unit.

use crate::{
    common,
    memory::{
        mmu as generic_mmu,
        mmu::{
            AccessPermissions, AddressSpace, AssociatedTranslationTable, AttributeFields,
            MemAttributes, Page, PageSliceDescriptor, TranslationGranule,
        },
        Physical, Virtual,
    },
    synchronization::InitStateLock,
};

//--------------------------------------------------------------------------------------------------
// Private Definitions
//--------------------------------------------------------------------------------------------------

type KernelTranslationTable =
    <KernelVirtAddrSpace as AssociatedTranslationTable>::TableStartFromBottom;

//--------------------------------------------------------------------------------------------------
// Public Definitions
//--------------------------------------------------------------------------------------------------

/// The translation granule chosen by this BSP. This will be used everywhere else in the kernel to
/// derive respective data structures and their sizes. For example, the `crate::memory::mmu::Page`.
pub type KernelGranule = TranslationGranule<{ 64 * 1024 }>;

/// The kernel's virtual address space defined by this BSP.
pub type KernelVirtAddrSpace = AddressSpace<{ 8 * 1024 * 1024 * 1024 }>;

//--------------------------------------------------------------------------------------------------
// Global instances
//--------------------------------------------------------------------------------------------------

/// The kernel translation tables.
///
/// It is mandatory that InitStateLock is transparent.
///
/// That is, `size_of(InitStateLock<KernelTranslationTable>) == size_of(KernelTranslationTable)`.
/// There is a unit tests that checks this porperty.
static KERNEL_TABLES: InitStateLock<KernelTranslationTable> =
    InitStateLock::new(KernelTranslationTable::new());

//--------------------------------------------------------------------------------------------------
// Private Code
//--------------------------------------------------------------------------------------------------

/// Helper function for calculating the number of pages the given parameter spans.
const fn size_to_num_pages(size: usize) -> usize {
    assert!(size > 0);
    assert!(size % KernelGranule::SIZE == 0);

    size >> KernelGranule::SHIFT
}

/// The boot core's stack.
fn virt_stack_page_desc() -> PageSliceDescriptor<Virtual> {
    let num_pages = size_to_num_pages(super::boot_core_stack_size());

    PageSliceDescriptor::from_addr(super::virt_boot_core_stack_start(), num_pages)
}

/// The Read-Only (RO) pages of the kernel binary.
fn virt_ro_page_desc() -> PageSliceDescriptor<Virtual> {
    let num_pages = size_to_num_pages(super::ro_size());

    PageSliceDescriptor::from_addr(super::virt_ro_start(), num_pages)
}

/// The data pages of the kernel binary.
fn virt_data_page_desc() -> PageSliceDescriptor<Virtual> {
    let num_pages = size_to_num_pages(super::data_size());

    PageSliceDescriptor::from_addr(super::virt_data_start(), num_pages)
}

// The binary is still identity mapped, so we don't need to convert in the following.

/// The boot core's stack.
fn phys_stack_page_desc() -> PageSliceDescriptor<Physical> {
    virt_stack_page_desc().into()
}

/// The Read-Only (RO) pages of the kernel binary.
fn phys_ro_page_desc() -> PageSliceDescriptor<Physical> {
    virt_ro_page_desc().into()
}

/// The data pages of the kernel binary.
fn phys_data_page_desc() -> PageSliceDescriptor<Physical> {
    virt_data_page_desc().into()
}

//--------------------------------------------------------------------------------------------------
// Public Code
//--------------------------------------------------------------------------------------------------

/// Return a reference to the kernel's translation tables.
pub fn kernel_translation_tables() -> &'static InitStateLock<KernelTranslationTable> {
    &KERNEL_TABLES
}

/// Pointer to the last page of the physical address space.
pub fn phys_addr_space_end_page() -> *const Page<Physical> {
    common::align_down(
        super::phys_addr_space_end().into_usize(),
        KernelGranule::SIZE,
    ) as *const Page<_>
}

/// Map the kernel binary.
///
/// # Safety
///
/// - Any miscalculation or attribute error will likely be fatal. Needs careful manual checking.
pub unsafe fn kernel_map_binary() -> Result<(), &'static str> {
    generic_mmu::kernel_map_pages_at(
        "Kernel boot-core stack",
        &virt_stack_page_desc(),
        &phys_stack_page_desc(),
        &AttributeFields {
            mem_attributes: MemAttributes::CacheableDRAM,
            acc_perms: AccessPermissions::ReadWrite,
            execute_never: true,
        },
    )?;

    generic_mmu::kernel_map_pages_at(
        "Kernel code and RO data",
        &virt_ro_page_desc(),
        &phys_ro_page_desc(),
        &AttributeFields {
            mem_attributes: MemAttributes::CacheableDRAM,
            acc_perms: AccessPermissions::ReadOnly,
            execute_never: false,
        },
    )?;

    generic_mmu::kernel_map_pages_at(
        "Kernel data and bss",
        &virt_data_page_desc(),
        &phys_data_page_desc(),
        &AttributeFields {
            mem_attributes: MemAttributes::CacheableDRAM,
            acc_perms: AccessPermissions::ReadWrite,
            execute_never: true,
        },
    )?;

    Ok(())
}

//--------------------------------------------------------------------------------------------------
// Testing
//--------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use test_macros::kernel_test;

    /// Check alignment of the kernel's virtual memory layout sections.
    #[kernel_test]
    fn virt_mem_layout_sections_are_64KiB_aligned() {
        for i in [virt_stack_page_desc, virt_ro_page_desc, virt_data_page_desc].iter() {
            let start: usize = i().start_addr().into_usize();
            let end: usize = i().end_addr().into_usize();

            assert_eq!(start % KernelGranule::SIZE, 0);
            assert_eq!(end % KernelGranule::SIZE, 0);
            assert!(end >= start);
        }
    }

    /// Ensure the kernel's virtual memory layout is free of overlaps.
    #[kernel_test]
    fn virt_mem_layout_has_no_overlaps() {
        let layout = [
            virt_stack_page_desc(),
            virt_ro_page_desc(),
            virt_data_page_desc(),
        ];

        for (i, first_range) in layout.iter().enumerate() {
            for second_range in layout.iter().skip(i + 1) {
                assert!(!first_range.contains(second_range.start_addr()));
                assert!(!first_range.contains(second_range.end_addr_inclusive()));
                assert!(!second_range.contains(first_range.start_addr()));
                assert!(!second_range.contains(first_range.end_addr_inclusive()));
            }
        }
    }

    /// Check if KERNEL_TABLES is in .bss.
    #[kernel_test]
    fn kernel_tables_in_bss() {
        let bss_range = super::super::bss_range_inclusive();
        let kernel_tables_addr = &KERNEL_TABLES as *const _ as usize as *mut u64;

        assert!(bss_range.contains(&kernel_tables_addr));
    }
}
