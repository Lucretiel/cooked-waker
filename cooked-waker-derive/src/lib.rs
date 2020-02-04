extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{self, parse_macro_input, parse_quote, Data, DeriveInput, Fields, Ident};

/// IntoWaker derive implementation.
///
/// This derive creates an IntoWaker implementation for any concrete type
/// that implements `Wake` and `Clone`. It does this be creating a static
/// `RawWakerVTable` associated with that type, with methods that forward to
/// the relevant trait methods, using stowaway to pack the waker object into
/// a pointer, then wrapping it all in a Waker.
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
                use cooked_waker::{Wake, WakeRef};
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
                        WakeRef::wake_by_ref(waker)
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum WakeTrait {
    Wake,
    WakeRef,
}

impl WakeTrait {
    #[inline]
    fn ident(self) -> syn::Path {
        match self {
            WakeTrait::Wake => parse_quote! {::cooked_waker::Wake},
            WakeTrait::WakeRef => parse_quote! {::cooked_waker::WakeRef},
        }
    }

    #[inline]
    fn method(self) -> Ident {
        match self {
            WakeTrait::Wake => parse_quote! {wake},
            WakeTrait::WakeRef => parse_quote! {wake_by_ref},
        }
    }

    #[inline]
    fn name(self) -> &'static str {
        match self {
            WakeTrait::Wake => "Wake",
            WakeTrait::WakeRef => "WakeRef",
        }
    }

    #[inline]
    fn by_reference(self) -> bool {
        match self {
            WakeTrait::Wake => false,
            WakeTrait::WakeRef => true,
        }
    }
}

fn derive_wake_like(spec: WakeTrait, stream: TokenStream) -> TokenStream {
    let input = parse_macro_input!(stream as DeriveInput);

    let trait_path = spec.ident();
    let method = spec.method();

    let type_name = input.ident;
    let mut generics = input.generics;
    let where_clause = generics.make_where_clause();

    match input.data {
        Data::Struct(s) => {
            let fields = match s.fields {
                Fields::Named(fields) => fields.named,
                Fields::Unnamed(fields) => fields.unnamed,
                Fields::Unit => panic!(
                    "`{name}` can only be derived on structs with a single `{name}` field",
                    name = spec.name()
                ),
            };

            if fields.len() != 1 {
                panic!(
                    "Can only derive `{name}` on structs with exactly 1 field",
                    name = spec.name()
                );
            }

            let field = fields.first().unwrap();
            let field_type = &field.ty;
            let field_name: syn::Member = field
                .ident
                .clone()
                .map(syn::Member::Named)
                .unwrap_or_else(|| parse_quote!(0));

            where_clause
                .predicates
                .push(parse_quote! {#field_type: #trait_path});

            let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

            let self_param = if spec.by_reference() {
                quote! {&self}
            } else {
                quote! {self}
            };

            let implementation = quote! {
                impl #impl_generics #trait_path for #type_name #ty_generics #where_clause {
                    #[inline]
                    fn #method(#self_param) {
                        self.#field_name.#method()
                    }
                }
            };

            implementation.into()
        }
        Data::Enum(_e) => unimplemented!("derive(Wake) for enums is still WIP"),
        Data::Union(..) => panic!("`Wake` can only be derived for struct or enum types"),
    }
}

/// Create a `Wake` implementation for a struct or enum. This implementation
/// is created recursively:
///
/// - If the type is a `struct`, it must have exactly one field, and that field
///   must implement `Wake`. A wake implementation is created that forwards to
///   this field.
/// - If the type is an `enum`, each of its variants must either be emtpy, or
///   contain a single field that implements `Wake`.
#[proc_macro_derive(Wake)]
pub fn wake_derive(stream: TokenStream) -> TokenStream {
    derive_wake_like(WakeTrait::Wake, stream)
}

/// Create a `WakeRef` implementation for a struct or enum. This implementation
/// is created recursively:
///
/// - If the type is a `struct`, it must have exactly one field, and that field
///   must implement `WakeRef`. A `WakeRef` implementation is created that
///   forwards to this field.
/// - If the type is an `enum`, each of its variants must either be emtpy, or
///   contain a single field that implements `Wake`. A `WakeRef` implementation
///   is created that forwards to that field, or no-ops if it's an empty state.
#[proc_macro_derive(WakeRef)]
pub fn wake_ref_derive(stream: TokenStream) -> TokenStream {
    derive_wake_like(WakeTrait::WakeRef, stream)
}
