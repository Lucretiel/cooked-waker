#![no_std]

//! cooked_waker provides safe traits for working with
//! [`std::task::Waker`][Waker] and, more importantly, a set of derives for
//! safely converting normal, safe rust types into `Waker` instances. It cooks
//! `RawWaker` and `RawWakerVTable`, making them safe for consumption.
//!
//! It provides the [`Wake`] and [`WakeRef`] traits, which correspond to the
//! [`wake`][Waker::wake] and [`wake_by_ref`][Waker::wake_by_ref] methods
//! on [`std::task::Waker`][Waker], and it provides implenetations of these
//! types for the common reference & pointer types (`Arc`, `Rc`, `&'static`,
//! etc). These traits can also be derived for structs that have a single field
//! that implements `Wake` or `WakeRef`
//!
//! Additionally, it provides [`IntoWaker`], which allows converting any
//! `Wake + Clone` type into a [`Waker`]. Unfortunately, of limitations in
//! how generics interact with static, it's not possible to implement this
//! trait generically. We therefore instead provide a derive that can be
//! applied to any *concrete* type; see the [`IntoWaker`] documentation for
//! more information.
//!
//! # Basic example
//!
//! ```
//! use cooked_waker::{Wake, WakeRef, IntoWaker};
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use std::task::Waker;
//!
//! static wake_ref_count: AtomicUsize = AtomicUsize::new(0);
//! static wake_value_count: AtomicUsize = AtomicUsize::new(0);
//! static drop_count: AtomicUsize = AtomicUsize::new(0);
//!
//! // A simple Waker struct that atomically increments the relevant static
//! // counters. We can derive IntoWaker on it because it implenments Wake
//! // and Clone.
//! #[derive(Debug, Clone, IntoWaker)]
//! struct StaticWaker;
//!
//! impl WakeRef for StaticWaker {
//!     fn wake_by_ref(&self) {
//!         wake_ref_count.fetch_add(1, Ordering::SeqCst);
//!     }
//! }
//!
//! impl Wake for StaticWaker {
//!     fn wake(self) {
//!         wake_value_count.fetch_add(1, Ordering::SeqCst);
//!     }
//! }
//!
//! impl Drop for StaticWaker {
//!     fn drop(&mut self) {
//!         drop_count.fetch_add(1, Ordering::SeqCst);
//!     }
//! }
//!
//! assert_eq!(drop_count.load(Ordering::SeqCst), 0);
//!
//! let waker = StaticWaker;
//! {
//!     let waker1: Waker = waker.into_waker();
//!
//!     waker1.wake_by_ref();
//!     assert_eq!(wake_ref_count.load(Ordering::SeqCst), 1);
//!
//!     let waker2: Waker = waker1.clone();
//!     waker2.wake_by_ref();
//!     assert_eq!(wake_ref_count.load(Ordering::SeqCst), 2);
//!
//!     waker1.wake();
//!     assert_eq!(wake_value_count.load(Ordering::SeqCst), 1);
//!     assert_eq!(drop_count.load(Ordering::SeqCst), 1);
//! }
//! assert_eq!(drop_count.load(Ordering::SeqCst), 2);
//! ```
//!
//! # Arc example
//!
//! ```
//! use cooked_waker::{Wake, WakeRef, IntoWaker};
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use std::sync::Arc;
//! use std::task::Waker;
//!
//! // A simple struct that counts the number of times it is awoken. Can't
//! // be awoken by value (because that would discard the counter), so we
//! // must instead wrap it in an Arc (see CounterHandle)
//! #[derive(Debug, Default)]
//! struct Counter {
//!     // We use atomic usize because we need Send + Sync and also interior
//!     // mutability
//!     count: AtomicUsize,
//! }
//!
//! impl Counter {
//!     fn get(&self) -> usize {
//!         self.count.load(Ordering::SeqCst)
//!     }
//! }
//!
//! impl WakeRef for Counter {
//!     fn wake_by_ref(&self) {
//!         let _prev = self.count.fetch_add(1, Ordering::SeqCst);
//!     }
//! }
//!
//! // A shared handle to a Counter.
//! //
//! // We can derive Wake and WakeRef because the inner field implements
//! // them, and we can derive IntoWaker because this is a concrete type
//! // with Wake + Clone + Send + Sync. Note that *any* concrete type can have
//! // IntoWaker implemented for it; it doesn't have to be "pointer-sized"
//! #[derive(Debug, Clone, Default, WakeRef, Wake, IntoWaker)]
//! struct CounterHandle {
//!     counter: Arc<Counter>,
//! }
//!
//! impl CounterHandle {
//!     fn get(&self) -> usize {
//!         self.counter.get()
//!     }
//! }
//!
//! let counter = CounterHandle::default();
//!
//! // Create an std::task::Waker
//! let waker: Waker = counter.clone().into_waker();
//!
//! waker.wake_by_ref();
//! waker.wake_by_ref();
//!
//! let waker2 = waker.clone();
//! waker2.wake_by_ref();
//!
//! // This calls Counter::wake_by_ref because the Arc doesn't have exclusive
//! // ownership of the underlying Counter
//! waker2.wake();
//!
//! assert_eq!(counter.get(), 4);
//! ```

extern crate alloc;

#[cfg(feature = "derive")]
#[allow(unused_imports)]
#[macro_use]
extern crate cooked_waker_derive;

#[cfg(feature = "derive")]
pub use cooked_waker_derive::*;

use alloc::boxed::Box;
use alloc::rc;
use alloc::sync as arc;
use core::task::Waker;

// Needed so that the derive macro can use it without requiring downstream
// users to list it as a dependency
#[cfg(feature = "derive")]
#[doc(hidden)]
pub use stowaway;

/// Wakers that can wake by reference. This trait is used to enable a [`Wake`]
/// implementation for types that don't own an underlying handle, like `Arc<T>`
/// and `&T`.
///
/// This trait can be derived for `struct` types that have a single field that
/// implements `RefWake`. Unlike [`IntoWaker`], this can even be derived for
/// generic types and will correctly set up the relevant trait bounds, but note
/// that [`IntoWaker`] can still only be derived for concrete types.
pub trait WakeRef {
    /// Wake up the task by reference. In general [`Wake::wake`] should be
    /// preferred, if available, as it's probably more efficient.
    ///
    /// This function should be called by [`Waker::wake_by_ref`]; a derived
    /// `IntoWaker` implementation will set this up automatically.
    fn wake_by_ref(&self);
}

/// Wakers that can wake by value. This is the primary means of waking a task.
///
/// This trait is implemented for most container types, like `Box<T: Wake>`
/// and `Option<T: Wake>`. It is also implemented for shared pointer types like
/// `Arc<T>` and `&T`, but those implementations call `T::wake_by_ref`, because
/// they don't have ownership of the underlying `T`.
///
/// This trait can be derived for `struct` types that have a single field that
/// implements [`Wake`]. Unlike [`IntoWaker`], this can even be derived for
/// generic types and will correctly set up the relevant trait bounds, but note
/// that [`IntoWaker`] can still only be derived for concrete types.
pub trait Wake: WakeRef + Sized {
    /// Wake up the task by value. By default, this simply calls
    /// [`WakeRef::wake_by_ref`].
    ///
    /// This function should be called by [`Waker::wake`]; a derived
    /// `IntoWaker` implementation will set this up automatically.
    fn wake(self) {
        self.wake_by_ref()
    }
}

/// Objects that can be converted into an [`Waker`]. You should
/// usually be able to derive this trait for any concrete type that implements
/// [`Wake + Clone + Send + Sync + 'static`].
///
/// Note that, due to limitations in how generics interact with statics, it
/// is not currently possible to implement this trait generically (otherwise
/// we'd simply have a global implementation for all `T: Wake + Clone`).
/// Therefore, any implementation must manually create a
/// [`RawWakerVTable`] associated
/// with the concrete type `Self`, and find a way to convert `Self` to and
/// from a `RawWaker`.
///
/// This trait can be derived for any *concrete* type. This derive sets up a
/// [`RawWakerVTable`] for the type, and arranges a conversion into a [`Waker`]
/// through the `stowaway` crate, which allows packing the bytes of any sized
/// type into a pointer (boxing it if it's too large to fit). This means that
/// "large" waker structs will simply be boxed, but wakers that contain a
/// single `Box` or `Arc` field will simply move their pointer directly. This
/// `Waker` will then call the relevant `Wake`, `RefWake`, or `Clone` methods
/// throughout its lifecycle.
///
/// [`RawWakerVTable`]: core::task::RawWakerVTable
pub trait IntoWaker: Wake + Clone + Send + Sync + 'static {
    /// Convert this object into a `Waker`.
    #[must_use]
    fn into_waker(self) -> Waker;
}

// Waker implementations for std types.
impl<T: WakeRef> WakeRef for &T {
    #[inline]
    fn wake_by_ref(&self) {
        T::wake_by_ref(*self)
    }
}

impl<T: WakeRef> Wake for &T {}

impl<T: WakeRef + ?Sized> WakeRef for Box<T> {
    #[inline]
    fn wake_by_ref(&self) {
        T::wake_by_ref(self.as_ref())
    }
}

impl<T: Wake> Wake for Box<T> {
    #[inline]
    fn wake(self) {
        T::wake(*self)
    }
}

impl<T: WakeRef + ?Sized> WakeRef for arc::Arc<T> {
    #[inline]
    fn wake_by_ref(&self) {
        T::wake_by_ref(self.as_ref())
    }
}

impl<T: WakeRef + ?Sized> Wake for arc::Arc<T> {}

impl<T: WakeRef + ?Sized> WakeRef for arc::Weak<T> {
    #[inline]
    fn wake_by_ref(&self) {
        self.upgrade().wake()
    }
}

impl<T: WakeRef + ?Sized> Wake for arc::Weak<T> {}

impl<T: WakeRef + ?Sized> WakeRef for rc::Rc<T> {
    #[inline]
    fn wake_by_ref(&self) {
        T::wake_by_ref(self.as_ref())
    }
}

impl<T: WakeRef + ?Sized> Wake for rc::Rc<T> {
    #[inline]
    fn wake(self) {
        T::wake_by_ref(self.as_ref())
    }
}

impl<T: WakeRef + ?Sized> WakeRef for rc::Weak<T> {
    #[inline]
    fn wake_by_ref(&self) {
        self.upgrade().wake()
    }
}

impl<T: WakeRef + ?Sized> Wake for rc::Weak<T> {}

impl<T: WakeRef> WakeRef for Option<T> {
    #[inline]
    fn wake_by_ref(&self) {
        if let Some(ref waker) = *self {
            waker.wake_by_ref()
        }
    }
}

impl<T: Wake> Wake for Option<T> {
    #[inline]
    fn wake(self) {
        if let Some(waker) = self {
            waker.wake()
        }
    }
}

impl WakeRef for Waker {
    #[inline]
    fn wake_by_ref(&self) {
        Waker::wake_by_ref(self)
    }
}

impl Wake for Waker {
    #[inline]
    fn wake(self) {
        Waker::wake(self)
    }
}

impl IntoWaker for Waker {
    #[inline]
    fn into_waker(self) -> Waker {
        self
    }
}
