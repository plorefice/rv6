use std::io::Cursor;

static EXT2_IMG: &str = "tests/data/ext2.img";

#[test]
fn read_ext2_fs() {
    let data = std::fs::read(EXT2_IMG).unwrap();
    let _fs = ext2::FileSystem::new(Cursor::new(data));
}
