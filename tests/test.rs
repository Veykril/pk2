use std::fmt::Write;
use std::io::Cursor;
use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};
use pk2_sync::sync::{DirEntry, Directory, Pk2};

#[test]
fn constructing_archive_works() {
    let directory = Utf8PathBuf::from("src");
    let mut out_archive = Pk2::create_new_in_memory("secret_key").unwrap();
    pack_files(&mut out_archive, &directory, &directory);
    let mut out = String::new();
    list_files(&mut out, out_archive.open_root_dir(), Utf8Path::new("/"), 0);
    assert_eq!(
        out,
        r"/
 blowfish.rs
 error.rs
 filetime.rs
 format/
        block_chain.rs
        chain_index.rs
        entry.rs
        header.rs
 format.rs
 lib.rs
 parse.rs
"
    );
}

fn pack_files(out_archive: &mut Pk2<Cursor<Vec<u8>>>, dir_path: &Utf8Path, base: &Utf8Path) {
    use std::io::{Read, Write};
    let mut buf = Vec::new();
    let mut entries =
        std::fs::read_dir(dir_path).unwrap().map(|entry| entry.unwrap()).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let ty = entry.file_type().unwrap();
        let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
        if ty.is_dir() {
            pack_files(out_archive, &path, base);
        } else if ty.is_file() {
            let mut file = std::fs::File::open(&path).unwrap();
            file.read_to_end(&mut buf).unwrap();
            let mut archive_file = out_archive
                .create_file(Utf8Path::new("/").join(path.strip_prefix(base).unwrap()))
                .unwrap();
            archive_file.set_access_time(SystemTime::UNIX_EPOCH);
            archive_file.set_create_time(SystemTime::UNIX_EPOCH);
            archive_file.set_modify_time(SystemTime::UNIX_EPOCH);
            archive_file.write_all(&buf).unwrap();
            buf.clear();
        }
    }
}

fn list_files(
    out: &mut impl Write,
    folder: Directory<Cursor<Vec<u8>>>,
    path: &Utf8Path,
    mut ident_level: usize,
) {
    let dir_name = path.file_name().unwrap_or("");
    writeln!(out, "{}{dir_name}/", " ".repeat(ident_level)).unwrap();
    ident_level += dir_name.chars().count() + 1;
    for entry in folder.entries() {
        match entry {
            DirEntry::File(file) => {
                writeln!(out, "{}{}", " ".repeat(ident_level), file.name()).unwrap();
            }
            DirEntry::Directory(dir) => {
                let dir_name = dir.name();
                let path = path.join(dir_name);
                if let "." | ".." = dir_name {
                    continue;
                }
                list_files(&mut *out, dir, &path, ident_level);
            }
        }
    }
}
