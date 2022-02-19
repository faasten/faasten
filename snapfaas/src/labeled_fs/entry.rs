use super::*;


impl LabeledEntry {
    pub fn new_empty(label: Label) -> Self {
        Self { label, inner: T::new }
    }
}
