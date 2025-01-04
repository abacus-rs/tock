use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{
    bracketed, parse::{Parse, ParseStream}, parse_macro_input, punctuated::Punctuated, Field, FieldMutability, Fields, FieldsUnnamed, Ident, Type, Variant, Visibility
};


struct State {
    ident: syn::Ident,
    transitions: Punctuated<Ident, syn::Token![,]>
}

impl Parse for State {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        let ident = input.parse()?;
        let _ : syn::Token![=>] = input.parse()?;
        let _ : syn::token::Bracket = bracketed!(content in input);
        let transitions_to: Punctuated<Ident, syn::Token![,]> = content.parse_terminated(syn::Ident::parse, syn::Token![,])?;
        Ok(State{
            ident,
            transitions: transitions_to
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
    registers: syn::Ident,
    states: Punctuated<State, syn::Token![,]>
}


impl Parse for MacroInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _ : custom_keywords::peripheral_name = input.parse()?;
        let _ : syn::Token![=] = input.parse()?;
        let peripheral_name: syn::LitStr = input.parse()?;
        let _ : syn::Token![,] = input.parse()?;

        let _ : custom_keywords::registers = input.parse()?;
        let _ : syn::Token![=] = input.parse()?;
        let registers_ident: syn::Ident = input.parse()?;
        let _ : syn::Token![,] = input.parse()?;

        let _ : custom_keywords::states = input.parse()?;
        let _ : syn::Token![=] = input.parse()?;
        let states_content;
        let _ : syn::token::Bracket = bracketed!(states_content in input);
        let states: Punctuated<State, syn::Token![,]> = states_content.parse_terminated(State::parse, syn::Token![,])?;

        Ok(MacroInput{
            peripheral_name: peripheral_name.value(),
            registers: registers_ident,
            states,
        })
    }
}

#[proc_macro]
pub fn states(input: TokenStream) -> TokenStream {
    let parsed_input = parse_macro_input!(input as MacroInput);

    // form reg and store type names from given peripheral name
    let register = format_ident!("{}Register", parsed_input.peripheral_name);
    let store = format_ident!("{}Store", parsed_input.peripheral_name);
    let storable = format_ident!("Storable{}States", parsed_input.peripheral_name);
    let peripheral = format_ident!("{}Peripheral", parsed_input.peripheral_name);
    let register_block = parsed_input.registers;

    let mut result = quote! {
        pub struct #register<S: kernel::power_manager::State>  {
            reg: StaticRef<#register_block<S>>,
        }
    };

    for state in &parsed_input.states {
        let state_ident = state.ident.clone();
        result.extend(quote!{
            pub struct #state_ident;
        });
    }

    let store_variants: Vec<Variant> = parsed_input.states.iter().map(|x| {
        let state_ident = x.ident.clone();
        Variant{
            attrs: Vec::new(),
            ident: state_ident.clone(),
            discriminant: None,
            fields: Fields::Unnamed(
                FieldsUnnamed {
                    paren_token: syn::token::Paren(Span::call_site()),
                    unnamed: Punctuated::from_iter(vec![Field {
                    attrs: Vec::new(),
                    vis: Visibility::Inherited,
                    mutability: FieldMutability::None,
                    ident: None,
                    colon_token: None,
                    ty: Type::Verbatim(quote! {
                        #register<#state_ident>
                    }),
                }]),
            })
        }
    }).collect();

    // Expands to:
    // pub enum Nrf5xTempStore {
    //     Off(Nrf5xTempRegister<state_ident>),
    //     Reading(Nrf5xTempRegister<state_ident>),
    // }
    result.extend(quote! {
        pub enum #store {
            #(#store_variants),*
        }

        impl Store for #store {}
        impl StateEnum for #store {}
    });

    result.extend(quote! {
        pub struct #peripheral {}
        
        // use kernel::power_manager::{Peripheral, StateEnum, Store};
        impl Peripheral for #peripheral {
            type StateEnum = #store;
            type Store = #store;
        }
    });

    for state in &parsed_input.states {
        let state_ident = state.ident.clone();
        result.extend(quote!{
            impl State for #state_ident {
                type Reg = #register<#state_ident>;
                type StateEnum = #store;
            }
            impl Reg for #register<#state_ident> {
                type StateEnum = #store;
            }
        });
    }

    for state in &parsed_input.states {
        let state_trait = format_ident!("{}State", state.ident.clone());
        result.extend(quote!{
            #[allow(dead_code)]
            trait #state_trait {} 
        });
    }

    if !parsed_input.states.is_empty() {
        let state1 = parsed_input.states.get(0).unwrap();
        for state2 in parsed_input.states.iter() {
            if state1.ident == state2.ident {
                continue;
            }
            let state_trait = format_ident!("{}State{}State", state1.ident, state2.ident);
            result.extend(quote!{
                #[allow(dead_code)]
                trait #state_trait : State {} 
            });
            for variant in parsed_input.states.iter() {
                let variant_ident = variant.ident.clone();
                result.extend(quote!{
                    impl #state_trait for #variant_ident {} 
                });
            }

            result.extend(quote!{
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

    for state in &parsed_input.states {
        let state_ident = state.ident.clone();
        result.extend(quote!{
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

    for state in &parsed_input.states {
        let state_ident = state.ident.clone();
        let step_trait = format_ident!("Step{}", state_ident);
        let into_fn = format_ident!("into_{}", state_ident.to_string().to_lowercase());
        result.extend(quote! {
            trait #step_trait: Sized {
                fn #into_fn<PM: PowerManager<#peripheral>>(
                    self,
                    _pm: &PM,
                ) -> Result<#register<#state_ident>, PowerError<Self>>;
            }
        });
    }

    for state in &parsed_input.states {
        let state_ident = state.ident.clone(); 
        for transition in &state.transitions {
            let step_transition = format_ident!("Step{}", transition);
            let into_fn = format_ident!("into_{}", transition.to_string().to_lowercase());
            result.extend(quote!{
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

    // impl From<Nrf5xTempRegister<Off>> for Nrf5xTemperatureStore {
    //     fn from(reg: Nrf5xTempRegister<Off>) -> Self {
    //         Nrf5xTemperatureStore::Off(reg)
    //     }
    // }
    for state in &parsed_input.states {
        let state_ident = state.ident.clone();
        result.extend(quote!{
            impl From<#register<#state_ident>> for #store {
                fn from(reg: #register<#state_ident>) -> Self {
                    #store::#state_ident(reg)
                }
            }
        });
    }


    result.extend(quote!{
        pub enum #storable {
            #(#store_variants),*
        }
    });

    result.extend(quote!{
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


        impl<S: State> Deref for Nrf5xTempRegister<S> {
            type Target = RegisterBlock<S>;
            fn deref(&self) -> &RegisterBlock<S> {
                self.reg.deref()
            }
        }
    });

    result.into()
}


// // ORIGINAL attribute proc macro 
//
// #[derive(FromMeta)]
// struct Args {
//     peripheral: String,
//     registers: Ident,
// }
//
// #[proc_macro_attribute]
// pub fn states_old(args: TokenStream, item: TokenStream) -> TokenStream {
//     let attr_args = match NestedMeta::parse_meta_list(args.into()) {
//         Ok(v) => v,
//         Err(e) => { return TokenStream::from(Error::from(e).write_errors()); }
//     };
//
//     let args = match Args::from_list(&attr_args) {
//         Ok(v) => v,
//         Err(e) => { return TokenStream::from(e.write_errors()); }
//     };
//
//     // form reg and store type names from given peripheral name
//     let register = format_ident!("{}Register", args.peripheral);
//     let store = format_ident!("{}Store", args.peripheral);
//     let peripheral = format_ident!("{}Peripheral", args.peripheral);
//     let register_block = args.registers;
//
//     let mut state_enum = parse_macro_input!(item as ItemEnum);
//
//     // let mut states = Vec::new();
//     // state_enum.variants.iter_mut().for_each(|variant| {
//     //
//     //     let state_ident = variant.ident.clone();
//     //     variant.fields = Fields::Unnamed(FieldsUnnamed {
//     //         paren_token: syn::token::Paren(Span::call_site()),
//     //         unnamed: Punctuated::from_iter(vec![Field {
//     //             attrs: Vec::new(),
//     //             vis: Visibility::Inherited,
//     //             mutability: FieldMutability::None,
//     //             ident: None,
//     //             colon_token: None,
//     //             ty: Type::Verbatim(quote! {
//     //                 #register<#state_ident>
//     //             }),
//     //         }]),
//     //     });
//     //
//     //     states.push(variant.ident.clone());
//     // });
//     //
//     // let store_variants = state_enum.variants.clone();
//     //
//     // let mut result = quote! {
//     //     #state_enum
//     //     enum #store {
//     //         #store_variants
//     //     }
//     // };
//     //
//     // for state in states {
//     //     result.extend(quote! {
//     //         impl State for #state {
//     //             type Reg = #register<#state>;
//     //             type StateEnum = #store;
//     //         }
//     //         impl Reg for #register<#state> {
//     //             type StateEnum = #store;
//     //         }
//     //     })
//     // }
//     // result.into()
//     
//     let mut result = quote! {
//         pub struct #register<S: kernel::power_manager::State>  {
//             reg: StaticRef<#register_block<S>>,
//         }
//     };
//
//     let mut store_enum = state_enum.clone();
//     store_enum.ident = store.clone();
//     store_enum.variants.iter_mut().for_each(|variant| {
//         variant.fields = Fields::Unnamed(FieldsUnnamed {
//             paren_token: syn::token::Paren(Span::call_site()),
//             unnamed: Punctuated::from_iter(vec![Field {
//                 attrs: Vec::new(),
//                 vis: Visibility::Inherited,
//                 mutability: FieldMutability::None,
//                 ident: None,
//                 colon_token: None,
//                 ty: Type::Verbatim(quote! {
//                     #register<#variant>
//                 }),
//             }]),
//         });
//     });
//
//     result.extend(quote!{
//         #store_enum
//     });
//
//     result.extend(quote! {
//         pub struct #peripheral {}
//         
//         // use kernel::power_manager::{Peripheral, StateEnum, Store};
//         impl kernel::power_manager::Peripheral for #peripheral {
//             type StateEnum = #store;
//             type Store = #store;
//         }
//     });
//
//     for state in state_enum.variants.iter() {
//         result.extend(quote!{
//             pub struct #state;
//
//             impl State for #state {
//                 type Reg = #register<#state>;
//                 type StateEnum = #store;
//             }
//             impl Reg for #register<#state> {
//                 type StateEnum = #store;
//             }
//
//         });
//     }
//     
//     for state in state_enum.variants.iter() {
//         let state_trait = format_ident!("{}State", state.ident);
//         result.extend(quote!{
//             #[allow(dead_code)]
//             trait #state_trait {} 
//         });
//     }
//
//     if state_enum.variants.len() >= 1 {
//         let state1 = state_enum.variants.get(0).unwrap();
//         for state2 in state_enum.variants.iter() {
//             if state1.ident == state2.ident {
//                 continue;
//             }
//             let state_trait = format_ident!("{}State{}State", state1.ident, state2.ident);
//             result.extend(quote!{
//                 #[allow(dead_code)]
//                 trait #state_trait : State {} 
//             });
//             for variant in state_enum.variants.iter() {
//                 result.extend(quote!{
//                     impl #state_trait for #variant {} 
//                 });
//             }
//         }
//     }
//
//     for state in state_enum.variants.iter() {
//         result.extend(quote!{
//             impl TryFrom<#store> for #register<#state> {
//                 type Error = (kernel::ErrorCode, #store);
//                 fn try_from(store: #store) -> Result<Self, Self::Error> {
//                     match store {
//                         #store::#state(reg) => Ok(reg),
//                         _ => Err((kernel::ErrorCode::INVAL, store)),
//                     }
//                 }
//             }
//         });
//     }
//
//     result.into()
// }

