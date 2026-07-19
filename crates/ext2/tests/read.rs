use std::io::Cursor;

use ext2::Read;

static EXT2_IMG: &str = "tests/data/ext2.img";

#[test]
fn read_ext2_fs() {
    let data = std::fs::read(EXT2_IMG).unwrap();
    let mut fs = ext2::FileSystem::mount(Cursor::new(data)).unwrap();

    let mut dir = fs.open_dir("/").unwrap();
    let names: Vec<_> = dir
        .iter()
        .map(|e| e.unwrap().file_name().unwrap().to_string())
        .collect();

    assert!(names.contains(&".".to_string()));
    assert!(names.contains(&"..".to_string()));
    assert!(names.contains(&"lost+found".to_string()));
    assert!(names.contains(&"etc".to_string()));
    assert!(names.contains(&"bin".to_string()));
    assert!(names.contains(&"sbin".to_string()));
    assert!(names.contains(&"init".to_string()));
}

#[test]
fn root_names() {
    let data = std::fs::read(EXT2_IMG).unwrap();
    let mut fs = ext2::FileSystem::mount(Cursor::new(data)).unwrap();

    let _ = fs.open_dir("/").unwrap();
    let _ = fs.open_dir(".").unwrap();
    let _ = fs.open_dir("").unwrap();
}

#[test]
fn read_file() {
    let data = std::fs::read(EXT2_IMG).unwrap();
    let mut fs = ext2::FileSystem::mount(Cursor::new(data)).unwrap();

    let mut file = fs.open("/init").unwrap();
    let mut buf = vec![0; 4096];

    let n = file.read(&mut buf).unwrap();
    assert_eq!(n, 15);

    let content = std::str::from_utf8(&buf[..n]).unwrap();
    assert_eq!(content, "#!/bin/sh\ntrue\n");
}
