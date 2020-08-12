#![no_std]

//! cooked_waker provides safe traits for working with
//! [`std::task::Waker`][Waker] and creating those wakers out of regular, safe
//! Rust structs. It cooks `RawWaker` and `RawWakerVTable`, making them safe
//! for consumption.
//!
//! It provides the [`Wake`] and [`WakeRef`] traits, which correspond to the
//! [`wake`][Waker::wake] and [`wake_by_ref`][Waker::wake_by_ref] methods
//! on [`std::task::Waker`][Waker], and it provides implenetations of these
//! types for the common reference & pointer types (`Arc`, `Rc`, `&'static`,
//! etc).
//!
//! Additionally, it provides [`IntoWaker`], which allows converting any
//! `Wake + Clone` type into a [`Waker`]. This trait is automatically derived
//! for any `Wake + Clone + Send + Sync + 'static` type.
//!
//! # Basic example
//!
//! ```
//! use cooked_waker::{Wake, WakeRef, IntoWaker, Stowable};
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use std::task::Waker;
//!
//! static wake_ref_count: AtomicUsize = AtomicUsize::new(0);
//! static wake_value_count: AtomicUsize = AtomicUsize::new(0);
//! static drop_count: AtomicUsize = AtomicUsize::new(0);
//!
//! // A simple Waker struct that atomically increments the relevant static
//! // counters.
//! #[derive(Debug, Clone)]
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
//! // See the stowaway docs for a description of this trait. Usually in
//! // practice you'll be using an Arc or Box, which require no unsafe.
//! unsafe impl Stowable for StaticWaker {}
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
//! // must instead wrap it in an Arc.
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
//! let counter_handle = Arc::new(Counter::default());
//!
//! // Create an std::task::Waker
//! let waker: Waker = counter_handle.clone().into_waker();
//!
//! waker.wake_by_ref();
//! waker.wake_by_ref();
//!
//! let waker2 = waker.clone();
//! waker2.wake_by_ref();
//!
//! // Because IntoWaker wrap the pointer directly, without additional
//! // boxing, we can use will_wake
//! assert!(waker.will_wake(&waker2));
//!
//! // This calls Counter::wake_by_ref because the Arc doesn't have exclusive
//! // ownership of the underlying Counter
//! waker2.wake();
//!
//! assert_eq!(counter_handle.get(), 4);
//! ```

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc;
use alloc::sync as arc;
use core::task::{RawWaker, RawWakerVTable, Waker};

pub use stowaway::Stowable;
use stowaway::{self, Stowaway};

/// Wakers that can wake by reference. This trait is used to enable a [`Wake`]
/// implementation for types that don't own an underlying handle, like `Arc<T>`
/// and `&T`.
///
/// This trait is implemented for most container and reference types, like
/// `&T where T: WakeRef`, `Box<T: WakeRef>`, and `Arc<T: WakeRef>`.
pub trait WakeRef {
    /// Wake up the task by reference. In general [`Wake::wake`] should be
    /// preferred, if available, as it's probably more efficient.
    ///
    /// A [`Waker`] created by [`IntoWaker`] will call this method through
    /// [`Waker::wake_by_ref`].
    fn wake_by_ref(&self);
}

/// Wakers that can wake by value. This is the primary means of waking a task.
///
/// This trait is implemented for most container types, like `Box<T: Wake>`
/// and `Option<T: Wake>`. It is also implemented for shared pointer types like
/// `Arc<T>` and `&T`, but those implementations call `T::wake_by_ref`, because
/// they don't have ownership of the underlying `T`.
pub trait Wake: WakeRef + Sized {
    /// Wake up the task by value. By default, this simply calls
    /// [`WakeRef::wake_by_ref`].
    ///
    /// A [`Waker`] created by [`IntoWaker`] will call this method through
    /// [`Waker::wake`].
    #[inline]
    fn wake(self) {
        self.wake_by_ref()
    }
}

/// Objects that can be converted into an [`Waker`]. This trait is
/// automatically implemented for any `Wake + Clone + Send + Sync + 'static +
/// Stowable` type.
///
/// The implementation of this trait sets up a [`RawWakerVTable`] for the type,
/// and arranges a conversion into a [`Waker`] through the [`stowaway`] crate,
/// which allows packing the bytes of any sized type into a pointer (boxing it
/// if it's too large to fit). This means that "large" waker structs will
/// simply be boxed, but wakers that contain a single `Box` or `Arc` field
/// (or any data smaller or the same size as a pointer) will simply move their
/// pointer directly. This `Waker` will then call the relevant `Wake`,
/// `RefWake`, or `Clone` methods throughout its lifecycle.
///
/// It should never be necessary to implement this trait manually.
///
/// [`RawWakerVTable`]: core::task::RawWakerVTable
/// [`Waker`]: core::task::Waker
/// [`stowaway`]: https://docs.rs/stowaway
pub trait IntoWaker: Wake + Clone + Send + Sync + 'static + Stowable {
    /// The RawWakerVTable for this type. This should never be used directly;
    /// it is entirely handled by `into_waker`. It is present as an associated
    /// const because that's the only way for it to work in generic contexts.
    #[doc(hidden)]
    const VTABLE: &'static RawWakerVTable;

    /// Convert this object into a `Waker`.
    #[must_use]
    fn into_waker(self) -> Waker;
}

impl<T: Wake + Clone + Send + Sync + 'static + Stowable> IntoWaker for T {
    const VTABLE: &'static RawWakerVTable = &RawWakerVTable::new(
        // clone
        |raw| {
            let raw = raw as *mut ();
            let waker: &T = unsafe { stowaway::ref_from_stowed(&raw) };
            let cloned = waker.clone();
            let stowed = Stowaway::new(cloned);
            RawWaker::new(Stowaway::into_raw(stowed), T::VTABLE)
        },
        // wake by value
        |raw| {
            let waker: T = unsafe { stowaway::unstow(raw as *mut ()) };
            Wake::wake(waker);
        },
        // wake by ref
        |raw| {
            let raw = raw as *mut ();
            let waker: &T = unsafe { stowaway::ref_from_stowed(&raw) };
            WakeRef::wake_by_ref(waker)
        },
        // Drop
        |raw| {
            let _waker: Stowaway<T> = unsafe { Stowaway::from_raw(raw as *mut ()) };
        },
    );

    fn into_waker(self) -> Waker {
        let stowed = Stowaway::new(self);
        let raw_waker = RawWaker::new(Stowaway::into_raw(stowed), T::VTABLE);
        unsafe { Waker::from_raw(raw_waker) }
    }
}

// Waker implementations for std types. Feel free to open PRs for additional
// stdlib types here.

// We'd prefer to implement WakeRef for T: Deref<Target=WakeRef>, but that
// results in type coherence issues with non-deref stdlib types.

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
        if let Some(waker) = self {
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
