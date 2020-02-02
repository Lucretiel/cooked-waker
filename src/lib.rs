use std::task;
use stowaway::{self, Stowaway};

pub trait RefWaker: Clone {
    fn wake_by_ref(&self);
}

pub trait Waker: RefWaker + Sized {
    fn wake(self) {
        self.wake_by_ref();
    }
}

impl<T: RefWaker> RefWaker for &'static T {
    fn wake_by_ref(&self) {
        T::wake_by_ref(*self)
    }
}

impl<T: RefWaker> Waker for &'static T {
    fn wake(self) {
        T::wake_by_ref(self)
    }
}

pub trait IntoWaker: Waker {
    fn into_waker(self) -> task::Waker;
}
#[derive(Debug, Clone, Default)]
struct ThreadParkWaker;

impl RefWaker for ThreadParkWaker {
    fn wake_by_ref(&self) {
        std::thread::current().unpark()
    }
}

impl Waker for ThreadParkWaker {
    fn wake(self) {
        std::thread::current().unpark()
    }
}

macro_rules! cook_waker {
    ($Waker:ty) => {
        impl $crate::IntoWaker for $Waker {
            #[inline]
            #[must_use]
            fn into_waker(self) -> std::task::Waker {
                let stowed = stowaway::stow(self);

                static VTABLE: std::task::RawWakerVTable = std::task::RawWakerVTable::new(
                    // clone
                    |raw| {
                        let raw = raw as *mut ();
                        let waker: &$Waker = unsafe { stowaway::ref_from_stowed(&raw) };
                        let cloned: $Waker = std::clone::Clone::clone(waker);
                        let stowed_clone = stowaway::stow(cloned);
                        std::task::RawWaker::new(stowed_clone, &VTABLE)
                    },
                    // wake by value
                    |raw| {
                        let waker: $Waker = unsafe { stowaway::unstow(raw as *mut ()) };
                        $crate::Waker::wake(waker);
                    },
                    // wake by ref
                    |raw| {
                        let raw = raw as *mut ();
                        let waker: &$Waker = unsafe { stowaway::ref_from_stowed(&raw) };
                        $crate::RefWaker::wake_by_ref(waker)
                    },
                    // Drop
                    |raw| {
                        let _waker: Stowaway<$Waker> =
                            unsafe { Stowaway::from_raw(raw as *mut ()) };
                    },
                );

                let raw_waker = std::task::RawWaker::new(stowed, &VTABLE);
                unsafe { std::task::Waker::from_raw(raw_waker) }
            }
        }
    };
}

cook_waker! {ThreadParkWaker}
