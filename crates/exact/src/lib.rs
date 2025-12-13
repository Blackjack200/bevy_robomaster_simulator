use std::mem::ManuallyDrop;

#[derive(Debug)]
pub enum ExactError {
    Insufficient,
    Overflow,
}

pub trait ExactExt<T> {
    type Output<const N: usize>;
    fn exact<const N: usize>(self) -> Result<Self::Output<N>, ExactError>;
}

pub trait ExactOneExt {
    type Output;
    fn into_single(self) -> Self::Output;
}

impl<T> ExactOneExt for [T; 1] {
    type Output = T;

    fn into_single(self) -> T {
        let array = ManuallyDrop::new(self);
        unsafe { std::ptr::read(&array[0]) }
    }
}
impl<T> ExactOneExt for Option<[T; 1]> {
    type Output = Option<T>;

    fn into_single(self) -> Option<T> {
        self.map(|v| v.into_single())
    }
}

impl<I, T> ExactExt<T> for I
where
    I: Iterator<Item = T>,
{
    type Output<const N: usize> = [T; N];

    fn exact<const N: usize>(mut self) -> Result<Self::Output<N>, ExactError> {
        use std::mem::MaybeUninit;
        let mut data: [MaybeUninit<T>; N] = [const { MaybeUninit::uninit() }; N];

        for i in 0..N {
            if let Some(val) = self.next() {
                data[i].write(val);
            } else {
                for j in 0..i {
                    unsafe { data[j].assume_init_drop() };
                }
                return Err(ExactError::Insufficient);
            }
        }

        if self.next().is_some() {
            for j in 0..N {
                unsafe { data[j].assume_init_drop() };
            }
            return Err(ExactError::Overflow);
        }
        Ok(unsafe { std::ptr::read(&data as *const _ as *const [T; N]) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug)]
    struct DropCounter<'a> {
        counter: &'a Cell<usize>,
    }

    impl<'a> Drop for DropCounter<'a> {
        fn drop(&mut self) {
            self.counter.set(self.counter.get() + 1);
        }
    }

    #[test]
    fn test_exact_ok() {
        let counter = Cell::new(0);
        let items = vec![
            DropCounter { counter: &counter },
            DropCounter { counter: &counter },
            DropCounter { counter: &counter },
        ];

        let result: Result<[DropCounter; 3], ExactError> = items.into_iter().exact();
        assert!(result.is_ok());
        let arr = result.unwrap();
        assert_eq!(counter.get(), 0);

        drop(arr);
        assert_eq!(counter.get(), 3);
    }

    #[test]
    fn test_exact_insufficient() {
        let counter = Cell::new(0);
        let items = vec![DropCounter { counter: &counter }];

        let result: Result<[DropCounter; 3], ExactError> = items.into_iter().exact();
        assert!(matches!(result, Err(ExactError::Insufficient)));
        assert_eq!(counter.get(), 1);
    }

    #[test]
    fn test_exact_overflow() {
        let counter = Cell::new(0);
        let items = vec![
            DropCounter { counter: &counter },
            DropCounter { counter: &counter },
            DropCounter { counter: &counter },
            DropCounter { counter: &counter },
        ];

        let result: Result<[DropCounter; 3], ExactError> = items.into_iter().exact();
        assert!(matches!(result, Err(ExactError::Overflow)));
        assert_eq!(counter.get(), 4);
    }

    #[test]
    fn test_into_single_move() {
        let counter = Cell::new(0);
        let arr = [DropCounter { counter: &counter }];

        let val = arr.into_single();
        assert_eq!(counter.get(), 0);
        drop(val);
        assert_eq!(counter.get(), 1);
    }
}
