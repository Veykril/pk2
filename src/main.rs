use std::path::Path;

#[allow(warnings, clippy::all)]
fn main() {
    use std::io::Read;
    use std::io::Write;
    let mut archive = pk2::Pk2::create("CreateTest.pk2", "169841")
        .or_else(|_| pk2::Pk2::open("CreateTest.pk2", "169841"))
        .unwrap();
    print(archive.open_dir("/").unwrap(), 0);
    let mut file = archive.create_file("/level1dir/test.file").unwrap();
    file.write_all(&[0, 1, 2, 3, 4][..]).unwrap();
    drop(file);
    for i in 0..=20 {
        //println!("{}", i);
        //archive.create_file(&format!("/test.file{}", i)).unwrap();
    }
    print(archive.open_dir("/").unwrap(), 0);
    /*
    let mut archive = gfxfilemanager::archive::Archive::open("Particles.pk2", b"169841").unwrap();
    let mut pk2file = archive.open_file("/animations/bandi.ban").unwrap();
    let mut buf = Vec::new();
    pk2file.read_to_end(&mut buf);
    let mut pk2filemut = archive.open_file_mut("/animations/bandi.ban").unwrap();
    let mut buf2 = Vec::new();
    pk2filemut.read_to_end(&mut buf2);
    assert_eq!(buf, buf2);*/
}

fn print(root: pk2::fs::Directory, level: usize) {
    println!(
        "{}{:?} {:?}",
        "\t".repeat(level),
        root.name(),
        root.files().count()
    );
    for dir in root.directories() {
        print(dir, level + 1)
    }

    for file in root.files() {
        println!("{}{}", "\t".repeat(level), file.name(),);
    }
}
