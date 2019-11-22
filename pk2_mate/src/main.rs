use clap::{crate_authors, crate_description, crate_name, crate_version};
use clap::{App, Arg, ArgMatches, SubCommand};

use std::path::{Path, PathBuf};

fn main() {
    let app = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .subcommand(extract_app())
        .subcommand(repack_app());
    let matches = app.get_matches();
    match matches.subcommand() {
        ("extract", Some(matches)) => extract(matches),
        ("repack", Some(matches)) => repack(matches),
        _ => println!("{}", matches.usage()),
    }
}

fn extract_app() -> App<'static, 'static> {
    SubCommand::with_name("extract")
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("archive")
                .short("a")
                .long("archive")
                .required(true)
                .takes_value(true)
                .help("Sets the archive to open"),
        )
        .arg(
            Arg::with_name("key")
                .short("k")
                .long("key")
                .takes_value(true)
                .default_value("169841")
                .help("Sets the blowfish key"),
        )
        .arg(
            Arg::with_name("out")
                .short("o")
                .long("out")
                .takes_value(true)
                .help("Sets the output path to extract to"),
        )
}

fn extract(matches: &ArgMatches<'static>) {
    let key = matches.value_of("key").unwrap().as_bytes();
    let archive_path = matches.value_of_os("archive").map(Path::new).unwrap();
    let out_path = matches
        .value_of_os("out")
        .map(PathBuf::from)
        .unwrap_or_else(|| archive_path.with_extension(""));
    let archive = pk2::Pk2::open(archive_path, key)
        .expect(&format!("failed to open archive at {:?}", archive_path));
    let folder = archive.open_directory("/").unwrap();
    println!("Extracting {:?} to {:?}.", archive_path, out_path);
    extract_files(folder, &out_path);
}

fn extract_files(folder: pk2::fs::Directory<'_>, out_path: &Path) {
    use std::io::Read;
    let _ = std::fs::create_dir(out_path);
    let mut buf = Vec::new();
    for entry in folder.entries() {
        match entry {
            pk2::fs::DirEntry::File(mut file) => {
                file.read_to_end(&mut buf).unwrap();
                let file_path = out_path.join(file.name());
                if let Err(e) = std::fs::write(&file_path, &buf) {
                    eprintln!("Failed writing file at {:?}: {}", file_path, e);
                }
                buf.clear();
            }
            pk2::fs::DirEntry::Directory(dir) => {
                let dir_name = dir.name();
                let path = out_path.join(dir_name);
                extract_files(dir, &path);
            }
        }
    }
}

fn repack_app() -> App<'static, 'static> {
    SubCommand::with_name("repack")
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("archive")
                .short("a")
                .long("archive")
                .required(true)
                .takes_value(true)
                .help("Sets the archive to open"),
        )
        .arg(
            Arg::with_name("key")
                .short("k")
                .long("key")
                .takes_value(true)
                .default_value("169841")
                .help("Sets the blowfish key for the input archive"),
        )
        .arg(
            Arg::with_name("packkey")
                .short("p")
                .long("packkey")
                .takes_value(true)
                .help("Sets the blowfish key for the output archive"),
        )
        .arg(
            Arg::with_name("out")
                .short("o")
                .long("out")
                .takes_value(true)
                .help("Sets the output path to repack to"),
        )
}

fn repack(matches: &ArgMatches<'static>) {
    let key = matches.value_of("key").unwrap().as_bytes();
    let packkey = matches
        .value_of("packkey")
        .or(matches.value_of("key"))
        .unwrap()
        .as_bytes();
    let archive_path = matches.value_of_os("archive").map(Path::new).unwrap();
    let out_archive_path = matches
        .value_of_os("out")
        .map(PathBuf::from)
        .unwrap_or_else(|| archive_path.with_extension("repack.pk2"));
    let in_archive = pk2::Pk2::open(archive_path, key)
        .expect(&format!("failed to open archive at {:?}", archive_path));
    let mut out_archive = pk2::Pk2::create(&out_archive_path, packkey).expect(&format!(
        "failed to create archive at {:?}",
        out_archive_path
    ));
    let folder = in_archive.open_directory("/").unwrap();
    println!("Repacking {:?} into {:?}.", archive_path, out_archive_path);
    repack_files(&mut out_archive, folder, "/".as_ref());
}

fn repack_files(out_archive: &mut pk2::Pk2, folder: pk2::fs::Directory<'_>, path: &Path) {
    use std::io::{Read, Write};
    let mut buf = Vec::new();
    for entry in folder.entries() {
        match entry {
            pk2::fs::DirEntry::File(mut file) => {
                file.read_to_end(&mut buf).unwrap();
                let mut file = out_archive.create_file(path.join(file.name())).unwrap();
                file.write_all(&buf).unwrap();
                buf.clear();
            }
            pk2::fs::DirEntry::Directory(dir) => {
                let path = path.join(dir.name());
                repack_files(out_archive, dir, &path);
            }
        }
    }
}
