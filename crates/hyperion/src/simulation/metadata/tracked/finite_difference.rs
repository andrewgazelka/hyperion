#[derive(Debug)]
pub struct FiniteDifference<T> {
    prev: T,
    current: T,
}

impl<T> FiniteDifference<T> {
    pub fn new(value: T) -> Self
    where
        T: Clone,
    {
        Self {
            prev: value.clone(),
            current: value,
        }
    }

    /// Returns the difference quotient (Δy/Δx where Δx = 1)
    /// This is also known as the forward difference
    pub fn difference_quotient(&self) -> T
    where
        T: std::ops::Sub<Output = T> + Clone,
    {
        self.current.clone() - self.prev.clone()
    }

    /// Updates the value, shifting current to previous and setting new current
    pub fn set(&mut self, new_value: T) {
        self.current = new_value;
    }

    /// Returns true if the value has changed from previous to current
    pub fn has_changed(&self) -> bool
    where
        T: PartialEq,
    {
        self.prev != self.current
    }

    pub fn pop(&mut self) -> (T, &T)
    where
        T: Clone,
    {
        let cloned = self.current.clone();
        core::mem::swap(&mut self.prev, &mut self.current);
        let previous = core::mem::replace(&mut self.prev, cloned);
        (previous, &self.current)
    }

    /// Returns both values as a tuple
    pub fn into_tuple(self) -> (T, T) {
        (self.prev, self.current)
    }

    /// Returns references to both values as a tuple
    pub fn as_tuple(&self) -> (&T, &T) {
        (&self.prev, &self.current)
    }
}

impl<T: std::fmt::Display> std::fmt::Display for FiniteDifference<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Δ({} → {})", self.prev, self.current)
    }
}
