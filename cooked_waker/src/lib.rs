#[macro_use]
extern crate cooked_waker_derive;

pub use cooked_waker_derive::*;

// Needed so that the derive macro can use it without requiring downstream
// users to list it as a dependency
#[doc(hidden)]
pub use stowaway;

/// Wakers that can wake by reference. This trait is used to enable a [`Wake`]
/// implementation for types that don't own an underlying handle, like [`Arc<T>`]
/// and `&'static T`.
pub trait RefWake {
    /// Wake up the task by reference. In general [`Wake::wake`] should be
    /// preferred, as it's probably more efficient, but this method should
    /// be preferred to `waker.clone().wake()`.
    fn wake_by_ref(&self);
}

/// Wakers that can wake by value. B
pub trait Wake: RefWake + Sized {
    /// Wake up the task by value. By default, this simply calls wake_by_ref.
    #[inline]
    fn wake(self) {
        self.wake_by_ref()
    }
}

/// Objects that can be converted into an [`std::Waker`]. You should usually
/// be able to derive this trait for any concrete type that implements
/// [`Waker`] and [`Clone`].
///
/// Note that, due to limitations in how generics interact with statics, it
/// is not currently possible to implement this trait generically (otherwise
/// we'd simply have a global implementation for all `T: Waker + Clone`.)
pub trait IntoWaker: Wake + Clone {
    fn into_waker(self) -> core::task::Waker;
}

// Waker implementations for std types.
impl<T: RefWake> RefWake for &'static T {
    #[inline]
    fn wake_by_ref(&self) {
        T::wake_by_ref(*self)
    }
}

impl<T: RefWake> Wake for &'static T {}

impl<T: RefWake + ?Sized> RefWake for Box<T> {
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

impl<T: RefWake + ?Sized> RefWake for std::sync::Arc<T> {
    #[inline]
    fn wake_by_ref(&self) {
        T::wake_by_ref(self.as_ref())
    }
}

impl<T: RefWake + ?Sized> Wake for std::sync::Arc<T> {
    #[inline]
    fn wake(self) {
        T::wake_by_ref(self.as_ref())
    }
}

impl<T: RefWake + ?Sized> RefWake for std::sync::Weak<T> {
    #[inline]
    fn wake_by_ref(&self) {
        self.upgrade().wake()
    }
}

impl<T: RefWake + ?Sized> Wake for std::sync::Weak<T> {}

impl<T: RefWake + ?Sized> RefWake for std::rc::Rc<T> {
    #[inline]
    fn wake_by_ref(&self) {
        T::wake_by_ref(self.as_ref())
    }
}

impl<T: RefWake + ?Sized> Wake for std::rc::Rc<T> {
    #[inline]
    fn wake(self) {
        T::wake_by_ref(self.as_ref())
    }
}

impl<T: RefWake + ?Sized> RefWake for std::rc::Weak<T> {
    #[inline]
    fn wake_by_ref(&self) {
        self.upgrade().wake()
    }
}

impl<T: RefWake + ?Sized> Wake for std::rc::Weak<T> {}

impl<T: RefWake> RefWake for Option<T> {
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
