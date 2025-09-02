use std::time::Instant;

pub enum FrameTimeTracker {
    Dummy,
    Culled(SamplingFrameTracker),
}
impl FrameTimeTracker {
    pub fn is_some(&self) -> bool {
        match self {
            FrameTimeTracker::Dummy => false,
            FrameTimeTracker::Culled(_) => true,
        }
    }
}

pub trait TrackFrameTime {
    fn start_frame(&mut self);
    fn end_frame(&mut self);
    fn get_average_us(&self) -> Option<u32>;
    fn get_average_us_cached(&mut self) -> Option<u32>;
}
impl TrackFrameTime for FrameTimeTracker {
    fn start_frame(&mut self) {
        match self {
            FrameTimeTracker::Dummy => (),
            FrameTimeTracker::Culled(t) => t.start_frame(),
        }
    }
    fn end_frame(&mut self) {
        match self {
            FrameTimeTracker::Dummy => (),
            FrameTimeTracker::Culled(t) => t.end_frame(),
        }
    }
    fn get_average_us(&self) -> Option<u32> {
        match self {
            FrameTimeTracker::Dummy => None,
            FrameTimeTracker::Culled(t) => t.get_average_us(),
        }
    }

    fn get_average_us_cached(&mut self) -> Option<u32> {
        match self {
            FrameTimeTracker::Dummy => None,
            FrameTimeTracker::Culled(t) => t.get_average_us_cached(),
        }
    }
}
pub struct SamplingFrameTracker {
    pub cnt: u8,
    pub samples: [u64; 8],
    pub cur_start: Option<Instant>,
    pub average_cache: Option<u32>,
}
impl SamplingFrameTracker {
    pub fn new() -> Self {
        Self { cnt: 0, cur_start: None, samples: [0; 8], average_cache: None }
    }
}

impl Default for SamplingFrameTracker {
    fn default() -> Self {
        Self::new()
    }
}
impl SamplingFrameTracker {
    fn clear_cache(&mut self) {
        self.average_cache = None;
    }
}
impl TrackFrameTime for SamplingFrameTracker {
    fn get_average_us(&self) -> Option<u32> {
        let sum: u64 = self.samples.iter().sum();

        sum.checked_div(self.samples.len() as u64).map(|x| x as u32)
    }

    fn get_average_us_cached(&mut self) -> Option<u32> {
        if let Some(x) = self.average_cache {
            Some(x)
        } else if let Some(y) = self.get_average_us() {
            self.average_cache = Some(y);
            Some(y)
        } else {
            None
        }
    }

    fn start_frame(&mut self) {
        if self.cnt % 16 == 0 {
            self.cur_start = Some(Instant::now());
        }
        self.cnt = self.cnt.wrapping_add(1);
    }

    fn end_frame(&mut self) {
        if let Some(start) = self.cur_start {
            for i in 1..(self.samples.len()) {
                self.samples[i - 1] = self.samples[i];
            }
            self.samples[self.samples.len() - 1] = start.elapsed().as_micros() as u64;
            self.cur_start = None;
            self.clear_cache();
        }
    }
}

pub fn us_to_human(mut us: u32) -> String {
    let mut b = String::new();
    fn component(b: &mut String, us: &mut u32, name: &str, in_us: u32) {
        let in_new = *us / in_us;
        if in_new != 0 {
            *us %= in_us;
            b.push_str(&format!("{in_new}{name} "));
        }
    }
    component(&mut b, &mut us, "s", 1000000);
    component(&mut b, &mut us, "ms", 1000);
    component(&mut b, &mut us, "us", 1);
    b.pop();
    b
}
pub fn us_to_human_u64(mut us: u64) -> String {
    let mut b = String::new();
    fn component(b: &mut String, us: &mut u64, name: &str, in_us: u64) {
        let in_new = *us / in_us;
        if in_new != 0 {
            *us %= in_us;
            b.push_str(&format!("{in_new}{name} "));
        }
    }
    component(&mut b, &mut us, "s", 1000000);
    component(&mut b, &mut us, "ms", 1000);
    component(&mut b, &mut us, "us", 1);
    b.pop();
    b
}
