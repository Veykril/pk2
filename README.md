# pk2

[![](https://tokei.rs/b1/github/Veykril/pk2)](https://github.com/Veykril/blend2d-rs)

A rust crate for reading and writing Silkroad Online's pk2 format

## pk2_mate

The [pk2_mate](./pk2_mate) binary contains 3 simplistic tools for working with pk2 archives.
- extract - extracts all files of a pk2 archive
- pack - packs all files of a directory into a new pk2 archive
- repack - repacks a pk2 archive into a new one(this gets rid of possible fragmentation)

For usage extraction of a particular tool run `pk2_mate 'tool' -h`(or `cargo run -p pk2_mate -- 'tool' -h` via cargo) with 'tool' replaced by the name of the tool. If no pk2 key is specified the tools will use the international silkroad online blowfish key by default.

## License

Licensed under the MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
