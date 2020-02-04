#![no_std]

//! cooked_waker provides traits for working with [`std::task::Waker`][Waker]
//! and, more importantly, a set of derives for safely converting normal,
//! safe rust types into `Waker` instances.
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
//! more information

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
/// and `&'static T`.
pub trait WakeRef {
    /// Wake up the task by reference. In general [`Wake::wake`] should be
    /// preferred, if available, as it's probably more efficient.
    ///
    /// This function is called by [`Waker::wake_by_ref`]
    fn wake_by_ref(&self);
}

/// Wakers that can wake by value. This is the primary means of waking a task
pub trait Wake: WakeRef + Sized {
    /// Wake up the task by value. By default, this simply calls
    /// [`WakeRef::wake_by_ref`].
    ///
    /// This function is called by [`Waker::wake`]
    fn wake(self) {
        self.wake_by_ref()
    }
}

/// Objects that can be converted into an [`Waker`]. You should
/// usually be able to derive this trait for any concrete type that implements
/// [`Waker`] and [`Clone`].
///
/// Note that, due to limitations in how generics interact with statics, it
/// is not currently possible to implement this trait generically (otherwise
/// we'd simply have a global implementation for all `T: Waker + Clone`.)
/// Therefore, any implementation must create a
/// [`RawWakerVTable`][core::task::RawWakerVTable] associated
/// with the concrete type `Self`, and find a way to convert `Self` to and
/// from a `RawWaker`.
///
/// This trait can be derived for any *concrete* `Wake + Clone` type. This
/// derive sets up a `RawWakerVTable` for the type, and arranges a conversion
/// into a `Waker` through the `stowaway` crate, which allows packing the bytes
/// of any sized type into a pointer (boxing it if it's too large to fit)
pub trait IntoWaker: Wake + Clone + Send + Sync {
    /// Convert this object into a `Waker`. Note that this must be safe:
    /// the `Waker` must take ownership of `Self` and correctly manage its
    /// operation and lifetime.
    fn into_waker(self) -> Waker;
}

// Waker implementations for std types.
impl<T: WakeRef> WakeRef for &'static T {
    #[inline]
    fn wake_by_ref(&self) {
        T::wake_by_ref(*self)
    }
}

impl<T: WakeRef> Wake for &'static T {}

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

impl<T: WakeRef + ?Sized> Wake for arc::Arc<T> {
    #[inline]
    fn wake(self) {
        T::wake_by_ref(self.as_ref())
    }
}

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
