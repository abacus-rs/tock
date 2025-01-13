use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    DeriveInput, Field, FieldMutability, Fields, FieldsUnnamed, Ident, ItemStruct, Type, Variant,
    Visibility,
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
    let register = format_ident!("{}Register", parsed_input.peripheral_name);
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

    /*
    for state in &parsed_input.states {
        let state_trait = format_ident!("{}State", state.shortname.clone());
        result.extend(quote! {
            #[allow(dead_code)]
            trait #state_trait {}
        });
    }

    if !parsed_input.states.is_empty() {
        let state1 = parsed_input.states.get(0).unwrap();
        for state2 in parsed_input.states.iter() {
            if state1.shortname == state2.shortname {
                continue;
            }
            let state_trait = format_ident!("{}State{}State", state1.shortname, state2.shortname);
            result.extend(quote! {
                #[allow(dead_code)]
                trait #state_trait : State {}
            });
            for variant in parsed_input.states.iter() {
                let variant_ident = variant.ident.clone();
                result.extend(quote! {
                    impl #state_trait for #variant_ident {}
                });
            }

            result.extend(quote! {
                impl<S: #state_trait> ReadWriteRegister<u32, EventDataReady::Register, S> {
                    fn write(&self, val: FieldValue<u32, EventDataReady::Register>) {
                        self.reg.write(val);
                    }
                }

                impl<S: #state_trait> ReadWriteRegister<u32, Intenset::Register, S> {
                    fn write(&self, val: FieldValue<u32, Intenset::Register>) {
                        self.reg.write(val);
                    }
                }

                impl<S: #state_trait> ReadWriteRegister<u32, Intenclr::Register, S> {
                    fn write(&self, val: FieldValue<u32, Intenclr::Register>) {
                        self.reg.write(val);
                    }
                }
            });
        }
    }
    */
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

    /*
        let mut unique_states = HashSet::new();

        for state in &parsed_input.states {
            if unique_states.contains(&state.ident) {
                continue;
            }

            unique_states.insert(state.ident.clone());

            let state_ident = state.ident.clone();
            for transition_states in &state.transitions {
                let transition = transition_states.ident.clone();
                let step_transition = format_ident!("Step{}", transition);
                let into_fn = format_ident!("into_{}", transition.to_string().to_lowercase());
                result.extend(quote! {
                    impl #step_transition for #register<#state_ident> {
                        fn #into_fn<PM: PowerManager<#peripheral>>(
                            self,
                            _pm: &PM,
                        ) -> Result<#register<#transition>, PowerError<Self>> {
                            // TOOD: accept reg write/clear as input
                            // self.task_stop.reg.write(TaskStop::ENABLE::SET);

                            unsafe {
                                Ok(transmute::<
                                    #register<#state_ident>,
                                    #register<#transition>,
                                >(self))
                            }
                        }
                    }
                });
            }
        }

        for state in &parsed_input.states {
        }
    */

    // We want to create:
    /*
       impl StateChangeRegister<u32, Task::Register, S, NAME>{
           fn write(&self, val: FieldValue<u32, Task::Register>) {
               self.reg.write(val);
           }
       }
    */

    /*
    let register_types = data.fields.iter().map(|field| {
        let field_type = &field.ty;

        let type_string = quote! { #field_type }.to_string();

        if type_string.contains("StateChangeRegister") {
            quote! {
                impl #field_type {

                }
            }
        } else if type_string.contains("ReadWriteRegister") {
            quote! {
                impl #field_type {

                }
            }
        } else if type_string.contains("ReadOnlyRegister") {
            quote! {
                impl #field_type {

                }
            }
        } else if type_string.contains("WriteOnlyRegister") {
            quote! {
                impl #field_type {

                }
            }
        } else {
            panic!("Unknown register type");
        }
    });
    */
    /*
        result.extend(quote! {
            #[allow(dead_code)]
            struct WriteRegister<T: UIntLike, R: RegisterLongName, S: State> {
                reg: WriteOnly<T, R>,
                associated_state: PhantomData<S>,
            }

            struct ReadRegister<T: UIntLike, R: RegisterLongName, S: State> {
                reg: ReadOnly<T, R>,
                associated_state: PhantomData<S>,
            }

            struct ReadWriteRegister<T: UIntLike, R: RegisterLongName, S: State> {
                reg: ReadWrite<T, R>,
                associated_state: PhantomData<S>,
            }

            struct StateChangeRegister<T: UIntLike, R: RegisterLongName, S: State> {
                reg: WriteOnly<T, R>,
                associated_state: PhantomData<S>,
            }


            impl<S: State> Deref for #register<S> {
                type Target = RegisterBlock<S>;
                fn deref(&self) -> &RegisterBlock<S> {
                    self.reg.deref()
                }
            }
        });
    */

    let ast: DeriveInput = syn::parse(item).unwrap();

    let data = match &ast.data {
        syn::Data::Struct(data) => data,
        _ => panic!("Unsupported data type"),
    };

    let struct_name = format!("{}RegisterBlock", parsed_input.peripheral_name);
    let struct_name_ident = format_ident!("{}", struct_name);

    let field_details = data.fields.iter().map(|field| {
        let field_type = &field.ty;
        let field_name = &field.ident;

        // DO ATTRIBUTE EXPANSION HERE

        quote! {
            #field_name: #field_type
        }
    });

    let struct_output = quote! {
        pub struct #struct_name_ident<S: State> {
            temp_associate: PhantomData<S>,
            #(#field_details),*
        }
    };

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
