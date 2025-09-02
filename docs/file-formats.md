# File formats
ENTRACE utilizes two distinct file formats, *`et`* (**E**n**T**race) and *`iet`* (**I**ntermediate **E**n**T**race).

The main data storage layout is identical in the two formats, the difference is in the headers and metadata only.
Therefore the two file formats are cheap to inter-convert.

The ENTRACE file formats are stable in practice, but should not be regarded as stable until ENTRACE reaches 1.0.
Of course, if the library reaches significant usage before then, conversion scripts to any new format(s) will be provided.

## Magic
Every ENTRACE file begins with a 10-byte magic number, which is structured like:
```rust
pub fn entrace_magic_for(version: u8, format: StorageFormat) -> [u8; 10] {
    let mut magic = [0, 69, 78, 84, 82, 65, 67, 69, 0, 0]; // b"\0ENTRACE" and two temporary 0s
    magic[8] = version;
    magic[9] = format as u8;
    magic
}
```

StorageFormat is currently:
```rust
pub enum StorageFormat {
    ET = 0,
    IET = 1,
    // Length-prefixed encoding of IET, used when sending traces over TCP
    IETPrefix = 2,
}
```

## `bincode`
ENTRACE uses [bincode](https://crates.io/crates/bincode) to read and write its data structures, specifically the `bincode::serde` family of functions.

Initially, ENCODE had its own set of encoders/decoders, but it was moved to `bincode` for code simplicity (as the old encoding was nearly identical to bincode's, too). 

## TraceEntry
The core of both file formats is a `TraceEntry`.
This is an on-disk block of data about a span.
Currently, TraceEntry is:
```rust
pub struct TraceEntry {
    pub parent: u32,
    pub message: Option<String>,
    pub metadata: MetadataContainer,
    pub attributes: Vec<(String, EnValue)>,
}
```

## PoolEntry
A PoolEntry is the implicit (non-data) information about a span, namely the edges it has in the span tree.
These are encoded by u32 indices into the Pool.
```rust
pub struct PoolEntry {
    pub children: Vec<u32>,
}
```
The Pool is a collection of `PoolEntry`-es (`Vec<PoolEntry`).
This gives the Pool an easily serializable, two-level structure, which is better for performance compared to a "true" pointer tree,
since a sizeable part of the pool stays in the CPU caches at all time.

## IET structure
The structure for IET is the simplest possible structure:

0. [magic](#Magic)
1. ([TraceEntry](#TraceEntry))*

## ET structure
ET is a modified version of the IET format, which gives up easy appendability for faster loading speeds[^1]. To be precise, ET has been designed with memory mappability in mind.

[^1]: Of course, you may append ET files, but since it involves re-writing the header, you'd have to move the data section back on every append (or reserve unused space in the front).

It is structured like:

0. [magic](#Magic)
1. offsets: ordered list of offsets (`Vec<u64>`) to the offsets of contained `TraceEntry`-es in the file, relative to the end of the header section
2. pool: list of [PoolEntry](#PoolEntry).
3. data section: ([TraceEntry](#TraceEntry))*

Since ET is designed to be an immutable data structure, optimized for reading traces, the `ETStorage` storage backend initially writes an IET file (while keeping track of the header data), then converts it into an ET file at shutdown time.
