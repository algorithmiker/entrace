use bitvec::vec::BitVec;
/// Poor man's roaring bitmap.
#[derive(Debug, Clone)]
pub enum EnBitVec {
    Vec(Vec<bool>),
    BitVec(BitVec<u64>),
}

impl EnBitVec {
    pub fn len_compressed(len: usize) -> bool {
        #[allow(non_snake_case)]
        let _100MB = 100 * 1024 * 1024;
        len > _100MB
    }
    pub fn is_compressed(&self) -> bool {
        match self {
            EnBitVec::Vec(_items) => false,
            EnBitVec::BitVec(_bit_vec) => true,
        }
    }
    pub fn push(&mut self, value: bool) {
        match self {
            EnBitVec::Vec(items) => {
                let len = items.len();
                if Self::len_compressed(len + 1) {
                    let mut bv: BitVec<u64> = BitVec::new();
                    bv.extend(items.iter());
                    bv.push(value);
                    *self = EnBitVec::BitVec(bv);
                    return;
                }
                items.push(value);
            }
            EnBitVec::BitVec(bit_vec) => {
                bit_vec.push(value);
            }
        }
    }
    pub fn set(&mut self, idx: usize, value: bool) {
        match self {
            EnBitVec::Vec(items) => items[idx] = value,
            EnBitVec::BitVec(bit_vec) => {
                bit_vec.set(idx, value);
            }
        }
    }
    pub fn get(&self, idx: usize) -> Option<bool> {
        match self {
            EnBitVec::Vec(items) => items.get(idx).copied(),
            EnBitVec::BitVec(bit_vec) => bit_vec.get(idx).map(|x| *x),
        }
    }
    /// Returns the new value.
    pub fn toggle(&mut self, idx: usize) -> Option<bool> {
        match self {
            EnBitVec::Vec(items) => {
                let v0 = items.get_mut(idx)?;
                *v0 = !(*v0);
                Some(*v0)
            }
            EnBitVec::BitVec(bit_vec) => {
                let mut v0 = bit_vec.get_mut(idx)?;
                *v0 = !(*v0);
                Some(*v0)
            }
        }
    }
    pub fn len(&self) -> usize {
        match self {
            EnBitVec::Vec(items) => items.len(),
            EnBitVec::BitVec(bit_vec) => bit_vec.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn repeat(value: bool, len: usize) -> Self {
        if Self::len_compressed(len) {
            return EnBitVec::BitVec(BitVec::repeat(value, len));
        }
        EnBitVec::Vec(vec![value; len])
    }
    pub fn new() -> Self {
        EnBitVec::Vec(vec![])
    }
    pub fn extend(&mut self, iter: impl IntoIterator<Item = bool>) {
        match self {
            EnBitVec::Vec(items) => items.extend(iter),
            EnBitVec::BitVec(bit_vec) => bit_vec.extend(iter),
        }
    }
    pub fn with_capacity(cap: usize) -> Self {
        if Self::len_compressed(cap) {
            Self::BitVec(BitVec::with_capacity(cap))
        } else {
            EnBitVec::Vec(Vec::with_capacity(cap))
        }
    }
}

impl Default for EnBitVec {
    fn default() -> Self {
        Self::new()
    }
}
