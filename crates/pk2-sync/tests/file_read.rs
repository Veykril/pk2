//! Tests for File read operations and seeking.

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use pk2_sync::sync::Pk2;

/// Helper to create an archive with a file containing the given data.
fn create_archive_with_file(name: &str, data: &[u8]) -> Pk2<Cursor<Vec<u8>>> {
    let mut archive = Pk2::create_new_in_memory("testkey").unwrap();
    {
        let mut file = archive.create_file(format!("/{}", name)).unwrap();
        file.write_all(data).unwrap();
        file.flush().unwrap();
    }
    archive
}

#[test]
fn read_file_contents() {
    let data = b"Hello, World!";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    assert_eq!(buf, data);
}

#[test]
fn read_file_partial() {
    let data = b"Hello, World!";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();
    let mut buf = [0u8; 5];
    let n = file.read(&mut buf).unwrap();

    assert_eq!(n, 5);
    assert_eq!(&buf, b"Hello");
}

#[test]
fn read_file_in_chunks() {
    let data = b"ABCDEFGHIJ";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    let mut buf = [0u8; 3];

    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"ABC");

    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"DEF");

    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"GHI");

    // Only 1 byte left
    let mut buf = [0u8; 1];
    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"J");
}

#[test]
fn read_exact_fails_at_eof() {
    let data = b"short";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();
    let mut buf = [0u8; 10];

    let result = file.read_exact(&mut buf);
    assert!(result.is_err(), "read_exact should fail when not enough data");
}

#[test]
fn read_returns_zero_at_eof() {
    let data = b"data";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    // Read all data first
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    // Further reads should return 0
    let mut buf = [0u8; 10];
    let n = file.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn file_size() {
    let data = b"12345678901234567890"; // 20 bytes
    let archive = create_archive_with_file("test.txt", data);

    let file = archive.open_file("/test.txt").unwrap();
    assert_eq!(file.size(), 20);
}

#[test]
fn file_name() {
    let archive = create_archive_with_file("myfile.txt", b"content");

    let file = archive.open_file("/myfile.txt").unwrap();
    assert_eq!(file.name(), "myfile.txt");
}

#[test]
fn seek_from_start() {
    let data = b"ABCDEFGHIJ";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    file.seek(SeekFrom::Start(5)).unwrap();

    let mut buf = [0u8; 3];
    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"FGH");
}

#[test]
fn seek_from_current_forward() {
    let data = b"ABCDEFGHIJ";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    // Read first 2 bytes
    let mut buf = [0u8; 2];
    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"AB");

    // Skip 3 bytes forward
    file.seek(SeekFrom::Current(3)).unwrap();

    // Now at position 5, read 2 more
    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"FG");
}

#[test]
fn seek_from_current_backward() {
    let data = b"ABCDEFGHIJ";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    // Seek to position 8
    file.seek(SeekFrom::Start(8)).unwrap();

    // Go back 3 bytes
    file.seek(SeekFrom::Current(-3)).unwrap();

    let mut buf = [0u8; 2];
    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"FG");
}

#[test]
fn seek_from_end() {
    let data = b"ABCDEFGHIJ";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    // Seek to 3 bytes before end
    file.seek(SeekFrom::End(-3)).unwrap();

    let mut buf = [0u8; 3];
    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"HIJ");
}

#[test]
fn seek_to_beginning() {
    let data = b"ABCDEFGHIJ";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    // Read some data
    let mut buf = [0u8; 5];
    file.read_exact(&mut buf).unwrap();

    // Seek back to start
    file.seek(SeekFrom::Start(0)).unwrap();

    // Read again
    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"ABCDE");
}

#[test]
fn seek_returns_new_position() {
    let data = b"ABCDEFGHIJ";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    let pos = file.seek(SeekFrom::Start(5)).unwrap();
    assert_eq!(pos, 5);

    let pos = file.seek(SeekFrom::Current(2)).unwrap();
    assert_eq!(pos, 7);

    let pos = file.seek(SeekFrom::End(-1)).unwrap();
    assert_eq!(pos, 9);
}

#[test]
fn seek_past_end_is_allowed() {
    let data = b"short";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    // Seeking past end is allowed (like regular files)
    let pos = file.seek(SeekFrom::Start(100)).unwrap();
    assert_eq!(pos, 100);
}

#[test]
fn seek_negative_fails() {
    let data = b"data";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    // Seeking to a negative position should fail
    let result = file.seek(SeekFrom::Current(-100));
    assert!(result.is_err());
}

#[test]
fn read_empty_file() {
    let archive = create_archive_with_file("empty.txt", b"");

    let mut file = archive.open_file("/empty.txt").unwrap();
    assert_eq!(file.size(), 0);

    let mut buf = Vec::new();
    let n = file.read_to_end(&mut buf).unwrap();
    assert_eq!(n, 0);
    assert!(buf.is_empty());
}

#[test]
fn read_large_file() {
    // Create a file larger than typical buffer sizes
    let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    let archive = create_archive_with_file("large.bin", &data);

    let mut file = archive.open_file("/large.bin").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    assert_eq!(buf.len(), data.len());
    assert_eq!(buf, data);
}

#[test]
fn read_binary_data() {
    // Test with binary data including null bytes
    let data: Vec<u8> = (0..=255).collect();
    let archive = create_archive_with_file("binary.bin", &data);

    let mut file = archive.open_file("/binary.bin").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    assert_eq!(buf, data);
}

#[test]
fn multiple_files_independent_read() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut f1 = archive.create_file("/file1.txt").unwrap();
        f1.write_all(b"Content of file 1").unwrap();
    }
    {
        let mut f2 = archive.create_file("/file2.txt").unwrap();
        f2.write_all(b"Content of file 2").unwrap();
    }

    let mut file1 = archive.open_file("/file1.txt").unwrap();
    let mut file2 = archive.open_file("/file2.txt").unwrap();

    let mut buf1 = Vec::new();
    let mut buf2 = Vec::new();

    file1.read_to_end(&mut buf1).unwrap();
    file2.read_to_end(&mut buf2).unwrap();

    assert_eq!(buf1, b"Content of file 1");
    assert_eq!(buf2, b"Content of file 2");
}

#[test]
fn file_is_copy() {
    let archive = create_archive_with_file("test.txt", b"data");

    let file1 = archive.open_file("/test.txt").unwrap();
    let file2 = file1; // Copy

    // Both should have the same properties
    assert_eq!(file1.name(), file2.name());
    assert_eq!(file1.size(), file2.size());
}

#[test]
fn file_clone_has_independent_seek_position() {
    let archive = create_archive_with_file("test.txt", b"ABCDEFGHIJ");

    let mut file1 = archive.open_file("/test.txt").unwrap();
    file1.seek(SeekFrom::Start(5)).unwrap();

    let mut file2 = file1;

    // Both start at the same position after clone
    let mut buf = [0u8; 2];
    file1.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"FG");

    // file2 also reads from position 5 (it's a copy with same state)
    file2.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"FG");
}

#[test]
fn open_file_in_subdirectory() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/subdir/nested.txt").unwrap();
        file.write_all(b"Nested content").unwrap();
    }

    let mut file = archive.open_file("/subdir/nested.txt").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    assert_eq!(buf, b"Nested content");
}

#[test]
fn open_nonexistent_file_fails() {
    let archive = Pk2::<Cursor<Vec<u8>>>::create_new_in_memory("test").unwrap();

    let result = archive.open_file("/nonexistent.txt");
    assert!(result.is_err());
}

#[test]
fn open_directory_as_file_fails() {
    let mut archive = Pk2::create_new_in_memory("test").unwrap();
    {
        let mut file = archive.create_file("/dir/file.txt").unwrap();
        file.write_all(b"data").unwrap();
    }

    let result = archive.open_file("/dir");
    assert!(result.is_err(), "Opening a directory as a file should fail");
}

#[test]
fn read_via_pk2_read_method() {
    let data = b"Quick read test";
    let archive = create_archive_with_file("quick.txt", data);

    let contents = archive.read("/quick.txt").unwrap();
    assert_eq!(contents, data);
}

#[test]
fn stream_position_after_read_to_end() {
    let data = b"ABCDEFGHIJ";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    // Position should be at end
    let pos = file.stream_position().unwrap();
    assert_eq!(pos, data.len() as u64);
}

#[test]
fn reread_after_seek_to_start() {
    let data = b"Rereadable content";
    let archive = create_archive_with_file("test.txt", data);

    let mut file = archive.open_file("/test.txt").unwrap();

    // First read
    let mut buf1 = Vec::new();
    file.read_to_end(&mut buf1).unwrap();

    // Seek back
    file.seek(SeekFrom::Start(0)).unwrap();

    // Second read
    let mut buf2 = Vec::new();
    file.read_to_end(&mut buf2).unwrap();

    assert_eq!(buf1, buf2);
    assert_eq!(buf1, data);
}
