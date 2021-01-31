use std::io;

use io::BufRead;

fn main() {
    let mut symbols: Vec<_> = io::stdin()
        .lock()
        .lines()
        .filter_map(|l| parse_nm_line(&l.unwrap()))
        .collect();

    symbols.sort_unstable_by_key(|sym| sym.0);

    print_prologue("ksyms_offsets");
    for &(addr, _) in &symbols {
        print_hex(addr);
    }

    print_prologue("ksyms_num_syms");
    print_dec(symbols.len());

    print_prologue("ksyms_markers");
    print_dec(0);
    let mut acc = 0;
    for (_, name) in &symbols {
        acc += name.len() + 1;
        print_dec(acc);
    }

    print_prologue("ksyms_names");
    for (_, name) in &symbols {
        print_ascii(name);
    }
}

fn parse_nm_line(s: &str) -> Option<(usize, String)> {
    let mut fields = s.split_whitespace();

    let addr = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
    let kind = fields.next().unwrap();

    if kind == "t" || kind == "T" {
        Some((
            addr,
            format!("{:#}", rustc_demangle::demangle(fields.next().unwrap())),
        ))
    } else {
        None
    }
}

fn print_prologue(label: &str) {
    println!(".section .rodata, \"a\"");
    println!(".global {0}\n.balign 8\n{0}:", label);
}

fn print_dec(n: usize) {
    println!("    .quad {}", n);
}

fn print_hex(n: usize) {
    println!("    .quad 0x{:x}", n);
}

fn print_ascii(s: &str) {
    println!("    .asciz \"{}\"", s);
}
