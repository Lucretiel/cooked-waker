extern crate proc_macro;

use proc_macro as pm;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{self, parse_macro_input, parse_quote, Data, DeriveInput, Fields};

/// `IntoWaker` derive implementation.
///
/// This derive creates an `IntoWaker` implementation for any concrete type.
/// It does this be creating a static `RawWakerVTable` associated with that
/// type, with methods that forward to the relevant trait methods, using
/// stowaway to pack the waker object into a pointer, then wrapping it all in
/// a Waker.
///
/// Note that `IntoWaker` requires `Wake + Clone + Send + Sync + 'static`.
#[proc_macro_derive(IntoWaker)]
pub fn into_waker_derive(stream: pm::TokenStream) -> pm::TokenStream {
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

                let stowed = Stowaway::new(self);

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

                let raw_waker = RawWaker::new(Stowaway::into_raw(stowed), &VTABLE);
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
    fn trait_path(self) -> syn::Path {
        match self {
            WakeTrait::Wake => parse_quote! {::cooked_waker::Wake},
            WakeTrait::WakeRef => parse_quote! {::cooked_waker::WakeRef},
        }
    }

    #[inline]
    fn method(self) -> syn::Ident {
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

    /// Change a token stream like `self` or `self.value` to `&self` or
    /// `&self.value` if this is WakeRef
    #[inline]
    fn apply_reference(self, input: TokenStream) -> TokenStream {
        match self {
            WakeTrait::Wake => input,
            WakeTrait::WakeRef => quote! {& #input},
        }
    }
}

fn derive_wake_like(spec: WakeTrait, stream: pm::TokenStream) -> pm::TokenStream {
    let input = parse_macro_input!(stream as DeriveInput);

    let trait_path = spec.trait_path();
    let method = spec.method();

    let type_name = input.ident;
    let mut generics = input.generics;
    let where_clause = generics.make_where_clause();

    match input.data {
        Data::Struct(s) => {
            // Normalize named and unnamed struct fields.
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

            // field_name is either `name` or `0`; it allows for `self.name`
            // or `self.0`.
            let field_name: syn::Member = field
                .ident
                .clone()
                .map(syn::Member::Named)
                .unwrap_or_else(|| parse_quote!(0));

            // Add "where FieldType: Wake"
            where_clause
                .predicates
                .push(parse_quote! {#field_type: #trait_path});

            let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

            // The self parameter for the function signature: self or &self
            let self_param = spec.apply_reference(quote! {self});

            // The getter for the self field: self.0 or self.field_name
            let field_invocation = spec.apply_reference(quote! {self.#field_name});

            let implementation = quote! {
                impl #impl_generics #trait_path for #type_name #ty_generics #where_clause {
                    #[inline]
                    fn #method(#self_param) {
                        #trait_path::#method(#field_invocation)
                    }
                }
            };

            implementation.into()
        }
        Data::Enum(..) => unimplemented!("derive(Wake) for enums is still WIP"),
        Data::Union(..) => panic!("`Wake` can only be derived for struct or enum types"),
    }
}

/// Create a `Wake` implementation for a `struct` that forwards to the
/// `struct`'s field. The `struct` must have exactly one field, and that
/// field must implement `Wake`.
///
/// In the future this derive will also support `enum`.
#[proc_macro_derive(Wake)]
pub fn wake_derive(stream: pm::TokenStream) -> pm::TokenStream {
    derive_wake_like(WakeTrait::Wake, stream)
}

/// Create a `WakeRef` implementation for a `struct` that forwards to the
/// `struct`'s field. The `struct` must have exactly one field, and that
/// field must implement `WakeRef`.
///
/// In the future this derive will also support `enum`.
#[proc_macro_derive(WakeRef)]
pub fn wake_ref_derive(stream: pm::TokenStream) -> pm::TokenStream {
    derive_wake_like(WakeTrait::WakeRef, stream)
}
