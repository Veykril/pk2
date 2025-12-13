//! Tests for archive creation operations.

use std::io::Cursor;

use pk2_sync::sync::Pk2;

#[test]
fn create_new_in_memory_with_key() {
    let archive = Pk2::create_new_in_memory("test_key").unwrap();
    // Archive should be created successfully
    let data: Vec<u8> = archive.into();
    // Header is 256 bytes, root block is 2560 bytes
    assert!(data.len() >= 256 + 2560, "Archive should contain header and root block");
}

#[test]
fn create_new_in_cursor() {
    let cursor = Cursor::new(Vec::with_capacity(4096));
    let archive = Pk2::create_new_in(cursor, "secret").unwrap();
    let data: Vec<u8> = archive.into();
    assert!(data.len() >= 256 + 2560);
}

#[test]
fn created_archive_has_valid_header() {
    let archive = Pk2::create_new_in_memory("test").unwrap();
    let data: Vec<u8> = archive.into();

    // Check PK2 signature at start of file
    let signature = b"JoyMax File Manager!\n";
    assert_eq!(&data[..signature.len()], signature);

    // Version is at offset 30, 4 bytes little-endian
    let version = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);
    assert_eq!(version, 0x0100_0002, "Version should be 0x01000002");

    // Encrypted flag is at offset 34
    assert_eq!(data[34], 1, "Encrypted flag should be set");
}

#[test]
fn created_unencrypted_archive_has_encrypted_flag_unset() {
    let archive = Pk2::create_new_in_memory("").unwrap();
    let data: Vec<u8> = archive.into();

    // Encrypted flag is at offset 34
    assert_eq!(data[34], 0, "Encrypted flag should not be set");
}

#[test]
fn created_archive_can_be_reopened_with_same_key() {
    let key = "my_secret_key";
    let archive = Pk2::create_new_in_memory(key).unwrap();
    let data: Vec<u8> = archive.into();

    // Reopen the archive
    let cursor = Cursor::new(data);
    let reopened = Pk2::open_in(cursor, key);
    assert!(reopened.is_ok(), "Should be able to reopen archive with same key");
}

#[test]
fn created_archive_has_root_directory() {
    let archive = Pk2::create_new_in_memory("test").unwrap();
    let root = archive.open_root_dir();

    // Root directory should have "." entry pointing to itself
    let mut found_dot = false;
    for entry in root.entries() {
        if let pk2_sync::sync::DirEntry::Directory(dir) = entry
            && dir.name() == "."
        {
            found_dot = true;
            break;
        }
    }
    assert!(found_dot, "Root directory should contain '.' entry");
}
