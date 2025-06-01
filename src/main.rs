#![no_std]
#![no_main]

mod rv64_mm;

use buddy_allocator::{BuddyAllocator, BuddyRawPage};
use core::{ptr::NonNull, sync::atomic::AtomicUsize};
use eonix_mm::{
    address::{Addr as _, PAddr, VAddr, VRange},
    page_table::{PageAttribute, PagingMode, RawAttribute, PTE as _},
    paging::{Page, PageAccess, PageAlloc, PageBlock, RawPage as RawPageTrait, PFN},
};
use intrusive_list::{container_of, Link};
use riscv::register::satp;
use riscv_rt::entry;
use rv64_mm::*;
use spin::Mutex;

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

type PageTable<'a> = eonix_mm::page_table::PageTable<'a, PagingModeSv48, BuddyPageAlloc, DirectPageAccess>;

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

#[entry]
fn main() -> ! {
    print("Hello World!\n");

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

    // Map 0x80200000-0x81200000 16MB identically, use 2MB page
    for (idx, pte) in page_table
        .iter_kernel_levels(VRange::from(VAddr::from(KIMAGE_PHYS_BASE)).grow(0x1000000), &PagingModeSv48::LEVELS[..=2])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x200 + 0x80200), PageAttribute64::from_page_attr(attr));
    }

    // Map 0x0000_0000_0000_0000-0x0000_007F_FFFF_FFFF 512GB
    // to 0xFFFF_FF00_0000_0000 to 0xFFFF_FF7F_FFFF_FFFF, use 1 GB page
    for (idx, pte) in page_table
        .iter_kernel_levels(VRange::from(VAddr::from(PHYS_MAP_VIRT)).grow(0x80_0000_0000), &PagingModeSv48::LEVELS[..=1])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x40000), PageAttribute64::from_page_attr(attr));
    }

    // Map 2 MB kernel image
    for (idx, pte) in page_table
        .iter_kernel(VRange::from(VAddr::from(KIMAGE_VIRT_BASE)).grow(0x20_0000))
        .enumerate()
    {
        pte.set(PFN::from(idx + 0x80200), PageAttribute64::from_page_attr(attr));
    }

    unsafe {
        satp::set(
            satp::Mode::Sv48,
            0,
            usize::from(PFN::from(page_table.addr())),
        );
    }

    print("paging enabled\n");

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
