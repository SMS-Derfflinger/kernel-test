#![no_std]
#![no_main]
#![feature(naked_functions)]

mod rv64_mm;
mod fdt;

use fdt::*;

use buddy_allocator::{BuddyAllocator, BuddyRawPage};
use core::{
    arch::{global_asm, naked_asm},
    ptr::NonNull,
    sync::atomic::AtomicUsize,
};
use eonix_mm::{
    address::{Addr as _, AddrOps, PAddr, VAddr, VRange},
    page_table::{self, PageAttribute, PagingMode, RawAttribute, PTE as _},
    paging::{Page, PageAccess, PageAlloc, PageBlock, RawPage as RawPageTrait, PFN},
};
use intrusive_list::{container_of, Link};
use riscv::{asm::sfence_vma_all, register::satp};
use rv64_mm::*;
use spin::Mutex;

//global_asm!(include_str!("entry.S"));

#[link_section = ".bootstack"]
static mut BOOT_STACK: [u8; 4096 * 16] = [0; 4096 * 16];

#[repr(C, align(4096))]
struct BootPageTable([u64; PTES_PER_PAGE]);

#[link_section = ".bootdata"]
static mut BOOT_PAGE_TABLE: BootPageTable = {
    let mut arr: [u64; PTES_PER_PAGE] = [0; PTES_PER_PAGE];
    arr[2] = (0x80000 << 10) | 0xcf;
    arr[510] = (0x80000 << 10) | 0xcf;
    BootPageTable(arr)
};

#[link_section = ".bootdata"]
static mut PAGES: [RawPage; 1024] = [const { RawPage::new() }; 1024];

const fn page(index: usize) -> &'static mut RawPage {
    let page = unsafe { PAGES.as_mut_ptr().add(index) };
    unsafe { &mut *page }
}

fn print_number(number: usize) {
    if number == 0 {
        print("0");
        return;
    }

    let mut buffer = [0u8; 20];
    let mut index = 0;

    let mut num = number;
    while num > 0 {
        buffer[index] = (num % 10) as u8 + b'0';
        num /= 10;
        index += 1;
    }

    for i in (0..index).rev() {
        sbi::legacy::console_putchar(buffer[i]);
    }
}

#[derive(Clone, Copy)]
struct RawPageHandle(usize);

impl From<PFN> for RawPageHandle {
    fn from(pfn: PFN) -> Self {
        assert!(usize::from(pfn) - 0x80400 < 1024, "PFN out of range");

        Self(usize::from(pfn) - 0x80400)
    }
}

impl From<RawPageHandle> for PFN {
    fn from(raw_page: RawPageHandle) -> Self {
        PFN::from(raw_page.0 + 0x80400)
    }
}

impl RawPageTrait for RawPageHandle {
    fn order(&self) -> u32 {
        page(self.0).order
    }

    fn refcount(&self) -> &AtomicUsize {
        &page(self.0).refcount
    }

    fn is_present(&self) -> bool {
        self.0 < 1024
    }
}

impl BuddyRawPage for RawPageHandle {
    unsafe fn from_link(link: &mut Link) -> Self {
        let page = container_of!(link, RawPage, link);
        let page_index = page.as_ptr().offset_from_unsigned(PAGES.as_ptr()) as usize;
        assert!(page_index < 1024, "Page index out of range");

        Self(page_index)
    }

    unsafe fn get_link(&self) -> &mut Link {
        &mut page(self.0).link
    }

    fn set_order(&self, order: u32) {
        page(self.0).order = order;
    }

    fn is_buddy(&self) -> bool {
        page(self.0).buddy
    }

    fn is_free(&self) -> bool {
        page(self.0).free
    }

    fn set_buddy(&self) {
        page(self.0).buddy = true;
    }

    fn set_free(&self) {
        page(self.0).free = true;
    }

    fn clear_buddy(&self) {
        page(self.0).buddy = false;
    }

    fn clear_free(&self) {
        page(self.0).free = false;
    }
}

struct RawPage {
    link: Link,
    free: bool,
    buddy: bool,
    order: u32,
    refcount: AtomicUsize,
}

impl RawPage {
    const fn new() -> Self {
        Self {
            link: Link::new(),
            free: false,
            buddy: false,
            order: 0,
            refcount: AtomicUsize::new(0),
        }
    }
}

struct DirectPageAccess;

impl PageAccess for DirectPageAccess {
    unsafe fn get_ptr_for_pfn(pfn: PFN) -> NonNull<PageBlock> {
        unsafe { NonNull::new_unchecked(PAddr::from(pfn).addr() as *mut _) }
    }
}

#[link_section = ".bootdata"]
static BUDDY: Mutex<BuddyAllocator<RawPageHandle>> = Mutex::new(BuddyAllocator::new());

#[derive(Clone)]
struct BuddyPageAlloc;

impl PageAlloc for BuddyPageAlloc {
    type RawPage = RawPageHandle;

    fn alloc_order(&self, order: u32) -> Option<Self::RawPage> {
        let retval = BUDDY.lock().alloc_order(order);

        retval.inspect(|raw_page| {
            print("allocated: ");

            print_number(raw_page.0);

            print("\n");
        });

        retval
    }

    unsafe fn dealloc(&self, raw_page: Self::RawPage) {
        BUDDY.lock().dealloc(raw_page);
    }

    fn has_management_over(&self, page_ptr: Self::RawPage) -> bool {
        BuddyAllocator::has_management_over(page_ptr)
    }
}

type PageTable<'a> = eonix_mm::page_table::PageTable<'a, PagingModeSv39, BuddyPageAlloc, DirectPageAccess>;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(message) = info.message().as_str() {
        print("Panicked with message: \"");
        print(message);
        print("\" at ");
        print(info.location().unwrap().file());
        print(":");
        print_number(info.location().unwrap().line() as usize);
        print(":");
        print_number(info.location().unwrap().column() as usize);
        print("\n");
    } else {
        print("Panicked\n");
    }

    loop {}
}

fn print(string: &str) {
    for c in string.chars() {
        sbi::legacy::console_putchar(u8::try_from(c).unwrap_or(b'?'));
    }
}

fn read(buffer: &mut [u8]) -> Option<usize> {
    let mut iter = buffer.iter_mut();

    let mut count = None;
    loop {
        let Some(ch) = sbi::legacy::console_getchar() else {
            continue;
        };

        if ch == b'\r' {
            sbi::legacy::console_putchar(b'\n');
            break;
        }

        if let Some(out_ch) = iter.next() {
            sbi::legacy::console_putchar(ch);

            *out_ch = ch;
            *count.get_or_insert_default() += 1;
        } else {
            break;
        }
    }

    count
}

extern "C" {
    fn _ekernel();
}

/// bootstrap in rust
#[naked]
#[no_mangle]
#[link_section = ".text.entry"]
unsafe extern "C" fn _start(hart_id: usize, dtb_addr: usize) -> ! {
    naked_asm!(
        "
        la   sp, {boot_stack}
        la   t0, {page_table}
        srli t0, t0, 12
        li   t1, 8 << 60
        or   t0, t0, t1
        csrw satp, t0
        sfence.vma
        li   t2, {virt_ram_offset}
        or   sp, sp, t2
        la   t3, riscv64_start
        or   t3, t3, t2
        jalr t3                      // call riscv64_start
        ",
        boot_stack = sym BOOT_STACK,
        page_table = sym BOOT_PAGE_TABLE,
        virt_ram_offset = const KIMAGE_OFFSET,
    )
}

fn map_physical_memory(page_table: &PageTable, attr: PageAttribute) {
    let ekernel = _ekernel as usize - 0xffff_ffff_0000_0000;

    let start = PAddr::from(ekernel).ceil_to(PageSize::_4KbPage as usize);
    let end = PAddr::from(ekernel).ceil_to(PageSize::_2MbPage as usize);
    let size_4kb = end - start;
    let range = VRange::from(VAddr::from(PHYS_MAP_VIRT + start.addr())).grow(size_4kb);
    let pfn_start = start.addr() >> PAGE_SIZE_BITS;
    print_number(range.start().addr() - PHYS_MAP_VIRT);
    print("\n");
    print_number(start.addr());
    print("\n");
    for (idx, pte) in page_table
        .iter_kernel_levels(range, &PagingModeSv39::LEVELS[..=2])
        .enumerate()
    {
        pte.set(PFN::from(idx + pfn_start), PageAttribute64::from_page_attr(attr));
    }

    let start = end;
    let end = start.ceil_to(PageSize::_1GbPage as usize);
    let size_2mb = end - start;
    let range = VRange::from(VAddr::from(PHYS_MAP_VIRT + start.addr())).grow(size_2mb);
    let pfn_start = start.addr() >> PAGE_SIZE_BITS;
    print_number(range.start().addr() - PHYS_MAP_VIRT);
    print("\n");
    print_number(start.addr());
    print("\n");
    for (idx, pte) in page_table
        .iter_kernel_levels(range, &PagingModeSv39::LEVELS[..=1])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x200 + pfn_start), PageAttribute64::from_page_attr(attr));
    }

    let start = end;
    let size_1gb = MEMORY_SIZE;
    let range = VRange::from(VAddr::from(PHYS_MAP_VIRT + start.addr())).grow(size_1gb);
    let pfn_start = start.addr() >> PAGE_SIZE_BITS;
    print_number(range.start().addr() - PHYS_MAP_VIRT);
    print("\n");
    print_number(start.addr());
    print("\n");
    for (idx, pte) in page_table
        .iter_kernel_levels(range, &PagingModeSv39::LEVELS[..=0])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x40000 + pfn_start), PageAttribute64::from_page_attr(attr));
    }
}

global_asm!(
    r#"
    .section .text.ap_boot
    .globl ap_boot_entry

    ap_boot_entry:
        csrr a0, mhartid
    "#
);

extern "C" {
    fn ap_boot_entry();
}

#[no_mangle]
pub unsafe extern "C" fn riscv64_start(hart_id: usize, dtb_addr: usize) -> ! {
    print("\n");
    print("Hello World!\n");

    print("ap_boot_entry:\n");
    print_number(ap_boot_entry as usize - KIMAGE_OFFSET);
    print("\n");

    print("hart_id:\n");
    print_number(hart_id);
    print("\n");

    let num_harts = get_num_harts(dtb_addr);
    print_number(num_harts);
    print("\n");

    BUDDY
        .lock()
        .create_pages(PAddr::from(0x80400000), PAddr::from(0x80700000));

    let root_table_page = Page::alloc_in(BuddyPageAlloc);

    let page_table = PageTable::new_in(&root_table_page, BuddyPageAlloc);

    let attr = PageAttribute::WRITE
        | PageAttribute::READ
        | PageAttribute::EXECUTE
        | PageAttribute::GLOBAL
        | PageAttribute::PRESENT;

    // Map 0x00000000-0x7fffffff 2GB MMIO,
    // to 0xffff ffff 0000 0000 to 0xffff ffff 7ffff ffff, use 1GB page
    for (idx, pte) in page_table
        .iter_kernel_levels(VRange::from(VAddr::from(MMIO_VIRT_BASE)).grow(0x2000_0000), &PagingModeSv39::LEVELS[..=0])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x40000), PageAttribute64::from_page_attr(attr));
    }

    map_physical_memory(&page_table, attr);

    /*// Map 0x0000_0000_0000_0000-0x0000_001F_FFFF_FFFF 128GB
    // to 0xffff_ffd6_0000_0000 to 0xffff_fff5_ffff_ffff, use 1 GB page
    for (idx, pte) in page_table
        .iter_kernel_levels(VRange::from(VAddr::from(PHYS_MAP_VIRT)).grow(0x20_0000_0000), &PagingModeSv39::LEVELS[..=0])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x40000), PageAttribute64::from_page_attr(attr));
    }*/

    // Map 2 MB kernel image
    for (idx, pte) in page_table
        .iter_kernel(VRange::from(VAddr::from(KIMAGE_VIRT_BASE)).grow(0x20_0000))
        .enumerate()
    {
        pte.set(PFN::from(idx + 0x80200), PageAttribute64::from_page_attr(attr));
    }

    unsafe {
        satp::set(
            satp::Mode::Sv39,
            0,
            usize::from(PFN::from(page_table.addr())),
        );
    }
    sfence_vma_all();

    print("paging enabled\n");
    print("stack message:\n");
    print_number(BOOT_STACK.as_ptr() as usize - KIMAGE_OFFSET);
    print("\n");
    print_number(BOOT_STACK.as_ptr() as usize + BOOT_STACK.len() - KIMAGE_OFFSET);
    print("\n");

    let mut buffer = [0u8; 4096];
    loop {
        let Some(count) = read(&mut buffer) else {
            continue;
        };

        print("Input string: ");
        print(str::from_utf8(&buffer[..count]).unwrap());
        print("\n");
    }
}
