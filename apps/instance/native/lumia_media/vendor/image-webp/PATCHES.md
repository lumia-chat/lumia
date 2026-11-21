# image-webp 0.2.4, patched

This is image-webp 0.2.4 as published on crates.io (its sources, manifest, lock file and both
licences, MIT or Apache-2.0), with one change. `lumia_media` uses it through
`[patch.crates-io]` in its `Cargo.toml`, so `image` uses it too.

## What is changed

**A lossless stream's prefix codes count against the decoder's memory limit**
(`WebPDecoder::set_memory_limit`). Before, nothing counted them.

- `LosslessDecoder` has a `memory_limit` (default `usize::MAX`). `WebPDecoder` passes its own
  to every lossless stream: a still VP8L image, a VP8L animation frame, and a lossless alpha
  plane (`read_alpha_chunk` takes the limit).
- `read_huffman_codes` counts the group array (now allocated exactly), then each code before
  it is built. `HuffmanTree::build_implicit` and `build_two_node` return
  `DecodingError::MemoryLimitExceeded` instead of allocating tables past what is left.
- A test, `decoder::tests::lossless_prefix_codes_count_against_memory_limit`.

Files: `src/decoder.rs`, `src/extended.rs`, `src/huffman.rs`, `src/lossless.rs`. Nothing else
differs from the published crate.

## Why

The number of prefix-code groups in a lossless stream is set by the values in its entropy
image (up to 65,536), not by the image's size, and each group's five codes build tables of
their own. A 4 by 4 file of 42 KB allocated 286 MB this way, and one of 168 KB allocated
1.1 GB, under any limit. `test/support/fixtures/media/prefix-codes.webp` is the 42 KB one.
`lumia_media` bounds everything that scales with the pixels before it decodes (architecture 3
and 12). This is the one allocation it cannot know beforehand, so it hands image-webp whatever
budget is left and image-webp refuses when the codes would pass it.

## When to drop it

When an image-webp release counts prefix codes against its memory limit. Upstream knows the
bug as [image-webp#197](https://github.com/image-rs/image-webp/issues/197), and
[#211](https://github.com/image-rs/image-webp/pull/211) fixes it (unreleased when this was
written, 2026-11-21); that fix also drops unused groups' tables and holds every buffer to one
limit. To drop the patch:
- delete this directory, and the `[patch.crates-io]` entry and the `exclude` in `lumia_media`'s
  `Cargo.toml`;
- raise `image-webp` to that release;
- check that `lumia_media`'s test
  `a_lossless_webp_with_more_prefix_codes_than_fit_is_refused_as_they_pass_the_limit` still
  passes.

## Testing the patch

The crate is not a member of `lumia_media`'s workspace, because its dev-dependencies include
libwebp bindings, which build C. Its own tests run from this directory:

    cargo test --release --lib --target-dir ../../target/vendor
