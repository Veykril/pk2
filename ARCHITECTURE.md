# PK2 Architecture

This document describes the high-level architecture of the `pk2` Rust crate, a library for reading and writing Silkroad Online's PK2 archive format.

## Overview

The project is organized as a Cargo workspace consisting of three packages:

```
pk2/
├── src/                    # Core pk2 library (no_std compatible)
├── crates/
│   └── pk2-sync/          # Higher-level synchronous I/O wrapper
└── pk2_mate/              # CLI tools for working with pk2 archives
```

## Package Hierarchy

```
┌─────────────────────────────────────────────────────────────┐
│                        pk2_mate                             │
│              (CLI tools: extract, pack, repack, list)       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                        pk2-sync                             │
│       (Synchronous file I/O, std::fs::File support)         │
│         Pk2, File, FileMut, Directory, DirEntry             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                          pk2                                │
│      (Core format parsing, no_std compatible primitives)    │
│   Blowfish, PackHeader, PackBlock, PackEntry, ChainIndex    │
└─────────────────────────────────────────────────────────────┘
```

---

## Core Library (`pk2`)

The core `pk2` crate provides low-level primitives for parsing and serializing the PK2 archive format via the sans-io approach. It is `no_std` compatible (with optional `std` feature) and uses `alloc` for heap allocations.

### Module Structure

```
pk2/src/
├── lib.rs              # Crate root, re-exports public API
├── blowfish.rs         # Little-endian Blowfish encryption implementation
├── error.rs            # Error types (ChainLookupError, HeaderError)
├── filetime.rs         # Windows FILETIME struct and conversions
├── parse.rs            # Low-level byte parsing utilities
├── format.rs           # Offset types (ChainOffset, BlockOffset, StreamOffset)
└── format/             # PK2 format handling structures
    ├── header.rs       # PackHeader - archive header
    ├── block_chain.rs  # PackBlockChain and PackBlock
    ├── chain_index.rs  # ChainIndex - in-memory file table index
    └── entry.rs        # PackEntry - file/directory entry
```

### Key Data Structures

#### `PackHeader`
The 256-byte archive header containing:
- Signature (`"JoyMax File Manager!\n"`)
- Version (`0x01000002`)
- Encryption flag
- Verification checksum (encrypted with Blowfish)

#### `PackEntry`
Represents a single entry in the archive:
- **Type**: Empty (0), Directory (1), or File (2)
- **Name**: Up to 81 bytes (EUC-KR encoded for Korean clients)
- **Timestamps**: Access, Create, Modify (Windows FILETIME format)
- **Position**: File data offset or directory children chain offset
- **Size**: File size in bytes
- **Next Block**: Pointer to continuation block

#### `PackBlock`
A collection of 20 `PackEntry` structures (2560 bytes total). Blocks are chained together via the `next_block` field in the last entry.

#### `PackBlockChain`
A linked list of `PackBlock`s representing a directory's contents. The chain is never empty.

#### `ChainIndex`
An in-memory hash map (`HashMap<ChainOffset, PackBlockChain>`) that indexes all block chains in the archive. Provides path resolution and lookup functionality.

### Archive Structure

```
┌────────────────────────────────────────────┐
│              PackHeader (256 bytes)        │
├────────────────────────────────────────────┤
│         Root PackBlock (2560 bytes)        │
│  ┌──────────────────────────────────────┐  │
│  │ Entry 0: "." (self reference)        │  │
│  │ Entry 1: ".." (parent reference)     │  │
│  │ Entry 2-18: Files/Directories        │  │
│  │ Entry 19: next_block pointer         │  │
│  └──────────────────────────────────────┘  │
├────────────────────────────────────────────┤
│         Additional Blocks...               │
├────────────────────────────────────────────┤
│         File Data Regions...               │
└────────────────────────────────────────────┘
```

### Encryption

PK2 archives use little-endian Blowfish encryption with a salted key derivation:

1. The user provides a key (default: `"169841"` for international Silkroad)
2. The key is XOR'd with a predefined salt (`PK2_SALT`)
3. The resulting key initializes the Blowfish cipher
4. All block data is encrypted/decrypted in 8-byte chunks

### Character Encoding

Original PK2 archives use EUC-KR encoding for file names. The `euc-kr` feature (enabled by default) provides proper encoding/decoding via the `encoding_rs` crate.

---

## Synchronous I/O Layer (`pk2-sync`)

The `pk2-sync` crate provides a higher-level, ergonomic API for working with PK2 archives using synchronous I/O.

### Module Structure

```
pk2-sync/src/
├── lib.rs    # Main Pk2 struct and creation/opening logic
├── fs.rs     # File, FileMut, Directory, DirEntry types
└── io.rs     # Low-level I/O operations (read/write blocks, entries)
```

### Threading Model

The crate supports both single-threaded and multi-threaded usage through a generic locking mechanism:

```rust
pub trait Lock<T>: Sized {
    fn new(inner: T) -> Self;
    fn into_inner(self) -> T;
    fn with_lock<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
}
```

Two lock implementations are provided:
- **`sync::Lock`**: Uses `std::sync::Mutex` for thread-safe access
- **`unsync::Lock`**: Uses `std::cell::RefCell` for single-threaded access

### Core Types

#### `Pk2<Buffer, Lock>`
The main archive handle, generic over:
- `Buffer`: The underlying I/O stream (e.g., `File`, `Cursor<Vec<u8>>`)
- `Lock`: The locking strategy (`sync::Lock` or `unsync::Lock`)

Key operations:
- `create_new()` / `open()` - Create or open archives
- `open_file()` / `open_file_mut()` - Open files for reading/writing
- `open_directory()` - Navigate directories
- `create_file()` / `delete_file()` - Modify archive contents

#### `File<'pk2, Buffer, Lock>`
A read-only handle to a file within the archive. Implements `Read` and `Seek`.

#### `FileMut<'pk2, Buffer, Lock>`
A mutable handle to a file. Implements `Read`, `Write`, and `Seek`. Data is buffered and flushed on drop.

#### `Directory<'pk2, Buffer, Lock>`
A handle to a directory. Provides iteration over entries via `entries()`.

#### `DirEntry<'pk2, Buffer, Lock>`
An enum representing either a `File` or `Directory` entry.

### I/O Operations

The `io` module provides low-level operations:
- `read_exact_at()` / `read_at()` - Read data at specific offsets
- `write_block()` / `write_entry_at()` - Write encrypted blocks/entries
- `append_data()` - Append file data to the archive
- `allocate_new_block_chain()` - Create new directory chains
- `allocate_empty_block()` - Extend block chains

---

## CLI Tools (`pk2_mate`)

A command-line utility providing four subcommands:

### Commands

| Command   | Description                                         |
|-----------|-----------------------------------------------------|
| `extract` | Extract all files from a PK2 archive to a directory |
| `pack`    | Pack a directory into a new PK2 archive             |
| `repack`  | Repack an archive to remove fragmentation           |
| `list`    | List contents of a PK2 archive                      |

### Usage Examples

```bash
# Extract archive
pk2_mate extract -a game.pk2 -o output_dir -k 169841

# Pack directory
pk2_mate pack -d my_files/ -a output.pk2 -k 169841

# Repack archive (defragment)
pk2_mate repack -a game.pk2 -o game_repacked.pk2

# List contents
pk2_mate list -a game.pk2
```

---

## Data Flow

### Reading a File

```
1. User calls pk2.open_file("/path/to/file")
2. ChainIndex resolves path components to chain offsets
3. Final entry lookup returns file metadata (offset, size)
4. File struct created with stream position
5. User calls file.read() -> seeks to data offset, reads bytes
```

### Writing a File

```
1. User calls pk2.create_file("/path/to/file")
2. Path resolved, directories created as needed
3. Empty entry allocated in parent directory's block chain
4. FileMut created with internal buffer
5. User writes data to buffer
6. On drop/flush: data appended to archive, entry updated with offset/size
```

### Opening an Archive

```
1. Read and validate PackHeader (256 bytes)
2. Verify encryption key against header checksum
3. Parse root block chain starting at offset 256
4. Recursively discover all directory chains via ChainIndexParser
5. Build ChainIndex hash map in memory
6. Return Pk2 handle ready for use
```

---

## Feature Flags

| Feature   | Default | Description                                      |
|-----------|---------|--------------------------------------------------|
| `euc-kr`  | Yes     | EUC-KR encoding support via `encoding_rs`        |
| `std`     | No      | Enable `std` library features                    |

---

## Testing

Tests are located in:
- `pk2/src/` - Unit tests within modules (e.g., `entry.rs`)
- `pk2/tests/test.rs` - Integration tests for archive creation and manipulation

Run tests with:
```bash
cargo test --all
```
