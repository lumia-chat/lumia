# image-webp: a lossless WebP's prefix codes and the memory limit

Status (2026-11-21): **already known upstream; not to be filed.** The bug is [image-webp#197](https://github.com/image-rs/image-webp/issues/197) (opened 2026-10-24), and [image-webp#211](https://github.com/image-rs/image-webp/pull/211) (opened 2026-11-12, unreviewed when checked) fixes it more fully than the patch here: it drops the tables of groups the image never uses, and holds the lossless, ALPH, VP8 and animation buffers to one memory limit. Others there carry patches against 0.2.4 as Lumia does. image-rs's security policy treats out-of-memory as a normal bug, not a vulnerability. Until a release has #211, Lumia carries the change below in `apps/instance/native/lumia_media/vendor/image-webp` (its `PATCHES.md` says how to drop it); see plan step 12, "The review". What is below is kept for reference only: the issue and pull request texts were written before #197 and #211 were found.

- [`image-webp-prefix-codes.patch`](image-webp-prefix-codes.patch): against image-webp's `main` at `946b31adf319a8f87530742716d9e2776044c431`. It applies with `patch -p1` or `git apply`, passes upstream's `cargo test` (the new test included) and `cargo fmt --check`, and adds no clippy warnings. Its header lines are from `diff -ru`: before filing, make it a commit on a fork and use `git format-patch`, or open it as a pull request.
- The vendored copy is the same change against the 0.2.4 release, with 0.2.4's older Huffman tables.
- The figures for 0.2.4 below were measured with a counting allocator; those for `main` were worked out from its code, not measured.

## Issue text


**A small lossless WebP can make the decoder allocate gigabytes, whatever `set_memory_limit` says**

In a VP8L stream with meta prefix codes, the number of prefix-code groups is the largest
value in the entropy image plus one, up to 65,536, and has nothing to do with the image's
dimensions. `read_huffman_codes` reads every group, five codes each, and each code builds its
tables. None of this is checked against `WebPDecoder::set_memory_limit`, and image's
`Limits::max_alloc` does not reach it either.

A 4x4 image whose 1x1 entropy image names group 4,095, where each group's green code has
2,048 symbols of length 11 (one code-length symbol, so the 2,048 lengths cost no bits), comes to
42 KB. With 0.2.4 it allocates 286 MB of tables (counted with a counting allocator) and then
decodes. 16,384 groups is 168 KB of file and 1.1 GB. Every 66-bit code costs a 1,024-entry table
and a 4,096-node vector in 0.2.4. On `main`, with the new tables, a code like that costs about
6 KB (a 1,024-entry primary table and 2,048 secondary entries). All 65,536 groups then come to
about 400 MB from under 700 KB of input, and up to about 800 MB with codes that fill the
secondary table. Those figures for `main` are worked out from the code, not measured.

A service that decodes uploads under a memory budget cannot bound this from the outside: the
group count is in the entropy image, and each code's size is known only once its lengths are
read.

## Pull request text

The decoder's memory limit now covers what a lossless image stream's prefix codes allocate.
Each code's tables are checked against what the limit has left before they are allocated. When a
code would pass the limit, decoding fails with `DecodingError::MemoryLimitExceeded`.

- `LosslessDecoder` gets a `memory_limit` (default `usize::MAX`, so nothing changes unless a limit
  is set). `WebPDecoder` passes its own to every lossless stream it decodes: a still VP8L image, a
  VP8L animation frame, and a lossless alpha plane (`read_alpha_chunk` gains the parameter).
- `read_huffman_codes` first counts the group array (`Vec::with_capacity` now, rather than grown
  by doubling), then each code as it is read.
- `HuffmanTree::build_implicit` and `build_two_node` take the bytes left and return the error
  before they allocate a table that would not fit. On `main` this covers the primary table and
  each growth of the secondary table (now reserved exactly). `HuffmanTree::memory_usage` reports
  what a built code holds.
- The code-length code, which is dropped as soon as the lengths are read, is checked against the
  whole limit and not counted.
- Test: `lossless_prefix_codes_count_against_memory_limit` builds the stream above with 2 and with
  64 groups. Under a 256 KiB limit the first decodes and the second is refused. Without a limit
  both decode.

The limit applies to each image stream (the main image, each transform's subimage, the entropy
image). Their codes are not held at the same time, so one stream's codes never hold more than
the limit. Pixel buffers are still not counted, so the `TODO` on `set_memory_limit` stays.
