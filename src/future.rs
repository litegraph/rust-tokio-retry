use either::Either;
use futures::{Async, IntoFuture, Future, Poll};
use futures::future::{Flatten, FutureResult};
use std::iter::{Iterator, IntoIterator};
use std::error::Error;
use std::io;
use std::cmp;
use std::fmt;
use std::time::Duration;
#[cfg(feature = "tokio_timer")]
use tokio_timer;
#[cfg(feature = "tokio_core")]
use tokio_core::reactor;

use super::Action;

pub trait Sleep {
    type Future: Future;
    fn sleep(&mut self, duration: Duration) -> Self::Future;
}

#[cfg(feature = "tokio_timer")]
impl Sleep for tokio_timer::Timer {
    type Future = tokio_timer::Sleep;
    fn sleep(&mut self, duration: Duration) -> Self::Future {
        tokio_timer::Timer::sleep(self, duration)
    }
}

#[cfg(feature = "tokio_core")]
impl Sleep for reactor::Handle {
    type Future = Flatten<FutureResult<reactor::Timeout, io::Error>>;
    fn sleep(&mut self, duration: Duration) -> Self::Future {
        reactor::Timeout::new(duration, self).into_future().flatten()
    }
}

/// Represents the errors possible during the execution of the `RetryFuture`.
#[derive(Debug)]
pub enum RetryError<OE, TE> {
    OperationError(OE),
    TimerError(TE)
}

impl<OE: cmp::PartialEq, TE> cmp::PartialEq for RetryError<OE, TE> {
    fn eq(&self, other: &RetryError<OE, TE>) -> bool  {
        match (self, other) {
            (&RetryError::TimerError(_), _) => false,
            (_, &RetryError::TimerError(_)) => false,
            (&RetryError::OperationError(ref left_err), &RetryError::OperationError(ref right_err)) =>
                left_err.eq(right_err)
        }
    }
}

impl<OE: fmt::Display, TE: fmt::Display> fmt::Display for RetryError<OE, TE> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            RetryError::OperationError(ref err) => err.fmt(formatter),
            RetryError::TimerError(ref err) => err.fmt(formatter)
        }
    }
}

impl<OE: Error, TE: Error> Error for RetryError<OE, TE> {
    fn description(&self) -> &str {
        match *self {
            RetryError::OperationError(ref err) => err.description(),
            RetryError::TimerError(ref err) => err.description()
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            RetryError::OperationError(ref err) => Some(err),
            RetryError::TimerError(ref err) => Some(err)
        }
    }
}

enum RetryState<S, A> where S: Sleep, A: Action {
    Running(A::Future),
    Sleeping(S::Future)
}

/// Future that drives multiple attempts at an action via a retry strategy.
pub struct RetryFuture<S, I, A> where S: Sleep, I: Iterator<Item=Duration>, A: Action {
    strategy: I,
    state: RetryState<S, A>,
    action: A,
    sleep: S
}

impl<S, I, A> RetryFuture<S, I, A> where S: Sleep, I: Iterator<Item=Duration>, A: Action {
    pub fn spawn<T: IntoIterator<IntoIter=I, Item=Duration>>(sleep: S, strategy: T, mut action: A) -> RetryFuture<S, I, A> {
        RetryFuture {
            strategy: strategy.into_iter(),
            state: RetryState::Running(action.run()),
            action: action,
            sleep: sleep
        }
    }

    fn attempt(&mut self) -> Poll<A::Item, RetryError<A::Error, <S::Future as Future>::Error>> {
        let future = self.action.run();
        self.state = RetryState::Running(future);
        return self.poll();
    }

    fn retry(&mut self, err: A::Error) -> Poll<A::Item, RetryError<A::Error, <S::Future as Future>::Error>> {
        match self.strategy.next() {
            None => Err(RetryError::OperationError(err)),
            Some(duration) => {
                let future = self.sleep.sleep(duration);
                self.state = RetryState::Sleeping(future);
                return self.poll();
            }
        }
    }
}

impl<S, I, A> Future for RetryFuture<S, I, A> where S: Sleep, I: Iterator<Item=Duration>, A: Action {
    type Item = A::Item;
    type Error = RetryError<A::Error, <S::Future as Future>::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let result = match self.state {
            RetryState::Running(ref mut future) =>
                Either::Left(future.poll()),
            RetryState::Sleeping(ref mut future) =>
                Either::Right(future.poll().map_err(RetryError::TimerError))
        };

        match result {
            Either::Left(poll_result) => match poll_result {
                Ok(async) => Ok(async),
                Err(err) => self.retry(err)
            },
            Either::Right(poll_result) => match poll_result? {
                Async::NotReady => Ok(Async::NotReady),
                Async::Ready(_) => self.attempt()
            }
        }
    }
}

#[test]
fn attempts_just_once() {
    use std::default::Default;
    use std::iter::empty;
    let mut num_calls = 0;
    let res = RetryFuture::spawn(tokio_timer::Timer::default(), empty(), || {
        num_calls += 1;
        Err::<(), u64>(42)
    }).wait();

    assert_eq!(res, Err(RetryError::OperationError(42)));
    assert_eq!(num_calls, 1);
}

#[test]
fn attempts_until_max_retries_exceeded() {
    use std::default::Default;
    use std::time::Duration;
    use super::strategy::FixedInterval;
    let s = FixedInterval::new(Duration::from_millis(100)).take(2);
    let mut num_calls = 0;
    let res = RetryFuture::spawn(tokio_timer::Timer::default(), s, || {
        num_calls += 1;
        Err::<(), u64>(42)
    }).wait();

    assert_eq!(res, Err(RetryError::OperationError(42)));
    assert_eq!(num_calls, 3);
}

#[test]
fn attempts_until_success() {
    use std::default::Default;
    use std::time::Duration;
    use super::strategy::FixedInterval;
    let s = FixedInterval::new(Duration::from_millis(100));
    let mut num_calls = 0;
    let res = RetryFuture::spawn(tokio_timer::Timer::default(), s, || {
        num_calls += 1;
        if num_calls < 4 {
            Err::<(), u64>(42)
        } else {
            Ok::<(), u64>(())
        }
    }).wait();

    assert_eq!(res, Ok(()));
    assert_eq!(num_calls, 4);
}
