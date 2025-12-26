//! Tests for FileMut write operations, buffering, and flushing.

use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::time::SystemTime;

use pk2_sync::sync::Pk2;

#[test]
fn write_and_read_back() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"Hello, World!").unwrap();
    }

    let mut file = archive.open_file("/test.txt").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    assert_eq!(buf, b"Hello, World!");
}

#[test]
fn write_returns_bytes_written() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    let mut file = archive.create_file("/test.txt").unwrap();

    let n = file.write(b"Hello").unwrap();
    assert_eq!(n, 5);
}

#[test]
fn write_multiple_times() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"Hello, ").unwrap();
        file.write_all(b"World!").unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"Hello, World!");
}

#[test]
fn write_empty_file() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/empty.txt").unwrap();
        file.write_all(b"").unwrap();
    }

    let file = archive.open_file("/empty.txt").unwrap();
    assert_eq!(file.size(), 0);
}

#[test]
fn flush_persists_data() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"Before flush").unwrap();
        file.flush().unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"Before flush");
}

#[test]
fn drop_flushes_automatically() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();

    // Create file and let it drop without explicit flush
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"Auto-flushed").unwrap();
        // No explicit flush, relying on Drop
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"Auto-flushed");
}

#[test]
fn write_large_data() {
    let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/large.bin").unwrap();
        file.write_all(&data).unwrap();
    }

    let contents = archive.read("/large.bin").unwrap();
    assert_eq!(contents.len(), data.len());
    assert_eq!(contents, data);
}

#[test]
fn write_binary_data() {
    // Test with all possible byte values
    let data: Vec<u8> = (0..=255).collect();

    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/binary.bin").unwrap();
        file.write_all(&data).unwrap();
    }

    let contents = archive.read("/binary.bin").unwrap();
    assert_eq!(contents, data);
}

#[test]
fn write_in_subdirectory() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/subdir/file.txt").unwrap();
        file.write_all(b"Nested content").unwrap();
    }

    let contents = archive.read("/subdir/file.txt").unwrap();
    assert_eq!(contents, b"Nested content");
}

#[test]
fn write_creates_nested_directories() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/a/b/c/deep.txt").unwrap();
        file.write_all(b"Deep file").unwrap();
    }

    let contents = archive.read("/a/b/c/deep.txt").unwrap();
    assert_eq!(contents, b"Deep file");

    // Verify intermediate directories exist
    assert!(archive.open_directory("/a").is_ok());
    assert!(archive.open_directory("/a/b").is_ok());
    assert!(archive.open_directory("/a/b/c").is_ok());
}

#[test]
fn file_mut_name() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    let file = archive.create_file("/myfile.txt").unwrap();

    assert_eq!(file.name(), "myfile.txt");
}

#[test]
fn file_mut_size_before_write() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    let file = archive.create_file("/new.txt").unwrap();

    // New file should have size 0
    assert_eq!(file.size(), 0);
}

#[test]
fn create_file_fails_if_exists() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/exists.txt").unwrap();
        file.write_all(b"First").unwrap();
    }

    let result = archive.create_file("/exists.txt");
    assert!(result.is_err(), "Creating a file that already exists should fail");
}

#[test]
fn create_file_truncate_new_file() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file_truncate("/new.txt").unwrap();
        file.write_all(b"Hello").unwrap();
    }

    let contents = archive.read("/new.txt").unwrap();
    assert_eq!(contents, b"Hello");
}

#[test]
fn create_file_truncate_existing_file() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/exists.txt").unwrap();
        file.write_all(b"Original content that is longer").unwrap();
    }

    // Truncate and write shorter content
    {
        let mut file = archive.create_file_truncate("/exists.txt").unwrap();
        file.write_all(b"Short").unwrap();
    }

    let contents = archive.read("/exists.txt").unwrap();
    assert_eq!(contents, b"Short");
}

#[test]
fn open_file_mut_existing() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"Original content").unwrap();
    }

    // Open for modification - note: this does a partial overwrite, not truncation
    {
        let mut file = archive.open_file_mut("/test.txt").unwrap();
        // Write same length content to fully replace
        file.write_all(b"Modified!content").unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"Modified!content");
}

#[test]
fn open_file_mut_read_existing_content() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"Existing data").unwrap();
    }

    {
        let mut file = archive.open_file_mut("/test.txt").unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"Existing data");
    }
}

#[test]
fn seek_while_writing() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"AAAAAAAAAA").unwrap(); // 10 A's
        file.seek(SeekFrom::Start(5)).unwrap();
        file.write_all(b"BBB").unwrap(); // Overwrite positions 5,6,7
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"AAAAABBBAA");
}

#[test]
fn seek_from_start_while_writing() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"0123456789").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(b"XXX").unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"XXX3456789");
}

#[test]
fn seek_from_end_while_writing() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"0123456789").unwrap();
        file.seek(SeekFrom::End(-3)).unwrap();
        file.write_all(b"END").unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"0123456END");
}

#[test]
fn seek_from_current_while_writing() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"ABCDEFGHIJ").unwrap();
        file.seek(SeekFrom::Start(2)).unwrap();
        file.seek(SeekFrom::Current(3)).unwrap(); // Now at position 5
        file.write_all(b"XX").unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"ABCDEXXHIJ");
}

#[test]
fn set_modify_time() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    let custom_time = SystemTime::UNIX_EPOCH;

    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.set_modify_time(custom_time);
        file.write_all(b"data").unwrap();
    }

    let file = archive.open_file("/test.txt").unwrap();
    assert_eq!(file.modify_time(), Some(custom_time));
}

#[test]
fn set_access_time() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    let custom_time = SystemTime::UNIX_EPOCH;

    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.set_access_time(custom_time);
        file.write_all(b"data").unwrap();
    }

    let file = archive.open_file("/test.txt").unwrap();
    assert_eq!(file.access_time(), Some(custom_time));
}

#[test]
fn set_create_time() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    let custom_time = SystemTime::UNIX_EPOCH;

    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.set_create_time(custom_time);
        file.write_all(b"data").unwrap();
    }

    let file = archive.open_file("/test.txt").unwrap();
    assert_eq!(file.create_time(), Some(custom_time));
}

#[test]
fn modify_time_updates_automatically() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    let before = SystemTime::now();

    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"data").unwrap();
    }

    let file = archive.open_file("/test.txt").unwrap();
    let modify_time = file.modify_time().unwrap();

    // Modify time should be set to around now
    assert!(modify_time >= before, "Modify time should be after test started");
}

#[test]
fn disable_automatic_modify_time_update() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    let custom_time = SystemTime::UNIX_EPOCH;

    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.set_modify_time(custom_time);
        file.update_modify_time(false); // Disable automatic update
        file.write_all(b"data").unwrap();
    }

    let file = archive.open_file("/test.txt").unwrap();
    assert_eq!(
        file.modify_time(),
        Some(custom_time),
        "Modify time should not have been auto-updated"
    );
}

#[test]
fn multiple_files_in_same_directory() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();

    {
        let mut file = archive.create_file("/file1.txt").unwrap();
        file.write_all(b"Content 1").unwrap();
    }
    {
        let mut file = archive.create_file("/file2.txt").unwrap();
        file.write_all(b"Content 2").unwrap();
    }
    {
        let mut file = archive.create_file("/file3.txt").unwrap();
        file.write_all(b"Content 3").unwrap();
    }

    assert_eq!(archive.read("/file1.txt").unwrap(), b"Content 1");
    assert_eq!(archive.read("/file2.txt").unwrap(), b"Content 2");
    assert_eq!(archive.read("/file3.txt").unwrap(), b"Content 3");
}

#[test]
fn write_after_read_on_file_mut() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"0123456789").unwrap();
    }

    {
        let mut file = archive.open_file_mut("/test.txt").unwrap();

        // Read first 5 bytes
        let mut buf = [0u8; 5];
        file.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"01234");

        // Write at current position (5)
        file.write_all(b"XXXXX").unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"01234XXXXX");
}

#[test]
fn flush_multiple_times() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();

        file.write_all(b"First").unwrap();
        file.flush().unwrap();

        file.seek(SeekFrom::End(0)).unwrap();
        file.write_all(b" Second").unwrap();
        file.flush().unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"First Second");
}

#[test]
fn write_extends_file() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"Short").unwrap();
    }

    {
        let mut file = archive.open_file_mut("/test.txt").unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        file.write_all(b" Extended content").unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    assert_eq!(contents, b"Short Extended content");
}

#[test]
fn overwrite_with_shorter_content() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/test.txt").unwrap();
        file.write_all(b"This is a long string").unwrap();
    }

    {
        let mut file = archive.open_file_mut("/test.txt").unwrap();
        // Writing shorter content only overwrites the beginning
        // The rest of the original content is preserved
        file.write_all(b"Short").unwrap();
    }

    let contents = archive.read("/test.txt").unwrap();
    // Only first 5 bytes changed, rest remains (note: "Shortis" not "Short is")
    // because "Short" overwrites "This " -> "Short" + "is a long string"
    assert_eq!(contents, b"Shortis a long string");
}

#[test]
fn delete_file() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/todelete.txt").unwrap();
        file.write_all(b"Delete me").unwrap();
    }

    // Verify file exists
    assert!(archive.open_file("/todelete.txt").is_ok());

    // Delete the file
    archive.delete_file("/todelete.txt").unwrap();

    // File should no longer exist
    assert!(archive.open_file("/todelete.txt").is_err());
}

#[test]
fn delete_nonexistent_file_fails() {
    let mut archive = Pk2::<Cursor<Vec<u8>>>::create_new_in_memory("test").unwrap();

    let result = archive.delete_file("/nonexistent.txt");
    assert!(result.is_err());
}

#[test]
fn write_preserves_other_files() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();

    // Create first file
    {
        let mut file = archive.create_file("/first.txt").unwrap();
        file.write_all(b"First file content").unwrap();
    }

    // Create second file
    {
        let mut file = archive.create_file("/second.txt").unwrap();
        file.write_all(b"Second file content").unwrap();
    }

    // Modify first file
    {
        let mut file = archive.open_file_mut("/first.txt").unwrap();
        file.write_all(b"Modified first").unwrap();
    }

    // Second file should be unchanged
    let contents = archive.read("/second.txt").unwrap();
    assert_eq!(contents, b"Second file content");
}

#[test]
fn write_in_chunks() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/chunked.txt").unwrap();

        // Write in small chunks
        for chunk in b"Hello, World!".chunks(3) {
            file.write_all(chunk).unwrap();
        }
    }

    let contents = archive.read("/chunked.txt").unwrap();
    assert_eq!(contents, b"Hello, World!");
}

#[test]
fn archive_roundtrip_with_files() {
    let key = "roundtrip_key";

    // Create archive with files
    let data = {
        let mut archive = Pk2::create_new_in_memory(key).unwrap();
        {
            let mut f = archive.create_file("/root.txt").unwrap();
            f.write_all(b"Root file").unwrap();
        }
        {
            let mut f = archive.create_file("/dir/nested.txt").unwrap();
            f.write_all(b"Nested file").unwrap();
        }
        let data: Vec<u8> = archive.into();
        data
    };

    // Reopen and verify
    let archive = Pk2::open_in(Cursor::new(data), key).unwrap();

    assert_eq!(archive.read("/root.txt").unwrap(), b"Root file");
    assert_eq!(archive.read("/dir/nested.txt").unwrap(), b"Nested file");
}
