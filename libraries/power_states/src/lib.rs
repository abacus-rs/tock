use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    DeriveInput, Field, FieldMutability, Fields, FieldsUnnamed, Ident, ItemStruct, PathArguments,
    Type, Variant, Visibility,
};

use std::collections::hash_set::HashSet;

struct State {
    ident: syn::Ident,
    shortname: syn::Ident,
    substates: Punctuated<syn::Ident, syn::Token![,]>,
    transitions: Punctuated<State, syn::Token![,]>,
}

impl State {
    fn generate_state(
        &self,
        register_name: &Ident,
        store_name: &Ident,
    ) -> proc_macro2::TokenStream {
        let mut result = proc_macro2::TokenStream::new();
        let state_ident = self.ident.clone();

        if self.substates.is_empty() {
            result.extend(quote! {
                pub struct #state_ident;
            });
        } else {
            let generic_params = self.substates.iter().enumerate().map(|(index, _)| {
                let entry = format!("T{}", index);
                let generic = syn::Ident::new(&entry, Span::call_site());

                quote! {
                    #generic: SubState
                }
            });

            let fields = self.substates.iter().enumerate().map(|(index, _)| {
                let field_name = format!("associated_{}", index);
                let generic_name = format!("T{}", index);

                let generic = syn::Ident::new(&generic_name, Span::call_site());
                let field = syn::Ident::new(&field_name, Span::call_site());

                quote! {
                    #field: PhantomData<#generic>
                }
            });

            result.extend(quote! {
                pub struct #state_ident<#(#generic_params),*> {
                    #(#fields),*
                }
            });
        }

        result.extend(quote! {
            impl State for #state_ident {
                type Reg = #register_name<#state_ident>;
                type StateEnum = #store_name;
            }
            impl Reg for #register_name<#state_ident> {
                type StateEnum = #store_name;
            }
        });

        // impl From<Nrf5xTempRegister<Off>> for Nrf5xTemperatureStore {
        //     fn from(reg: Nrf5xTempRegister<Off>) -> Self {
        //         Nrf5xTemperatureStore::Off(reg)
        //     }
        // }
        result.extend(quote! {
            impl From<#register_name<#state_ident>> for #store_name {
                fn from(reg: #register_name<#state_ident>) -> Self {
                    #store_name::#state_ident(reg)
                }
            }
        });

        result
    }

    fn generate_state_transitions(&self) -> TokenStream {
        unimplemented!()
    }
}

struct SubStates {
    ident: syn::Ident,
}

impl Parse for State {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let state: Ident = input.parse()?;

        let substates = if input.peek(syn::token::Paren) {
            let content;
            let _: syn::token::Paren = syn::parenthesized!(content in input);
            let substates: Punctuated<Ident, syn::Token![,]> =
                content.parse_terminated(syn::Ident::parse, syn::Token![,])?;
            Some(substates)
        } else {
            None
        }
        .unwrap_or_else(|| Punctuated::new());

        let (transitions, shortname) = if input.peek(syn::Token![=>]) {
            input.parse::<syn::Token![=>]>()?;
            let content;
            let _: syn::token::Bracket = bracketed!(content in input);
            let transitions: Punctuated<State, syn::Token![,]> = content
                .parse_terminated(State::parse, syn::Token![,])
                .expect("0");

            let content;

            if input.peek(syn::token::Brace) {
                let _: syn::token::Brace = syn::braced!(content in input);
                let shortname = content.parse().map_or_else(|_| state.clone(), |x| x);
                (transitions, shortname)
            } else {
                (transitions, state.clone())
            }
        } else {
            (Punctuated::new(), state.clone())
        };

        Ok(State {
            ident: state,
            shortname,
            substates,
            transitions,
        })
    }
}

mod custom_keywords {
    syn::custom_keyword!(peripheral_name);
    syn::custom_keyword!(registers);
    syn::custom_keyword!(states);
}

#[derive(Clone)]
enum RegisterType {
    ReadOnly,
    WriteOnly,
    ReadWrite,
    StateChange,
}

impl RegisterType {
    fn to_ident(&self) -> Ident {
        match self {
            RegisterType::ReadOnly => format_ident!("ReadOnly"),
            RegisterType::WriteOnly => format_ident!("WriteOnly"),
            RegisterType::ReadWrite => format_ident!("ReadWrite"),
            RegisterType::StateChange => format_ident!("StateChange"),
        }
    }
}

impl Parse for RegisterType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        match ident.to_string().as_str() {
            "ReadOnly" => Ok(RegisterType::ReadOnly),
            "WriteOnly" => Ok(RegisterType::WriteOnly),
            "ReadWrite" => Ok(RegisterType::ReadWrite),
            "StateChange" => Ok(RegisterType::StateChange),
            x => {
                eprintln!("{:?}", x);
                Err(syn::Error::new(ident.span(), "Unknown register type"))
            }
        }
    }
}

struct Register {
    name: Ident,
    type_name: Ident,
    valid_states: Punctuated<State, syn::Token![,]>,
    register_shortname: syn::GenericArgument,
    register_type: RegisterType,
    register_bitwidth: Ident,
}

impl Register {}
struct RegisterAttributes {
    states: Punctuated<State, syn::Token![,]>,
    register_type: RegisterType,
    type_name: Ident,
}

impl Parse for RegisterAttributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        let valid_states = bracketed!(content in input);
        let states: Punctuated<State, syn::Token![,]> = content
            .parse_terminated(State::parse, syn::Token![,])
            .expect("1");

        input.parse::<syn::Token![,]>()?;
        let register_type: RegisterType = input.parse().expect("Invalid provided reg type.");

        input.parse::<syn::Token![,]>()?;
        let type_name: Ident = input.parse().expect("3");

        Ok(RegisterAttributes {
            states,
            register_type,
            type_name,
        })
    }
}

struct MacroInput {
    peripheral_name: String,
    states: Punctuated<State, syn::Token![,]>,
}

impl MacroInput {
    fn generate_state_store(
        &self,
        register_name: &Ident,
        store_name: &Ident,
    ) -> proc_macro2::TokenStream {
        let store_variants: Vec<Variant> = self
            .states
            .iter()
            .map(|state| {
                let substate_iter = state.substates.iter().map(|substate| {
                    quote! {
                        #substate
                    }
                });

                let substate_tokens = if state.substates.is_empty() {
                    quote! {}
                } else {
                    quote! {<#(#substate_iter),*>}
                };

                let state_ident = state.ident.clone();
                Variant {
                    attrs: Vec::new(),
                    ident: state.shortname.clone(),
                    discriminant: None,
                    fields: Fields::Unnamed(FieldsUnnamed {
                        paren_token: syn::token::Paren(Span::call_site()),
                        unnamed: Punctuated::from_iter(vec![Field {
                            attrs: Vec::new(),
                            vis: Visibility::Inherited,
                            mutability: FieldMutability::None,
                            ident: None,
                            colon_token: None,
                            ty: Type::Verbatim(quote! {
                                #register_name<#state_ident #substate_tokens>
                            }),
                        }]),
                    }),
                }
            })
            .collect();

        // Expands to:
        // pub enum Nrf5xTempStore {
        //     Off(Nrf5xTempRegister<state_ident>),
        //     Reading(Nrf5xTempRegister<state_ident>),
        // }
        quote! {
            pub enum #store_name{
                #(#store_variants),*
            }

            impl Store for #store_name {}
            impl StateEnum for #store_name {}
        }
    }

    fn generate_states(
        &self,
        register_name: &Ident,
        store_name: &Ident,
    ) -> proc_macro2::TokenStream {
        let mut created_states: HashSet<syn::Ident> = HashSet::new();

        let mut output = proc_macro2::TokenStream::new();

        for state in &self.states {
            if created_states.contains(&state.ident) {
                continue;
            }

            created_states.insert(state.ident.clone());

            output.extend(state.generate_state(register_name, store_name));
        }

        output
    }

    fn generate_disjunctive_states(&self) -> proc_macro2::TokenStream {
        // get unique state shortnames
        let mut unique_states = HashSet::new();
        for state in &self.states {
            unique_states.insert(state.shortname.clone());
        }

        // for each unique state shortname, output trait of the form
        // {ShortName1}State{ShortName2}State...{ShortNameN}State
        // such that this accounts for all combinations of all states.
        let mut output = proc_macro2::TokenStream::new();
        for root_state in &unique_states {
            let state_str = format!("{}State", root_state);
            for comb_state in &unique_states {
                if comb_state == root_state {
                    continue;
                }
                let comb_state_str = format!("{}State", comb_state);
                let state_str = format!("{}{}", state_str, comb_state_str);
                let state_trait = format_ident!("{}", state_str);
                output.extend(quote! {
                    trait #state_trait: State {}
                });
            }
        }

        output
    }
}

fn add_imports() -> proc_macro2::TokenStream {
    quote!(
        use kernel::power_manager::{
            Peripheral, State, SubState, StateEnum, Reg, Store, PowerManager, PowerError,
        };
        use core::marker::PhantomData;
        use core::mem::transmute;
        use core::ops::Deref;
        use kernel::utilities::registers::{FieldValue, UIntLike, RegisterLongName};
    )
}

impl Parse for MacroInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _: custom_keywords::peripheral_name = input.parse()?;
        let _: syn::Token![=] = input.parse()?;
        let peripheral_name: syn::LitStr = input.parse()?;
        let _: syn::Token![,] = input.parse()?;

        let _: custom_keywords::states = input.parse()?;
        let _: syn::Token![=] = input.parse()?;
        let states_content;
        let _: syn::token::Bracket = bracketed!(states_content in input);
        let states: Punctuated<State, syn::Token![,]> =
            states_content.parse_terminated(State::parse, syn::Token![,])?;

        Ok(MacroInput {
            peripheral_name: peripheral_name.value(),
            states,
        })
    }
}

#[proc_macro_attribute]
pub fn process_register(attr: TokenStream, item: TokenStream) -> TokenStream {
    let original = item.clone();

    let parsed_input = parse_macro_input!(item as ItemStruct);
    let parsed_attr = parse_macro_input!(attr as MacroInput);

    original.into()
}

#[proc_macro_attribute]
pub fn process_register_block(attr: TokenStream, item: TokenStream) -> TokenStream {
    let parsed_input = parse_macro_input!(attr as MacroInput);
    // form reg and store type names from given peripheral name
    let register = format_ident!("{}Registers", parsed_input.peripheral_name);
    let store = format_ident!("{}Store", parsed_input.peripheral_name);
    let storable = format_ident!("Storable{}States", parsed_input.peripheral_name);
    let peripheral = format_ident!("{}Peripheral", parsed_input.peripheral_name);
    let register_block_str = format!("{}{}", parsed_input.peripheral_name, "RegisterBlock");
    let register_block = format_ident!("{}", register_block_str);

    let mut result = add_imports();

    let block = quote! {
        pub struct #register<S: kernel::power_manager::State>  {
            reg: StaticRef<#register_block<S>>,
        }

        impl <S: State> Deref for #register<S> {
            type Target = #register_block<S>;
            fn deref(&self) -> &#register_block<S> {
                self.reg.deref()
            }
        }
    };

    result.extend(block);

    // Generate states
    result.extend(parsed_input.generate_states(&register, &store));

    // Generate store enum
    result.extend(parsed_input.generate_state_store(&register, &store));

    result.extend(quote! {
        pub struct #peripheral {}

        // use kernel::power_manager::{Peripheral, StateEnum, Store};
        impl Peripheral for #peripheral {
            type StateEnum = #store;
            type Store = #store;
        }
    });

    result.extend(parsed_input.generate_disjunctive_states());

    for state in &parsed_input.states {
        let state_ident = state.ident.clone();
        result.extend(quote! {
            impl TryFrom<#store> for #register<#state_ident> {
                type Error = (kernel::ErrorCode, #store);
                fn try_from(store: #store) -> Result<Self, Self::Error> {
                    match store {
                        #store::#state_ident(reg) => Ok(reg),
                        _ => Err((kernel::ErrorCode::INVAL, store)),
                    }
                }
            }
        });
    }

    let ast: DeriveInput = syn::parse(item).unwrap();

    let data = match &ast.data {
        syn::Data::Struct(data) => data,
        _ => panic!("Unsupported data type"),
    };

    let mut reg_vec: Vec<Register> = vec![];
    // parse into the registers
    for register in &data.fields {
        let reg_attr = register.attrs.iter().find_map(|attr| {
            // for each attribute in field attrs, leave doc macro comments
            // and remove RegAttributes.
            if attr.path().is_ident("RegAttributes") {
                return Some(attr.parse_args::<RegisterAttributes>().unwrap());
            }
            None
        });

        if reg_attr.is_none() {
            continue;
        }

        let reg_attr = reg_attr.unwrap();

        if let Type::Path(type_path) = &register.ty {
            if let Some(segment) = type_path.path.segments.last() {
                let type_ident = &segment.ident; // Extract `WriteOnly`

                // Check for generics
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    let generic_args = &args.args;
                    if generic_args.len() != 2 {
                        panic!("Expected 2 generic arguments");
                    }
                    let register_shortname = generic_args[1].clone();

                    eprintln!("here we are");
                    let register_bitwidth =
                        if let syn::GenericArgument::Type(Type::Path(type_path)) = &generic_args[0]
                        {
                            let generic_ident = &type_path.path;
                            generic_ident.segments.first().unwrap().ident.clone()
                        } else {
                            panic!("unreachable");
                        };

                    reg_vec.push(Register {
                        name: register.ident.clone().unwrap(),
                        type_name: reg_attr.type_name.clone(),
                        valid_states: reg_attr.states,
                        register_shortname,
                        register_type: reg_attr.register_type,
                        register_bitwidth,
                    });
                }
            }
        }
    }

    let struct_name = format!("{}RegisterBlock", parsed_input.peripheral_name);
    let struct_name_ident = format_ident!("{}", struct_name);

    let field_details = data.fields.iter().map(|field| {
        let field_type = field.ty.clone();
        let field_name = field.ident.clone().unwrap();

        let mut requires_gen = field.attrs.iter().any(|attr| {
            // for each attribute in field attrs, leave all macros but
            // RegAttributes.
            if attr.path().is_ident("RegAttributes") {
                return true;
            }
            false
        });

        let field_attr = field.attrs.iter().map(|attr| {
            // for each attribute in field attrs, leave all macros but
            // RegAttributes.
            if attr.path().is_ident("RegAttributes") {
                return quote!{};
            } else {
                return quote! {#attr};
            }
        });


    // DO ATTRIBUTE EXPANSION HERE

        if requires_gen {
            let reg_attr = field.attrs.iter().find_map(|attr| {
                // for each attribute in field attrs, leave doc macro comments
                // and remove RegAttributes.
                if attr.path().is_ident("RegAttributes") {
                    return Some(attr.parse_args::<RegisterAttributes>().unwrap());
                }
                None
            }).expect("reg attribute error");
            if let Type::Path(type_path) = field_type.clone() {
                if let Some(segment) = type_path.path.segments.last() {
                    let type_ident = &segment.ident; // Extract `WriteOnly`

                    // Check for generics
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        let generic_args = &args.args;
                        if generic_args.len() != 2 {
                            panic!("Expected 2 generic arguments");
                        }
                        let register_shortname = generic_args[1].clone();

                        let register_bitwidth =
                        if let syn::GenericArgument::Type(Type::Path(type_path)) = &generic_args[0]
                        {
                            let generic_ident = &type_path.path;
                            generic_ident.segments.first().unwrap().ident.clone()
                        } else {
                            panic!("unreachable");
                        };

                    reg_vec.push(Register {
                        name: field_name.clone(),
                        type_name: reg_attr.type_name.clone(),
                        valid_states: reg_attr.states,
                        register_shortname: register_shortname.clone(),
                        register_type: reg_attr.register_type.clone(),
                        register_bitwidth: register_bitwidth.clone(),
                    });

                    let internal_naming = reg_attr.type_name.clone();
                    let reg_type = reg_attr.register_type.clone().to_ident();
                    quote! {
                        #(#field_attr)*
                        pub #field_name: #reg_type<#register_bitwidth, #register_shortname, #internal_naming, S>
                    }
                } else {
                    panic!("unreachable a")
                }
            } else {
                panic!("unreachable b");
            }
        } else {
            panic!("unreachable c");
        }
    } else {
        quote! {
            #(#field_attr)*
            #field_name: #field_type
        }
    }
});

    let struct_output = quote! {
        pub struct #struct_name_ident<S: State> {
            #(#field_details),*
        }
    };

    for reg in reg_vec {
        //result.extend(reg.)
    }

    result.extend(struct_output);

    result.into()
}

/*

THINGS TO GENERATE:

StateStore
States
SubStates
TryFrom
From

*/
