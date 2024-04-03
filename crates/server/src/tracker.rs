/// Tracks changes in a value
pub struct Tracker<T> {
    previous: T,
    current: T,
}

impl<T: Clone> Tracker<T> {
    pub fn new(value: T) -> Self {
        Self {
            previous: value.clone(),
            current: value.clone(),
        }
    }

    pub fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut T),
    {
        f(&mut self.current);
    }

    pub const fn previous(&self) -> &T {
        &self.previous
    }

    pub const fn current(&self) -> &T {
        &self.current
    }

    pub fn update_previous(&mut self) {
        self.previous = self.current.clone();
    }
}
