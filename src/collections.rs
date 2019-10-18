use num_traits::{Unsigned, Bounded, AsPrimitive, WrappingAdd, WrappingSub};

/// A ring buffer of sequence numbers and associated values.
/// The ring buffer can have any size, but it should preferably be a power of two.
/// Any other size may result in a __reduction__ in the total number of sequence numbers available.
/// For example:
/// 
/// With sequence numbers of type _T = u8_ and a size of _7_, the total number of available sequence numbers become:
///
/// _(255 / 7) * 7 == 252_
#[derive(Debug, Clone)]
pub struct SequenceRingBuffer<T, V> {
    size: T,
    max: T,
    current: T,
    buffer: Vec<Option<V>>
}

impl<T, V> SequenceRingBuffer<T, V> where T: PartialOrd + Unsigned + Bounded + WrappingAdd + WrappingSub + AsPrimitive<usize>, V: Clone {
    /// Creates a new buffer with the specified size.
    /// The size limits the number of active (sequence number, value)-pairs that can be stored.
    /// Once the size is exceeded the oldest pair will be dropped.
    pub fn new(size: T) -> Self {
        Self {
            size,
            max: ((T::max_value() / size) * size),
            current: T::zero(),
            buffer: (0..size.as_()).map(|_| None).collect()
        }
    }

    /// Inserts the value and returns the associated sequence number.
    /// The entry with the oldest sequence number at the time will fall out of the buffer (being overwritten by the new one).
    pub fn insert(&mut self, value: V) -> T {
        let seq = self.current.wrapping_add(&T::one()) % self.max;
        let index: usize = (seq % self.size).as_();

        self.buffer[index] = Some(value);
        self.current = seq;
        
        seq
    }

    /// Checks whether the sequence number is within bounds, returning the actual index in the buffer if it is.
    fn within_bounds(&self, seq: &T) -> Option<T> {
        let high = self.current;
        let low = self.current.wrapping_sub(&self.size);

        if (low <= high && low < *seq && *seq <= high) || (low > high && high <= *seq && *seq < low) {
            Some(*seq % self.size)
        } else {
            None
        }
    }

    /// Removes the value associated with the sequence number, returning the value if it was found.
    pub fn remove(&mut self, seq: &T) -> Option<V> {
        self.within_bounds(seq).and_then(|index| {
            let value = self.buffer[index.as_()].clone();
            self.buffer[index.as_()] = None;

            value
        })
    }

    /// Gets a reference to the value associated with the sequence number, if it exists.
    pub fn get(&self, seq: &T) -> Option<&V> {
        self.within_bounds(seq).and_then(|index| {
            self.buffer.get(index.as_()).and_then(|value| {
                value.as_ref()
            })
        })
    }
}