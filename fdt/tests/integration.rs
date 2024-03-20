use fdt::{Fdt, PropEncodedArray, StringList};

#[test]
fn load_dtb() {
    let fdt = Fdt::from_bytes(include_bytes!("data/qemu-riscv.dtb")).unwrap();

    let root = fdt.root_node().unwrap();
    assert_eq!(root.name(), "");

    let props = root.properties().collect::<Vec<_>>();
    assert_eq!(props.len(), 4);

    assert_eq!(props[0].name(), Some("#address-cells"));
    assert_eq!(props[0].value::<u32>(), Some(2));

    assert_eq!(props[1].name(), Some("#size-cells"));
    assert_eq!(props[1].value::<u32>(), Some(2));

    assert_eq!(props[2].name(), Some("compatible"));
    assert_eq!(props[2].value(), Some("riscv-virtio"));

    assert_eq!(props[3].name(), Some("model"));
    assert_eq!(props[3].value(), Some("riscv-virtio,qemu"));

    let children = root.children().collect::<Vec<_>>();
    assert_eq!(children.len(), 10);
}

#[test]
fn reserved_memory_map() {
    let fdt = Fdt::from_bytes(include_bytes!("data/qemu-riscv.dtb")).unwrap();
    assert_eq!(fdt.reserved_memory_map().count(), 0);
}

#[test]
fn find_by_path() {
    let fdt = Fdt::from_bytes(include_bytes!("data/qemu-riscv.dtb")).unwrap();

    // Root node
    assert!(fdt.find_by_path("").unwrap().is_some());
    assert!(fdt.find_by_path("/").unwrap().is_some());

    // Unknown nodes
    assert!(fdt.find_by_path("invalid").unwrap().is_none());
    assert!(fdt.find_by_path("/invalid").unwrap().is_none());

    // Valid nodes
    assert!(fdt.find_by_path("/poweroff").unwrap().is_some());
    assert!(fdt.find_by_path("/memory@80000000").unwrap().is_some());
    assert!(fdt
        .find_by_path("/cpus/cpu@0/interrupt-controller")
        .unwrap()
        .is_some());
}

#[test]
fn node_parent() {
    let fdt = Fdt::from_bytes(include_bytes!("data/qemu-riscv.dtb")).unwrap();
    assert!(fdt.root_node().unwrap().parent().is_none());

    let n = fdt.find_by_path("/poweroff").unwrap().unwrap();
    assert_eq!(n.parent().unwrap().name(), ""); // parent is root

    let n = fdt
        .find_by_path("/cpus/cpu@0/interrupt-controller")
        .unwrap()
        .unwrap();
    assert_eq!(n.parent().unwrap().identifier(), "cpu@0");
    assert_eq!(
        n.parent().and_then(|p| p.parent()).unwrap().identifier(),
        "cpus"
    );
}

#[test]
fn memory_nodes() {
    let fdt = Fdt::from_bytes(include_bytes!("data/qemu-riscv.dtb")).unwrap();

    let memory_nodes = fdt
        .root_node()
        .unwrap()
        .children()
        .filter(|n| n.name() == "memory")
        .collect::<Vec<_>>();

    assert_eq!(memory_nodes.len(), 1);

    let mut regs: PropEncodedArray<(u64, u64)> = memory_nodes[0].property("reg").unwrap();

    assert_eq!(regs.next(), Some((0x80000000, 0x8000000)));
    assert_eq!(regs.next(), None);
}

#[test]
fn stringlist_prop() {
    let fdt = Fdt::from_bytes(include_bytes!("data/qemu-riscv.dtb")).unwrap();

    let mut test_compatible_prop: StringList = fdt
        .find_by_path("/soc/test@100000")
        .unwrap()
        .and_then(|n| n.property("compatible"))
        .unwrap();

    assert_eq!(test_compatible_prop.next(), Some("sifive,test1"));
    assert_eq!(test_compatible_prop.next(), Some("sifive,test0"));
    assert_eq!(test_compatible_prop.next(), Some("syscon"));
    assert_eq!(test_compatible_prop.next(), None);
}
