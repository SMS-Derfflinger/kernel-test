pub fn get_num_harts(dtb_addr: usize) -> usize {
    let fdt = unsafe {
        fdt::Fdt::from_ptr(dtb_addr as *const u8)
            .expect("Failed to parse device tree from dtb_addr")
    };
    let dtb_cpus = fdt.cpus();
    let mut num_harts = 0;

    for _cpu in dtb_cpus {
        num_harts += 1;
    }
    num_harts
}
