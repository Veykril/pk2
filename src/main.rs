#[allow(warnings, clippy::all)]
fn main() {
    use std::io::Read;
    use std::io::Write;
    let mut archive = gfxfilemanager::archive::Archive::open("Particles.pk2", b"169841").unwrap();
    print(archive.open_dir("/.").unwrap());
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

fn print(root: gfxfilemanager::fs::Directory) {
    for dir in root.directories() {
        if dir.name() == "." || dir.name() == ".." {
            continue
        }
        print(dir)
    }

    for file in root.files() {
        println!("{:?}", file.name());
    }
}