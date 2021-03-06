// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Checks the trait declaration, makes the trait declaration module local, removes all method
//! default implementations and implements the trait for `&mut dyn Externalities`.

use crate::utils::{generate_crate_access, get_function_argument_types_without_ref};

use syn::{
	ItemTrait, TraitItemMethod, Result, TraitItem, Error, fold::{self, Fold}, spanned::Spanned,
	Visibility, Receiver, Type, Generics,
};

use proc_macro2::TokenStream;

use quote::quote;

/// Process the given trait definition, by checking that the definition is valid, fold it to the
/// essential definition and implement this essential definition for `dyn Externalities`.
pub fn process(trait_def: &ItemTrait, is_wasm_only: bool) -> Result<TokenStream> {
	let impl_trait = impl_trait_for_externalities(trait_def, is_wasm_only)?;
	let essential_trait_def = ToEssentialTraitDef::convert(trait_def.clone())?;

	Ok(
		quote! {
			#impl_trait

			#essential_trait_def
		}
	)
}

/// Converts the given trait definition into the essential trait definition without method
/// default implementations and visibility set to inherited.
struct ToEssentialTraitDef {
	/// All errors found while doing the conversion.
	errors: Vec<Error>,
}

impl ToEssentialTraitDef {
	/// Convert the given trait definition to the essential trait definition.
	fn convert(trait_def: ItemTrait) -> Result<ItemTrait> {
		let mut folder = ToEssentialTraitDef {
			errors: Vec::new(),
		};

		let res = folder.fold_item_trait(trait_def);

		if let Some(first_error) = folder.errors.pop() {
			Err(
				folder.errors.into_iter().fold(first_error, |mut o, n| {
					o.combine(n);
					o
				})
			)
		} else {
			Ok(res)
		}
	}

	fn push_error<S: Spanned>(&mut self, span: &S, msg: &str) {
		self.errors.push(Error::new(span.span(), msg));
	}

	fn error_on_generic_parameters(&mut self, generics: &Generics) {
		if let Some(param) = generics.params.first() {
			self.push_error(param, "Generic parameters not supported.");
		}
	}
}

impl Fold for ToEssentialTraitDef {
	fn fold_trait_item_method(&mut self, mut method: TraitItemMethod) -> TraitItemMethod {
		if method.default.take().is_none() {
			self.push_error(&method, "Methods need to have an implementation.");
		}

		let arg_types = get_function_argument_types_without_ref(&method.sig);
		arg_types.filter_map(|ty|
			match *ty {
				Type::ImplTrait(impl_trait) => Some(impl_trait),
				_ => None
			}
		).for_each(|invalid| self.push_error(&invalid, "`impl Trait` syntax not supported."));

		self.error_on_generic_parameters(&method.sig.generics);

		fold::fold_trait_item_method(self, method)
	}

	fn fold_item_trait(&mut self, mut trait_def: ItemTrait) -> ItemTrait {
		self.error_on_generic_parameters(&trait_def.generics);

		trait_def.vis = Visibility::Inherited;
		fold::fold_item_trait(self, trait_def)
	}

	fn fold_receiver(&mut self, receiver: Receiver) -> Receiver {
		if receiver.reference.is_none() {
			self.push_error(&receiver, "Taking `Self` by value is not allowed.");
		}

		fold::fold_receiver(self, receiver)
	}
}

/// Implements the given trait definition for `dyn Externalities`.
fn impl_trait_for_externalities(trait_def: &ItemTrait, is_wasm_only: bool) -> Result<TokenStream> {
	let trait_ = &trait_def.ident;
	let crate_ = generate_crate_access();
	let methods = trait_def
		.items
		.iter()
		.filter_map(|i| match i {
			TraitItem::Method(ref method) => Some(method),
			_ => None,
		});

	let impl_type = if is_wasm_only {
		quote!( &mut dyn #crate_::wasm_interface::FunctionContext )
	} else {
		quote!( &mut dyn #crate_::Externalities )
	};

	Ok(
		quote! {
			#[cfg(feature = "std")]
			impl #trait_ for #impl_type {
				#( #methods )*
			}
		}
	)
}
