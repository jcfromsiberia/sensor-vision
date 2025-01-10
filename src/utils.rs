use strum::IntoEnumIterator;

pub trait CircularEnum: IntoEnumIterator + Sized + PartialEq {
    fn next(&self) -> Self {
        let current_index = Self::iter()
            .enumerate()
            .find(|(_, item)| item == self)
            .unwrap()
            .0;
        Self::iter().cycle().skip(current_index + 1).next().unwrap()
    }

    fn prev(&self) -> Self {
        let rev_current_index = Self::iter()
            .rev()
            .enumerate()
            .find(|(_, item)| item == self)
            .unwrap()
            .0;
        Self::iter().rev().cycle().skip(rev_current_index + 1).next().unwrap()
    }
}

impl<T> CircularEnum for T where T: IntoEnumIterator + Sized + PartialEq {}
