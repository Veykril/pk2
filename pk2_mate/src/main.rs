use std::fs::FileTimes;
use std::io::{stdout, Write};

use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use pk2_sync::sync::{DirEntry, Directory, Pk2};

#[derive(Parser, Debug)]
#[command(version, author, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Extracts a pk2 archive into a directory.
    Extract {
        /// Sets the archive to open.
        #[arg(short, long)]
        archive: Utf8PathBuf,
        /// Sets the blowfish key.
        #[arg(short, long, default_value = "169841")]
        key: String,
        /// Sets the output path to extract to.
        #[arg(short, long)]
        out: Utf8PathBuf,
        /// If passed, writes file times to the extracted files.
        #[arg(short, long)]
        write_time: bool,
    },
    /// Repackages a pk2 archive into a new archive, removing fragmentation.
    Repack {
        /// Sets the archive to open.
        #[arg(short, long)]
        archive: Utf8PathBuf,
        /// Sets the blowfish key.
        #[arg(short, long, default_value = "169841")]
        key: String,
        /// Sets the blowfish key for the output archive.
        #[arg(long, default_value = "169841")]
        output_key: String,
        /// The path of the created archive.
        #[arg(short, long)]
        out: Option<Utf8PathBuf>,
    },
    /// Packs a directory into a pk2 archive.
    Pack {
        /// Sets the directory to pack.
        #[arg(short, long)]
        directory: Utf8PathBuf,
        /// Sets the blowfish key for the resulting archive.
        #[arg(short, long, alias = "output_key", default_value = "169841")]
        key: String,
        /// Sets the output path to pack into.
        #[arg(short, long, alias = "out")]
        archive: Option<Utf8PathBuf>,
    },
    /// Lists the contents of a pk2 archive.
    List {
        /// Sets the archive to open.
        #[arg(short, long)]
        archive: Utf8PathBuf,
        /// Sets the blowfish key.
        #[arg(short, long, default_value = "169841")]
        key: String,
        /// If passed, shows file times.
        #[arg(short, long)]
        write_time: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        return;
    };
    match command {
        Commands::Extract { archive, key, out, write_time } => {
            extract(archive, key, out, write_time);
        }
        Commands::Repack { archive, key, output_key, out } => {
            repack(archive, key, output_key, out);
        }
        Commands::Pack { directory, key, archive } => {
            pack(directory, key, archive);
        }
        Commands::List { archive, key, write_time } => {
            list(archive, key, write_time);
        }
    }
}

fn extract(archive_path: Utf8PathBuf, key: String, out: Utf8PathBuf, write_time: bool) {
    let archive = Pk2::open(&archive_path, key)
        .unwrap_or_else(|e| panic!("failed to open archive at {:?}: {e}", archive_path));
    let folder = archive.open_directory("/").unwrap();
    println!("Extracting {:?} to {:?}.", archive_path, out);
    extract_files(folder, &out, write_time);
}

fn extract_files(folder: Directory<'_>, out_path: &Utf8Path, write_times: bool) {
    use std::io::Read;
    let _ = std::fs::create_dir(out_path);
    let mut buf = Vec::new();
    for entry in folder.entries() {
        match entry {
            DirEntry::File(mut file) => {
                file.read_to_end(&mut buf).unwrap();
                let file_path = out_path.join(file.name());
                let os_file = std::fs::File::create(&file_path);
                let res = os_file.and_then(|mut os_file| {
                    os_file.write_all(&buf)?;
                    if write_times {
                        let mut times = FileTimes::new();
                        if let Some(time) = file.modify_time() {
                            times = times.set_modified(time);
                        }
                        if let Some(time) = file.access_time() {
                            times = times.set_accessed(time);
                        }
                        if let Err(e) = os_file.set_times(times) {
                            eprintln!("Failed writing file times at {file_path:?}: {e}");
                        }
                    }
                    Ok(())
                });
                if let Err(e) = res {
                    eprintln!("Failed writing file at {file_path:?}: {e}");
                }
                buf.clear();
            }
            DirEntry::Directory(dir) => {
                if dir.is_backlink() {
                    continue;
                }
                let dir_name = dir.name();
                let path = out_path.join(dir_name);
                extract_files(dir, &path, write_times);
            }
        }
    }
}

fn repack(archive_path: Utf8PathBuf, key: String, output_key: String, out: Option<Utf8PathBuf>) {
    let out_archive_path = out.unwrap_or_else(|| archive_path.with_extension("repack.pk2"));
    let in_archive = Pk2::open(&archive_path, key)
        .unwrap_or_else(|e| panic!("failed to open archive at {:?}: {e}", archive_path));
    let mut out_archive = Pk2::create_new(&out_archive_path, output_key)
        .unwrap_or_else(|e| panic!("failed to create archive at {:?}: {e}", out_archive_path));
    let folder = in_archive.open_directory("/").unwrap();
    println!("Repacking {:?} into {:?}.", archive_path, out_archive_path);
    repack_files(&mut out_archive, folder, "/".as_ref());
}

fn repack_files(out_archive: &mut Pk2, folder: Directory<'_>, path: &Utf8Path) {
    use std::io::{Read, Write};
    let mut buf = Vec::new();
    for entry in folder.entries() {
        match entry {
            DirEntry::File(mut file) => {
                file.read_to_end(&mut buf).unwrap();
                let mut out_file = out_archive.create_file(path.join(file.name())).unwrap();
                out_file.copy_file_times(&file);
                out_file.write_all(&buf).unwrap();
                buf.clear();
            }
            DirEntry::Directory(dir) => {
                if dir.is_backlink() {
                    continue;
                }
                let dir_name = dir.name();
                let path = path.join(dir_name);
                repack_files(out_archive, dir, &path);
            }
        }
    }
}

fn pack(directory: Utf8PathBuf, key: String, archive: Option<Utf8PathBuf>) {
    let out_archive_path = archive.unwrap_or_else(|| directory.with_extension("pk2"));
    if !directory.is_dir() {
        return;
    }
    let mut out_archive = Pk2::create_new(&out_archive_path, key)
        .unwrap_or_else(|e| panic!("failed to create archive at {:?}: {e}", out_archive_path));
    println!("Packing {:?} into {:?}.", directory, out_archive_path);
    pack_files(&mut out_archive, &directory, &directory);
}

fn pack_files(out_archive: &mut Pk2, dir_path: &Utf8Path, base: &Utf8Path) {
    use std::io::{Read, Write};
    let mut buf = Vec::new();
    for entry in std::fs::read_dir(dir_path).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
        if ty.is_dir() {
            pack_files(out_archive, &path, base);
        } else if ty.is_file() {
            let mut file = std::fs::File::open(&path).unwrap();
            file.read_to_end(&mut buf).unwrap();
            out_archive
                .create_file(Utf8Path::new("/").join(path.strip_prefix(base).unwrap()))
                .unwrap()
                .write_all(&buf)
                .unwrap();
            buf.clear();
        }
    }
}

fn list(archive: Utf8PathBuf, key: String, _write_time: bool) {
    let archive = Pk2::open(&archive, key)
        .unwrap_or_else(|e| panic!("failed to open archive at {:?}: {e}", archive));
    let folder = archive.open_directory("/").unwrap();
    list_files(&mut stdout(), folder, "/".as_ref(), 0);
}

fn list_files(out: &mut impl Write, folder: Directory, path: &Utf8Path, mut ident_level: usize) {
    writeln!(out, "{}{path}", " ".repeat(ident_level)).unwrap();
    ident_level += path.as_os_str().to_str().unwrap_or_default().chars().count();
    for entry in folder.entries() {
        match entry {
            DirEntry::File(file) => {
                writeln!(out, "{}{}", " ".repeat(ident_level), file.name()).unwrap();
            }
            DirEntry::Directory(dir) => {
                if dir.is_backlink() {
                    continue;
                }
                let dir_name = dir.name();
                let path = path.join(dir_name);
                list_files(&mut *out, dir, &path, ident_level);
            }
        }
    }
}
