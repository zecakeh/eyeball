use std::{
    hash::{Hash, Hasher},
    mem, ops,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;
use tokio::sync::broadcast::{self, Sender};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

/// A value whose changes will be broadcast to subscribers.
///
/// `Observable<T>` dereferences to `T`, and does not have methods of its own to
/// not clash with methods of the inner type. Instead, to interact with the
/// `Observable` itself rather than the inner value, use its associated
/// functions (e.g. `Observable::subscribe(observable)`).
#[derive(Debug)]
pub struct Observable<T> {
    value: T,
    sender: Sender<T>,
}

impl<T: Clone + Send + 'static> Observable<T> {
    /// Create a new `Observable` with the given initial value.
    pub fn new(value: T) -> Self {
        let (sender, _) = broadcast::channel(1);
        Self { value, sender }
    }

    /// Obtain a new subscriber.
    pub fn subscribe(this: &Self) -> Subscriber<T> {
        let rx = this.sender.subscribe();
        Subscriber::new(BroadcastStream::new(rx))
    }

    /// Get a reference to the inner value.
    ///
    /// Usually, you don't need to call this function since `Observable<T>`
    /// implements `Deref`. Use this if you want to pass the inner value to a
    /// generic function where the compiler can't infer that you want to have
    /// the `Observable` dereferenced otherwise.
    pub fn get(this: &Self) -> &T {
        &this.value
    }

    /// Set the inner value to the given `value` and notify subscribers.
    pub fn set(this: &mut Self, value: T) {
        Self::replace(this, value);
    }

    /// Set the inner value to the given `value`, notify subscribers and return
    /// the previous value.
    pub fn replace(this: &mut Self, value: T) -> T {
        let result = mem::replace(&mut this.value, value);
        Self::broadcast_update(this);
        result
    }

    /// Update the inner value and notify subscribers.
    ///
    /// Note that even if the inner value is not actually changed by the
    /// closure, subscribers will be notified as if it was. Use one of the
    /// other update methods below if you want to conditionally mutate the
    /// inner value.
    pub fn update(this: &mut Self, f: impl FnOnce(&mut T)) {
        f(&mut this.value);
        Self::broadcast_update(this);
    }

    /// Update the inner value and notify subscribers if the updated value does
    /// not equal the previous value.
    pub fn update_eq(this: &mut Self, f: impl FnOnce(&mut T))
    where
        T: PartialEq,
    {
        let prev = this.value.clone();
        f(&mut this.value);
        if this.value != prev {
            Self::broadcast_update(this);
        }
    }

    /// Update the inner value and notify subscribers if the hash of the updated
    /// value does not equal the hash of the previous value.
    pub fn update_hash(this: &mut Self, f: impl FnOnce(&mut T))
    where
        T: Hash,
    {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        this.value.hash(&mut hasher);
        let prev_hash = hasher.finish();

        f(&mut this.value);

        let mut hasher = DefaultHasher::new();
        this.value.hash(&mut hasher);
        let new_hash = hasher.finish();

        if prev_hash != new_hash {
            Self::broadcast_update(this);
        }
    }

    fn broadcast_update(this: &Self) {
        if this.sender.receiver_count() != 0 {
            let _num_receivers = this.sender.send(this.value.clone()).unwrap_or(0);
            #[cfg(feature = "tracing")]
            tracing::debug!("New observable value broadcast to {_num_receivers} receivers");
        }
    }
}

// Note: No DerefMut because all mutating must go through inherent methods that
// notify subscribers
impl<T> ops::Deref for Observable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// A subscriber for updates of an [`Observable`].
///
/// Use its [`Stream`] implementation to interact with it (futures-util and
/// other futures-related crates have extension traits with convenience
/// methods).
#[derive(Debug)]
pub struct Subscriber<T> {
    inner: BroadcastStream<T>,
}

impl<T> Subscriber<T> {
    fn new(inner: BroadcastStream<T>) -> Self {
        Self { inner }
    }
}

impl<T: Clone + Send + 'static> Stream for Subscriber<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let poll = match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(value))) => Poll::Ready(Some(value)),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(_)))) => continue,
                Poll::Pending => Poll::Pending,
            };

            return poll;
        }
    }
}
