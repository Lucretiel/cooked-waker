extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(IntoWaker)]
pub fn into_waker_derive(stream: TokenStream) -> TokenStream {
    let input = parse_macro_input!(stream as DeriveInput);

    if !input.generics.params.is_empty() {
        panic!("IntoWaker can only be derived for concrete types");
    }

    #[allow(non_snake_case)]
    let WakerStruct = input.ident;

    let implementation = quote! {
        impl cooked_waker::IntoWaker for #WakerStruct {
            #[must_use]
            fn into_waker(self) -> core::task::Waker {
                use core::task::{Waker, RawWaker, RawWakerVTable};
                use core::clone::Clone;
                use cooked_waker::{Wake, RefWake};
                use cooked_waker::stowaway::{self, Stowaway};

                let stowed = stowaway::stow(self);

                static VTABLE: RawWakerVTable = RawWakerVTable::new(
                    // clone
                    |raw| {
                        let raw = raw as *mut ();
                        let waker: & #WakerStruct = unsafe { stowaway::ref_from_stowed(&raw) };
                        let cloned: #WakerStruct = Clone::clone(waker);
                        let stowed_clone = stowaway::stow(cloned);
                        RawWaker::new(stowed_clone, &VTABLE)
                    },
                    // wake by value
                    |raw| {
                        let waker: #WakerStruct = unsafe { stowaway::unstow(raw as *mut ()) };
                        Wake::wake(waker);
                    },
                    // wake by ref
                    |raw| {
                        let raw = raw as *mut ();
                        let waker: & #WakerStruct = unsafe { stowaway::ref_from_stowed(&raw) };
                        RefWake::wake_by_ref(waker)
                    },
                    // Drop
                    |raw| {
                        let _waker: Stowaway<#WakerStruct> = unsafe {
                            Stowaway::from_raw(raw as *mut ())
                        };
                    },
                );

                let raw_waker = RawWaker::new(stowed, &VTABLE);
                unsafe { Waker::from_raw(raw_waker) }
            }
        }
    };

    implementation.into()
}
/*

#[proc_macro_derive(Waker)]
pub fn into_waker_derive(stream: TokenStream) -> TokenStream {
    let input = parse_macro_input!(stream as DeriveInput);


}
 */
