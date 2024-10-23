use ndarray::Array1;

#[derive(Debug, Clone)]
pub struct NdArrayStats {
    counts: Array1<u64>,
    means: Array1<f64>,
    m2s: Array1<f64>,
    mins: Array1<f64>,
    maxs: Array1<f64>,
    width: usize,
}

impl NdArrayStats {
    #[must_use]
    pub fn new(width: usize) -> Self {
        Self {
            counts: Array1::zeros(width),
            means: Array1::zeros(width),
            m2s: Array1::zeros(width),
            mins: Array1::from_elem(width, f64::INFINITY),
            maxs: Array1::from_elem(width, f64::NEG_INFINITY),
            width,
        }
    }

    pub fn update(&mut self, values: &[f64]) {
        assert_eq!(values.len(), self.width, "Input length must match width");

        let values = Array1::from_vec(values.to_vec());

        // Update counts
        self.counts += 1;
        let counts = self.counts.mapv(|x| x as f64);

        // Update means (Welford's algorithm)
        let delta = &values - &self.means;
        self.means = &self.means + &(&delta / &counts);

        // Update M2
        let delta2 = &values - &self.means;
        self.m2s = &self.m2s + &(delta * delta2);

        // Update mins and maxs
        self.mins.zip_mut_with(&values, |a, b| *a = a.min(*b));
        self.maxs.zip_mut_with(&values, |a, b| *a = a.max(*b));
    }

    #[must_use]
    pub fn count(&self, idx: usize) -> u64 {
        self.counts[idx]
    }

    #[must_use]
    pub fn mean(&self, idx: usize) -> Option<f64> {
        if self.counts[idx] > 0 {
            Some(self.means[idx])
        } else {
            None
        }
    }

    #[must_use]
    pub fn variance(&self, idx: usize) -> Option<f64> {
        if self.counts[idx] > 1 {
            Some(self.m2s[idx] / (self.counts[idx] - 1) as f64)
        } else {
            None
        }
    }

    #[must_use]
    pub fn std_dev(&self, idx: usize) -> Option<f64> {
        self.variance(idx).map(f64::sqrt)
    }

    #[must_use]
    pub fn min(&self, idx: usize) -> Option<f64> {
        if self.counts[idx] > 0 {
            Some(self.mins[idx])
        } else {
            None
        }
    }

    #[must_use]
    pub fn max(&self, idx: usize) -> Option<f64> {
        if self.counts[idx] > 0 {
            Some(self.maxs[idx])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;

    #[test]
    fn test_ndarray_stats() {
        let mut stats = NdArrayStats::new(4);

        // Update with 4 independent series
        let updates = [
            // Series 1: [2.0, 4.0]
            // Series 2: [3.0, 6.0]
            // Series 3: [4.0, 8.0]
            // Series 4: [5.0, 10.0]
            vec![2.0, 3.0, 4.0, 5.0],
            vec![4.0, 6.0, 8.0, 10.0],
        ];

        for update in &updates {
            stats.update(update);
        }

        // Test series 1 (index 0)
        assert_eq!(stats.count(0), 2);
        assert_relative_eq!(stats.mean(0).unwrap(), 3.0);
        assert_relative_eq!(stats.min(0).unwrap(), 2.0);
        assert_relative_eq!(stats.max(0).unwrap(), 4.0);

        // Test series 2 (index 1)
        assert_eq!(stats.count(1), 2);
        assert_relative_eq!(stats.mean(1).unwrap(), 4.5);
        assert_relative_eq!(stats.min(1).unwrap(), 3.0);
        assert_relative_eq!(stats.max(1).unwrap(), 6.0);

        // Test series 3 (index 2)
        assert_eq!(stats.count(2), 2);
        assert_relative_eq!(stats.mean(2).unwrap(), 6.0);
        assert_relative_eq!(stats.min(2).unwrap(), 4.0);
        assert_relative_eq!(stats.max(2).unwrap(), 8.0);

        // Test series 4 (index 3)
        assert_eq!(stats.count(3), 2);
        assert_relative_eq!(stats.mean(3).unwrap(), 7.5);
        assert_relative_eq!(stats.min(3).unwrap(), 5.0);
        assert_relative_eq!(stats.max(3).unwrap(), 10.0);
    }
}
