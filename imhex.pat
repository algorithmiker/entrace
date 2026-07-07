// this is an imhex pattern that can parse the header of an entrace file, to determine format/version.
// copy-paste into the pattern field of imhex, and click run.

u8 NULL @ 0;
char TAG[7] @ 1;
u8 VERSION @ 8;
enum StorageFormat : u8 {
 ET = 0,
 IET = 1,
 IETLengthPrefixed = 2
};
StorageFormat FORMAT @ 9;

// bincode varint. From the docs at https://github.com/bincode-org/bincode/blob/7195538d41fc1b9e21b3c427ecb2c7ba36b989dd/docs/spec.md
// | value range        | first byte  | postfix           |
// | ------------------ | ----------- | ----------------- |
// | x < 251            | x (as u8)   | -                 |
// | 251 <= x < 2^16    | 251         | u16  with value x |
// | 2^16 <= x < 2^32   | 252         | u32  with value x |
// | 2^32 <= x < 2^64   | 253         | u64  with value x |
// | 2^64 <= x < 2^128  | 254         | u128 with value x |
struct Varint {
    u8 first;
    if (first < 251) {
        u8 value = first;
    }
    if(first == 251) {
        u16 value;
    }
    if(first==252) {
        u32 value;
    }
    if(first==253) {
        u64 value;
    }
    if(first==254) {
        u128 value;
    }
    
} [[ format("format_varint")]];
fn format_varint(Varint u) {
    return u.value;
};
struct VarIntArray {
    Varint LEN;
    Varint VALUES[LEN.value];
};

struct ET_HEADER {
    if (FORMAT == StorageFormat::ET) {
        Varint RECORD_COUNT;
        Varint OFFSETS[RECORD_COUNT.value];
        VarIntArray CHILD_LISTS[RECORD_COUNT.value];
        
    }
};
ET_HEADER ET_HEADER @ 10;
