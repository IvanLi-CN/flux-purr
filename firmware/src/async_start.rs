//! Coordinate a synchronous start with an asynchronous resource owner.

use core::{
    future::{Future, poll_fn},
    pin::pin,
    task::Poll,
};

/// Waits for a resource guard, constructs and first-polls the operation while
/// holding it, then releases it before awaiting any remaining asynchronous work.
/// The operation keeps its original future, result and event registration.
pub async fn after_resource_idle<A, G, F, O>(acquire: A, operation: F) -> O::Output
where
    A: Future<Output = G>,
    F: FnOnce() -> O,
    O: Future,
{
    let guard = acquire.await;
    let mut operation = pin!(operation());
    // This poll_fn is always ready: releasing the guard must not depend on
    // the executor scheduling this task again after a Pending start event.
    let first = poll_fn(|cx| Poll::Ready(operation.as_mut().poll(cx))).await;
    drop(guard);
    match first {
        Poll::Ready(result) => result,
        Poll::Pending => operation.await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{
        cell::Cell,
        pin::Pin,
        task::{Context, Waker},
    };

    #[derive(Default)]
    struct Bus {
        occupied: Cell<bool>,
    }

    impl Bus {
        fn try_acquire(&self) -> Option<Guard<'_>> {
            if self.occupied.replace(true) {
                None
            } else {
                Some(Guard(self))
            }
        }

        async fn acquire(&self) -> Guard<'_> {
            poll_fn(|_| self.try_acquire().map_or(Poll::Pending, Poll::Ready)).await
        }
    }

    struct Guard<'a>(&'a Bus);

    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.occupied.set(false);
        }
    }

    fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(Waker::noop()))
    }

    #[test]
    fn in_flight_transaction_finishes_before_synchronous_start() {
        let bus = Bus::default();
        let transaction = bus.try_acquire().unwrap();
        let started = Cell::new(false);
        let remaining_event_ready = Cell::new(false);
        let operation = after_resource_idle(bus.acquire(), || {
            assert!(bus.occupied.get());
            started.set(true);
            poll_fn(|_| {
                remaining_event_ready
                    .get()
                    .then_some(37)
                    .map_or(Poll::Pending, Poll::Ready)
            })
        });
        let mut operation = pin!(operation);

        assert_eq!(poll(operation.as_mut()), Poll::Pending);
        assert!(
            !started.get(),
            "Wi-Fi must not interrupt the I2C transaction"
        );
        drop(transaction);
        assert_eq!(poll(operation.as_mut()), Poll::Pending);
        assert!(started.get());
        let later_pd_turn = bus.try_acquire().expect("start-event wait releases I2C");
        remaining_event_ready.set(true);
        assert_eq!(poll(operation.as_mut()), Poll::Ready(37));
        assert!(bus.occupied.get(), "must not release another user's guard");
        drop(later_pd_turn);
    }

    #[test]
    fn first_poll_is_protected_but_event_wait_is_not() {
        let bus = Bus::default();
        let polls = Cell::new(0);
        let operation = after_resource_idle(bus.acquire(), || {
            poll_fn(|_| {
                polls.set(polls.get() + 1);
                assert_eq!(bus.occupied.get(), polls.get() == 1);
                Poll::<()>::Pending
            })
        });
        let mut operation = pin!(operation);
        assert_eq!(poll(operation.as_mut()), Poll::Pending);
        assert_eq!(polls.get(), 2);
        assert!(bus.try_acquire().is_some());
    }

    #[test]
    fn immediate_result_and_error_release_the_guard() {
        let bus = Bus::default();
        for result in [Ok(19), Err("driver start failed")] {
            let operation = after_resource_idle(bus.acquire(), || async {
                assert!(bus.occupied.get());
                result
            });
            let mut operation = pin!(operation);
            assert_eq!(poll(operation.as_mut()), Poll::Ready(result));
            assert!(bus.try_acquire().is_some());
        }
    }

    #[test]
    fn cancelling_bus_wait_does_not_start_or_release_the_owner() {
        let bus = Bus::default();
        let transaction = bus.try_acquire().unwrap();
        let started = Cell::new(false);
        {
            let operation = after_resource_idle(bus.acquire(), || async {
                started.set(true);
            });
            let mut operation = pin!(operation);
            assert_eq!(poll(operation.as_mut()), Poll::Pending);
        }
        assert!(!started.get());
        assert!(bus.occupied.get());
        drop(transaction);
        assert!(bus.try_acquire().is_some());
    }

    #[test]
    fn cancelling_event_wait_keeps_bus_available() {
        let bus = Bus::default();
        let operation_dropped = Cell::new(false);
        struct PendingOperation<'a>(&'a Cell<bool>);
        impl Future for PendingOperation<'_> {
            type Output = ();
            fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
                Poll::Pending
            }
        }
        impl Drop for PendingOperation<'_> {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        {
            let operation =
                after_resource_idle(bus.acquire(), || PendingOperation(&operation_dropped));
            let mut operation = pin!(operation);
            assert_eq!(poll(operation.as_mut()), Poll::Pending);
            assert!(!bus.occupied.get());
        }
        assert!(operation_dropped.get());
        assert!(bus.try_acquire().is_some());
    }
}
