#[allow(warnings, clippy::all)]
fn main() {
    use std::io::Read;
    use std::io::Write;

    let archive = gfxfilemanager::archive::Archive::open("Particles.pk2", b"169841").unwrap();
    let mut pk2file = archive.open_file("/animations/bandi.ban").unwrap();
    let pk2dir = archive.open_dir("/").unwrap();
    {
        let file = pk2dir.open_file("animations/bandi.ban").unwrap();
    }
}
/*
let mut buf = Vec::new();
pk2file.read_to_end(&mut buf);
let mut file = std::fs::File::create("bandi.ban").unwrap();
file.write_all(&buf).unwrap();*/

//gfxfilemanager::archive::Archive::create("Particlestestown.pk2", b"169841").unwrap();
/*
match root_block.0[1] {
    PackEntry::Folder { pos_children, .. } => {
        for e in &pack.read_block(pos_children).unwrap().0 {
            println!("{:?}", e.name())
        }
        //println!("{:#?}", pack.read_block(pos_children).unwrap());
    }
    _ => (),
}*/
//let mut pack: Archive = Archive::new(path, b"169841").unwrap();
/*
*/
