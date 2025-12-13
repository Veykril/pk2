//! Tests for Directory operations, enumeration, and navigation.

use std::io::{Cursor, Write};

use pk2_sync::sync::{DirEntry, Pk2};

/// Helper to create an archive with a predefined structure.
fn create_test_archive() -> Pk2<Cursor<Vec<u8>>> {
    let mut archive = Pk2::create_new_in_memory("testkey").unwrap();

    // Create files in root
    {
        let mut f = archive.create_file("/root1.txt").unwrap();
        f.write_all(b"root file 1").unwrap();
    }
    {
        let mut f = archive.create_file("/root2.txt").unwrap();
        f.write_all(b"root file 2").unwrap();
    }

    // Create files in subdirectory
    {
        let mut f = archive.create_file("/subdir/file1.txt").unwrap();
        f.write_all(b"subdir file 1").unwrap();
    }
    {
        let mut f = archive.create_file("/subdir/file2.txt").unwrap();
        f.write_all(b"subdir file 2").unwrap();
    }

    // Create nested structure
    {
        let mut f = archive.create_file("/a/b/c/deep.txt").unwrap();
        f.write_all(b"deep file").unwrap();
    }

    archive
}

#[test]
fn open_root_directory() {
    let archive = Pk2::<Cursor<Vec<u8>>>::create_new_in_memory("test").unwrap();
    let root = archive.open_root_dir();

    // Root directory should have "." entry
    assert_eq!(root.name(), ".");
}

#[test]
fn open_directory_via_path() {
    let archive = create_test_archive();

    let dir = archive.open_directory("/subdir").unwrap();
    assert_eq!(dir.name(), "subdir");
}

#[test]
fn open_nested_directory() {
    let archive = create_test_archive();

    let dir = archive.open_directory("/a/b/c").unwrap();
    assert_eq!(dir.name(), "c");
}

#[test]
fn open_root_via_empty_path() {
    let archive = create_test_archive();

    let dir = archive.open_directory("/").unwrap();
    assert_eq!(dir.name(), ".");
}

#[test]
fn open_nonexistent_directory_fails() {
    let archive = create_test_archive();

    let result = archive.open_directory("/nonexistent");
    assert!(result.is_err());
}

#[test]
fn open_file_as_directory_fails() {
    let archive = create_test_archive();

    let result = archive.open_directory("/root1.txt");
    assert!(result.is_err(), "Opening a file as directory should fail");
}

#[test]
fn directory_entries_iteration() {
    let archive = create_test_archive();
    let root = archive.open_root_dir();

    let entries: Vec<_> = root.entries().collect();

    // Should have: ".", "root1.txt", "root2.txt", "subdir", "a"
    assert!(entries.len() >= 4, "Root should have at least 4 entries (., files, dirs)");
}

#[test]
fn directory_entries_excludes_dotdot() {
    let archive = create_test_archive();
    let root = archive.open_root_dir();

    for entry in root.entries() {
        if let DirEntry::Directory(dir) = entry {
            assert_ne!(dir.name(), "..", "entries() should exclude '..' entries");
        }
    }
}

#[test]
fn directory_files_iterator() {
    let archive = create_test_archive();
    let root = archive.open_root_dir();

    let files: Vec<_> = root.files().collect();

    // Should have root1.txt and root2.txt
    assert_eq!(files.len(), 2);

    let names: Vec<_> = files.iter().map(|f| f.name()).collect();
    assert!(names.contains(&"root1.txt"));
    assert!(names.contains(&"root2.txt"));
}

#[test]
fn subdirectory_entries() {
    let archive = create_test_archive();
    let subdir = archive.open_directory("/subdir").unwrap();

    let entries: Vec<_> = subdir.entries().collect();

    // Should have: ".", "..", "file1.txt", "file2.txt"
    // But entries() excludes "." and ".."
    let file_count = entries.iter().filter(|e| matches!(e, DirEntry::File(_))).count();

    assert_eq!(file_count, 2);
}

#[test]
fn open_file_from_directory() {
    let archive = create_test_archive();
    let subdir = archive.open_directory("/subdir").unwrap();

    let file = subdir.open_file("file1.txt").unwrap();
    assert_eq!(file.name(), "file1.txt");
    assert_eq!(file.size(), 13); // "subdir file 1"
}

#[test]
fn open_subdirectory_from_directory() {
    let archive = create_test_archive();
    let dir_a = archive.open_directory("/a").unwrap();

    let dir_b = dir_a.open_directory("b").unwrap();
    assert_eq!(dir_b.name(), "b");
}

#[test]
fn open_nested_path_from_directory() {
    let archive = create_test_archive();
    let dir_a = archive.open_directory("/a").unwrap();

    let dir_c = dir_a.open_directory("b/c").unwrap();
    assert_eq!(dir_c.name(), "c");
}

#[test]
fn directory_open_returns_dir_entry() {
    let archive = create_test_archive();
    let root = archive.open_root_dir();

    // Open a file
    let file_entry = root.open("root1.txt").unwrap();
    assert!(matches!(file_entry, DirEntry::File(_)));

    // Open a directory
    let dir_entry = root.open("subdir").unwrap();
    assert!(matches!(dir_entry, DirEntry::Directory(_)));
}

#[test]
fn for_each_file_recursive() {
    let archive = create_test_archive();

    let mut found_files = Vec::new();
    archive
        .for_each_file("/", |path, file| {
            found_files.push((path.to_path_buf(), file.name().to_string()));
            Ok(())
        })
        .unwrap();

    // Should find all files in the archive
    assert!(found_files.len() >= 5, "Should find at least 5 files");

    // Verify some expected files
    let names: Vec<_> = found_files.iter().map(|(_, name)| name.as_str()).collect();
    assert!(names.contains(&"root1.txt"));
    assert!(names.contains(&"root2.txt"));
    assert!(names.contains(&"deep.txt"));
}

#[test]
fn for_each_file_from_subdirectory() {
    let archive = create_test_archive();

    let mut found_files = Vec::new();
    archive
        .for_each_file("/subdir", |path, file| {
            found_files.push((path.to_path_buf(), file.name().to_string()));
            Ok(())
        })
        .unwrap();

    // Should only find files in /subdir
    assert_eq!(found_files.len(), 2);

    let names: Vec<_> = found_files.iter().map(|(_, name)| name.as_str()).collect();
    assert!(names.contains(&"file1.txt"));
    assert!(names.contains(&"file2.txt"));
}

#[test]
fn directory_for_each_file() {
    let archive = create_test_archive();
    let dir_a = archive.open_directory("/a").unwrap();

    let mut found_files = Vec::new();
    dir_a
        .for_each_file(|path, file| {
            found_files.push((path.to_path_buf(), file.name().to_string()));
            Ok(())
        })
        .unwrap();

    // Should find deep.txt inside a/b/c
    assert_eq!(found_files.len(), 1);
    assert_eq!(found_files[0].1, "deep.txt");
}

#[test]
fn empty_directory_entries() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    // Create a directory by creating a file in it, then delete the file
    {
        let mut f = archive.create_file("/emptydir/temp.txt").unwrap();
        f.write_all(b"temp").unwrap();
    }
    archive.delete_file("/emptydir/temp.txt").unwrap();

    let dir = archive.open_directory("/emptydir").unwrap();
    let entries: Vec<_> = dir.entries().collect();

    // Should have "." and ".." but entries() filters those
    // After deleting the file, directory might have only . and ..
    let file_count = entries.iter().filter(|e| matches!(e, DirEntry::File(_))).count();
    assert_eq!(file_count, 0);
}

#[test]
fn directory_entries_match_files() {
    let archive = create_test_archive();
    let subdir = archive.open_directory("/subdir").unwrap();

    let entries: Vec<_> = subdir.entries().collect();
    let files: Vec<_> = subdir.files().collect();

    let entry_files: Vec<_> = entries
        .iter()
        .filter_map(|e| match e {
            DirEntry::File(f) => Some(f.name()),
            _ => None,
        })
        .collect();

    let file_names: Vec<_> = files.iter().map(|f| f.name()).collect();

    // Both should return the same files
    assert_eq!(entry_files.len(), file_names.len());
    for name in &entry_files {
        assert!(file_names.contains(name));
    }
}

#[test]
fn many_files_in_directory() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();

    // Create many files
    for i in 0..50 {
        let path = format!("/many/file{:02}.txt", i);
        let mut f = archive.create_file(&path).unwrap();
        f.write_all(format!("Content {}", i).as_bytes()).unwrap();
    }

    let dir = archive.open_directory("/many").unwrap();
    let files: Vec<_> = dir.files().collect();

    assert_eq!(files.len(), 50, "Should have 50 files in directory");
}

#[test]
fn directory_open_nonexistent_fails() {
    let archive = create_test_archive();
    let root = archive.open_root_dir();

    let result = root.open("nonexistent");
    assert!(result.is_err());
}

#[test]
fn case_insensitive_directory_lookup() {
    let archive = create_test_archive();

    // PK2 archives typically use case-insensitive lookups
    let result1 = archive.open_directory("/subdir");
    let result2 = archive.open_directory("/SUBDIR");
    let result3 = archive.open_directory("/SubDir");

    // All should succeed (case-insensitive)
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());
}

#[test]
fn case_insensitive_file_lookup() {
    let archive = create_test_archive();

    let result1 = archive.open_file("/root1.txt");
    let result2 = archive.open_file("/ROOT1.TXT");
    let result3 = archive.open_file("/Root1.Txt");

    // All should succeed (case-insensitive)
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());
}
