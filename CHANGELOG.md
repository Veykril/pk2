# Changelog

Notable changes to `pk2`, `pk2-sync`, and `pk2_mate` are recorded here.

## Unreleased

### Added

- `pk2`: Added `Blowfish::new_with_salt` for caller-supplied salts and exposed the default salt as `Blowfish::PK2_SALT`. `Blowfish::new` continues to use the default PK2 salt.

### Fixed

- `pk2`: Corrected the archive entry layout to use an 89-byte filename field instead of an 81-byte filename followed by an access timestamp. Writing now supports filenames of up to 88 encoded bytes plus a null terminator. Entry size and the offsets of creation time, modification time, and subsequent fields are unchanged.
- `pk2_mate`: Stopped restoring access times during extraction, since PK2 archives do not store them.

### Removed

- **Breaking:** Removed `pk2::entry::NonEmptyEntry::access_time`.
- **Breaking:** Removed `access_time()` from `pk2-sync`'s `File`, `FileMut`, and `Directory`, and `set_access_time()` from `FileMut`. Callers must stop reading or setting archive access times; creation and modification times remain supported.
