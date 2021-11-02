# pk2

[![Crates.io](https://img.shields.io/crates/v/pk2.svg)](https://crates.io/crates/pk2)
[![Docs.rs](https://docs.rs/pk2/badge.svg)](https://docs.rs/pk2)
![CI](https://github.com/veykril/pk2/workflows/CI/badge.svg)
[![](https://tokei.rs/b1/github/veykril/pk2)](https://github.com/veykril/pk2)

A rust crate for reading and writing Silkroad Online's pk2 format.

By default the crate pulls in [encoding_rs](https://crates.io/crates/encoding_rs) to properly work with the original pk2 files, since those use the [EUC-KR](https://en.wikipedia.org/wiki/Extended_Unix_Code#EUC-KR) encoding for file names. This dependency is feature gated behind the `euc-kr` feature.

## pk2_mate

The [pk2_mate](./pk2_mate) binary contains 3 simplistic tools for working with pk2 archives.
- extract - extracts all files of a pk2 archive
- pack - packs all files of a directory into a new pk2 archive
- repack - repacks a pk2 archive into a new one(this gets rid of possible fragmentation)

For usage extraction of a particular tool run `pk2_mate 'tool' -h`(or `cargo run -p pk2_mate -- 'tool' -h` via cargo) with 'tool' replaced by the name of the tool. If no pk2 key is specified the tools will use the international silkroad online blowfish key(169841) by default.

## License

Licensed under the MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
