extern crate futures;
extern crate tokio;
extern crate tokio_timer;
extern crate tokio_retry;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::Future;
use futures::sync::oneshot::spawn_fn;
use tokio::runtime::Runtime;
use tokio_retry::{Error, Retry, RetryIf};

#[test]
fn attempts_just_once() {
    use std::iter::empty;
    let runtime = Runtime::new().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let cloned_counter = counter.clone();
    let res = spawn_fn(move || {
        Retry::spawn(empty(), move || {
            cloned_counter.fetch_add(1, Ordering::SeqCst);
            Err::<(), u64>(42)
        })
    }, &runtime.executor()).wait();

    assert_eq!(res, Err(Error::OperationError(42)));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn attempts_until_max_retries_exceeded() {
    use tokio_retry::strategy::FixedInterval;
    let s = FixedInterval::from_millis(100).take(2);
    let runtime = Runtime::new().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let cloned_counter = counter.clone();
    let res = spawn_fn(move || {
        Retry::spawn(s, move || {
            cloned_counter.fetch_add(1, Ordering::SeqCst);
            Err::<(), u64>(42)
        })
    }, &runtime.executor()).wait();

    assert_eq!(res, Err(Error::OperationError(42)));
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[test]
fn attempts_until_success() {
    use tokio_retry::strategy::FixedInterval;
    let s = FixedInterval::from_millis(100);
    let runtime = Runtime::new().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let cloned_counter = counter.clone();
    let res = spawn_fn(move || {
        Retry::spawn(s, move || {
            let previous = cloned_counter.fetch_add(1, Ordering::SeqCst);
            if previous < 3 {
                Err::<(), u64>(42)
            } else {
                Ok::<(), u64>(())
            }
        })
    }, &runtime.executor()).wait();

    assert_eq!(res, Ok(()));
    assert_eq!(counter.load(Ordering::SeqCst), 4);
}

#[test]
fn attempts_retry_only_if_given_condition_is_true() {
    use tokio_retry::strategy::FixedInterval;
    let s = FixedInterval::from_millis(100).take(5);
    let runtime = Runtime::new().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let cloned_counter = counter.clone();
    let res = spawn_fn(move || {
        RetryIf::spawn(s, move || {
            let previous  = cloned_counter.fetch_add(1, Ordering::SeqCst);
            Err::<(), usize>(previous + 1)
        }, |e: &usize| *e < 3)
    }, &runtime.executor()).wait();

    assert_eq!(res, Err(Error::OperationError(3)));
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}