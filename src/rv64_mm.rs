use core::{marker::PhantomData, ptr::NonNull};

use eonix_mm::{
    page_table::{
        PageAttribute, PageTableLevel, PagingMode, RawAttribute, RawPageTable, TableAttribute, PTE,
    },
    paging::{PageBlock, PFN},
};

pub struct RV39;
pub struct RV39PTE(u64);
pub struct RV39RawTable<'a>(NonNull<RV39PTE>, PhantomData<&'a ()>);

#[derive(Clone, Copy)]
pub struct Attribute(u64);

impl Attribute {
    const PA_V: u64 = 1;
    const PA_R: u64 = 2;
    const PA_W: u64 = 4;
    const PA_X: u64 = 8;
    const PA_U: u64 = 16;
    const PA_G: u64 = 32;
    const PA_A: u64 = 64;
    const PA_D: u64 = 128;
    const PA_COW: u64 = 256;
    const PA_MMAP: u64 = 512;
}

impl RawAttribute for Attribute {
    fn null() -> Self {
        Self(0)
    }

    fn as_table_attr(self) -> Option<TableAttribute> {
        let mut attr = TableAttribute::empty();

        if self.0 & (Self::PA_R | Self::PA_W | Self::PA_X) != 0 {
            panic!("Invalid page attribute combination");
        }

        if self.0 & Self::PA_V != 0 {
            attr |= TableAttribute::PRESENT;
        }

        if self.0 & Self::PA_U != 0 {
            attr |= TableAttribute::USER;
        }

        if self.0 & Self::PA_G != 0 {
            attr |= TableAttribute::GLOBAL;
        }

        if self.0 & Self::PA_A != 0 {
            attr |= TableAttribute::ACCESSED;
        }

        Some(attr)
    }

    fn as_page_attr(self) -> Option<PageAttribute> {
        let mut attr = PageAttribute::empty();

        if self.0 & (Self::PA_R | Self::PA_W | Self::PA_X) == 0 {
            panic!("Invalid page attribute combination");
        }

        if self.0 & Self::PA_V != 0 {
            attr |= PageAttribute::PRESENT;
        }

        if self.0 & Self::PA_R != 0 {
            attr |= PageAttribute::READ;
        }

        if self.0 & Self::PA_W != 0 {
            attr |= PageAttribute::WRITE;
        }

        if self.0 & Self::PA_X != 0 {
            attr |= PageAttribute::EXECUTE;
        }

        if self.0 & Self::PA_U != 0 {
            attr |= PageAttribute::USER;
        }

        if self.0 & Self::PA_G != 0 {
            attr |= PageAttribute::GLOBAL;
        }

        if self.0 & Self::PA_A != 0 {
            attr |= PageAttribute::ACCESSED;
        }

        if self.0 & Self::PA_D != 0 {
            attr |= PageAttribute::DIRTY;
        }

        if self.0 & Self::PA_COW != 0 {
            attr |= PageAttribute::COPY_ON_WRITE;
        }

        if self.0 & Self::PA_MMAP != 0 {
            attr |= PageAttribute::MAPPED;
        }

        Some(attr)
    }

    fn from_table_attr(table_attr: eonix_mm::page_table::TableAttribute) -> Self {
        let mut raw_attr = 0;
        for attr in table_attr.iter() {
            match attr {
                TableAttribute::PRESENT => raw_attr |= Self::PA_V,
                TableAttribute::USER => raw_attr |= Self::PA_U,
                TableAttribute::GLOBAL => raw_attr |= Self::PA_G,
                TableAttribute::ACCESSED => raw_attr |= Self::PA_A,
                _ => unreachable!("Unsupported table attribute"),
            }
        }

        Self(raw_attr)
    }

    fn from_page_attr(page_attr: PageAttribute) -> Self {
        let mut raw_attr = 0;

        if page_attr
            .intersection(PageAttribute::READ | PageAttribute::WRITE | PageAttribute::EXECUTE)
            .is_empty()
        {
            panic!("Invalid page attribute combination");
        }

        for attr in page_attr.iter() {
            match attr {
                PageAttribute::PRESENT => raw_attr |= Self::PA_V,
                PageAttribute::READ => raw_attr |= Self::PA_R,
                PageAttribute::WRITE => raw_attr |= Self::PA_W,
                PageAttribute::EXECUTE => raw_attr |= Self::PA_X,
                PageAttribute::USER => raw_attr |= Self::PA_U,
                PageAttribute::GLOBAL => raw_attr |= Self::PA_G,
                PageAttribute::ACCESSED => raw_attr |= Self::PA_A,
                PageAttribute::DIRTY => raw_attr |= Self::PA_D,
                PageAttribute::COPY_ON_WRITE => raw_attr |= Self::PA_COW,
                PageAttribute::MAPPED => raw_attr |= Self::PA_MMAP,
                _ => unreachable!("Unsupported page attribute"),
            }
        }

        Self(raw_attr)
    }
}

impl PTE for RV39PTE {
    type Attr = Attribute;

    fn set(&mut self, pfn: PFN, attr: Self::Attr) {
        self.0 = (usize::from(pfn) << 10) as u64 | attr.0;
    }

    fn get(&self) -> (PFN, Self::Attr) {
        let pfn = PFN::from(self.0 as usize >> 10);
        let attr = Attribute(self.0 & 0x3FF);
        (pfn, attr)
    }
}

impl<'a> RawPageTable<'a> for RV39RawTable<'a> {
    type Entry = RV39PTE;

    fn index(&self, index: u16) -> &'a Self::Entry {
        unsafe { self.0.add(index as usize).as_ref() }
    }

    fn index_mut(&mut self, index: u16) -> &'a mut Self::Entry {
        unsafe { self.0.add(index as usize).as_mut() }
    }

    unsafe fn from_ptr(ptr: NonNull<PageBlock>) -> Self {
        Self(ptr.cast(), PhantomData)
    }
}

impl PagingMode for RV39 {
    type Entry = RV39PTE;

    type RawTable<'a> = RV39RawTable<'a>;

    const LEVELS: &'static [PageTableLevel] = &[
        PageTableLevel::new(30, 9),
        PageTableLevel::new(21, 9),
        PageTableLevel::new(12, 9),
    ];

    const KERNEL_ROOT_TABLE_PFN: PFN = PFN::from_val(0x80000000);
}
