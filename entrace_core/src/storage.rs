use std::thread::JoinHandle;

use tracing::Metadata;

use crate::Attrs;

pub trait Close {
    fn close(self);
}
impl Close for () {
    fn close(self) {}
}
impl<T> Close for JoinHandle<T> {
    fn close(self) {
        self.join().unwrap();
    }
}
impl Close for Vec<JoinHandle<()>> {
    fn close(self) {
        for x in self {
            x.close()
        }
    }
}

/// Used in the entrace backend to store data received by a [crate::tree_layer::TreeLayer]
pub trait Storage {
    fn new_span(&self, parent: u32, attrs: Attrs, meta: &'static Metadata<'_>);
    /// Implemented by default as a call to [Storage::new_span].
    fn new_event(&self, parent: u32, attrs: Attrs, meta: &'static Metadata<'_>) {
        self.new_span(parent, attrs, meta);
    }
}
