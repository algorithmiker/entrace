use std::time::Instant;

use circular_buffer::CircularBuffer;

pub struct BenchmarkManager {
    pub get_tree: SamplingBenchmark<1>,
}
impl Default for BenchmarkManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkManager {
    pub fn new() -> Self {
        Self { get_tree: SamplingBenchmark::new("get_tree", false) }
    }
}
// a benchmark which samples only every N-th pass.
pub struct SamplingBenchmark<const N: u8> {
    pub name: &'static str,
    pub enabled: bool,
    pub cnt: u8,
    pub samples: CircularBuffer<8, u64>,
    pub cur_start: Option<Instant>,
}

impl<const N: u8> SamplingBenchmark<N> {
    pub fn new(name: &'static str, enabled: bool) -> Self {
        SamplingBenchmark { name, enabled, cnt: 0, samples: CircularBuffer::new(), cur_start: None }
    }
    pub fn get_average_us(&self) -> Option<u64> {
        if !self.enabled {
            return None;
        };
        let sum: u64 = self.samples.iter().sum();
        sum.checked_div(self.samples.len() as u64)
    }
    pub fn start_pass(&mut self) {
        if !self.enabled {
            return;
        };
        if self.cnt % N == 0 {
            self.cur_start = Some(Instant::now());
        }
        self.cnt = self.cnt.wrapping_add(1);
    }

    pub fn end_pass(&mut self) {
        if !self.enabled {
            return;
        };
        if let Some(start) = self.cur_start {
            self.samples.push_back(start.elapsed().as_micros() as u64);
            self.cur_start = None;
        }
    }
}
