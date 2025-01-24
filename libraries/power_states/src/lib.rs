use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::{self, Punctuated},
    DeriveInput, Field, FieldMutability, Fields, FieldsUnnamed, Ident, PathArguments, Type,
    Variant, Visibility,
};

use std::collections::{hash_set::HashSet, HashMap};

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct State {
    ident: syn::Ident,
    shortname: syn::Ident,
    substates: Punctuated<syn::Ident, syn::Token![,]>,
    transitions: Punctuated<State, syn::Token![,]>,
}

impl State {
    fn form_concrete_state_type(&self) -> proc_macro2::TokenStream {
        let state_ident = self.ident.clone();

        if self.substates.is_empty() {
            quote! {
                #state_ident
            }
        } else {
            let args = self.substates.iter().map(|arg| {
                quote! {
                    #arg
                }
            });

            let args_ident = quote! {#(#args),*};
            quote!{
                #state_ident<#args_ident>
            }
        }
    }

    fn generate_state(
        &self,
        register_name: &Ident,
        store_name: &Ident,
        struct_name: &proc_macro2::TokenStream,
        any_type: bool
    ) -> proc_macro2::TokenStream {
        let mut result = proc_macro2::TokenStream::new();

        let state_ident = self.ident.clone();
        let struct_shortname = self.shortname.clone();
        // Form struct name / generics in carrots

        // Form full struct using formed name
        if self.substates.is_empty() {
            result.extend(quote! {
                impl State for #struct_name {
                    type Reg = #register_name<#state_ident>;
                    type StateEnum = #store_name;
                }

                impl Reg for #register_name<#struct_name> {
                    type StateEnum = #store_name;
                }

                impl From<#register_name<#struct_name>> for #store_name {
                    fn from(reg: #register_name<#struct_name>) -> Self {
                        #store_name::#state_ident(reg)
                    }
                }
           
                impl TryFrom<#store_name> for #register_name<#struct_name> {
                    type Error = (kernel::ErrorCode, #store_name);
                    fn try_from(store: #store_name) -> Result<Self, Self::Error> {
                        match store {
                            #store_name::#state_ident(reg) => Ok(reg),
                            _ => Err((kernel::ErrorCode::INVAL, store)),
                        }
                    }
                }
            });
        } else {

            let concrete_type = self.form_concrete_state_type();
            result.extend(quote!{
                impl State for #concrete_type {
                    type Reg = #register_name<#concrete_type>;
                    type StateEnum = #store_name;
                }

                impl Reg for #register_name<#concrete_type> {
                    type StateEnum = #store_name;
                }
                
            });

            if any_type {
                result.extend(quote! {
                    impl From<#register_name<#concrete_type>> for #store_name {
                        fn from(reg: #register_name<#concrete_type>) -> Self {
                            unimplemented!();
                        }
                    }

                    impl TryFrom<#store_name> for #register_name<#concrete_type> {
                        type Error = (kernel::ErrorCode, #store_name);
                        fn try_from(store: #store_name) -> Result<Self, Self::Error> {
                            unimplemented!();
                        }
                    }
                });
            } else {
                result.extend(quote!{
                    impl From<#register_name<#concrete_type>> for #store_name {
                      fn from(reg: #register_name<#concrete_type>) -> Self {
                          #store_name::#struct_shortname(reg)
                      }
                    }

                    impl TryFrom<#store_name> for #register_name<#concrete_type> {
                        type Error = (kernel::ErrorCode, #store_name);
                        fn try_from(store: #store_name) -> Result<Self, Self::Error> {
                            match store {
                                #store_name::#struct_shortname(reg) => Ok(reg),
                                _ => Err((kernel::ErrorCode::INVAL, store)),
                            }
                        }
                    }
                });
            }
        }
        
        result
    }
}

impl Parse for State {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let state: Ident = input.parse().expect("state_parse 0");

        let substates = if input.peek(syn::token::Paren) {
            let content;
            let _: syn::token::Paren = syn::parenthesized!(content in input);
            let substates: Punctuated<Ident, syn::Token![,]> = content
                .parse_terminated(syn::Ident::parse, syn::Token![,])
                .expect("state_parse 1");
            Some(substates)
        } else {
            None
        }
        .unwrap_or_else(|| Punctuated::new());

        let (transitions, shortname) = if input.peek(syn::Token![=>]) {
            input.parse::<syn::Token![=>]>().expect("state_parse 2");
            let content;
            let _: syn::token::Bracket = bracketed!(content in input);
            let transitions: Punctuated<State, syn::Token![,]> = content
                .parse_terminated(State::parse, syn::Token![,])
                .expect("state_parse 3");

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
    StateChange(State, syn::Path, Ident),
}

impl RegisterType {
    fn to_ident(&self) -> Ident {
        match self {
            RegisterType::ReadOnly => format_ident!("ReadOnly"),
            RegisterType::WriteOnly => format_ident!("WriteOnly"),
            RegisterType::ReadWrite => format_ident!("ReadWrite"),
            RegisterType::StateChange(_, _, _) => format_ident!("StateChange"),
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
            "StateChange" => {
                let content;
                let _: syn::token::Paren = syn::parenthesized!(content in input);
                let new_state = content.parse::<State>()?;

                let _: syn::Token![,] = content.parse()?;

                let instruction = content.parse::<syn::Path>().expect("registertype 2");

            
                if new_state.substates.is_empty() {
                    let state_shortname = new_state.ident.clone();
                    return Ok(RegisterType::StateChange(new_state, instruction, state_shortname));
                } else {
                    let _: syn::Token![,] = content.parse().expect("final comma in register state change");
                    let state_shortname = content.parse::<syn::Ident>().expect("registertype 3");
                    return Ok(RegisterType::StateChange(new_state, instruction, state_shortname))
                } 

            }
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

impl Register {
    fn generate_register_name_type(&self) -> proc_macro2::TokenStream {
        let type_name_ident = self.type_name.clone();
        quote! { struct #type_name_ident {}}
    }

    fn generate_register_op_bindings(
        &self,
        peripheral_name: &Ident,
        register_name: &Ident,
    ) -> proc_macro2::TokenStream {
        // impl ReadWriteRegister<#register_bitwidth, #register_shortname, #type_name, #validstate> {
        let register_bitwidth = self.register_bitwidth.clone();
        let register_shortname = self.register_shortname.clone();
        let type_name = self.type_name.clone();
        let validstate = self.valid_states.first().expect("generate reg op bindings").form_concrete_state_type();

        if self.valid_states.len() != 1 {
            panic!("Only one valid state is supported for now.");
        }

        match &self.register_type {
            RegisterType::ReadOnly => {
                quote! {
                    impl ReadOnlyRegister<#register_bitwidth, #register_shortname, #type_name, #validstate> {
                        pub fn get(&self) -> #register_bitwidth {
                            self.reg.get()
                        }
                    }
                }
            }
            RegisterType::WriteOnly => {
                quote! {
                    impl WriteOnlyRegister<#register_bitwidth, #register_shortname, #type_name, #validstate> {
                        pub fn set(&self, value: #register_bitwidth) {
                            self.reg.set(value)
                        }

                        pub fn write(&self, value: FieldValue<#register_bitwidth, #register_shortname>) {
                            self.reg.write(value)
                        }
                    }
                }
            }
            RegisterType::ReadWrite => {
                quote! {
                    impl ReadWriteRegister<#register_bitwidth, #register_shortname, #type_name, #validstate> {
                        pub fn get(&self) -> #register_bitwidth {
                            self.reg.get()
                        }
                        pub fn set(&self, value: #register_bitwidth) {
                            self.reg.set(value)
                        }

                        pub fn write(&self, value: FieldValue<#register_bitwidth, #register_shortname>) {
                            self.reg.write(value)
                        }
                    }
                }
            }
            RegisterType::StateChange(state, instruction, state_shortname) => {
                let reg_field_name = self.name.clone();
                let trait_name = format_ident!("Step{}", state_shortname);

                // Determine if this state contains an Any substate.
                let is_anytype = state.substates.iter().any(|substate| substate.to_string() == "Any");

                let to_state_fn_name =
                    format_ident!("into_{}", state_shortname.to_string().to_lowercase());

                if is_anytype {
                    // Create copy of state to change
                    let mut state = state.clone();
                    let from_state = self.valid_states.first().expect("state valid").clone();

                    // An Any substate means an state is valid. To mock up this behavior in the 
                    // type system, we must replace the Any substate with a generic type.
                    let map_any = |mut state: State| {
                        // For any substate that is Any, replace with generic T.
                        
                        for substate in state.substates.iter_mut() {
                            if substate.to_string() == "Any" {
                                *substate = format_ident!("T");
                            }
                        }

                        state.form_concrete_state_type()

                    };

                    let to_state = map_any(state);
                    let from_state = map_any(from_state);

                    quote! {
                        trait #trait_name<T: SubState>: Sized
                        where 
                            #to_state: State,
                            #from_state: State
                        {
                            fn #to_state_fn_name<PM: PowerManager<#peripheral_name>>(
                                self,
                                pm: &PM,
                            ) -> Result<#register_name<#to_state>, PowerError<Self>>;
                        }

                        impl <T: SubState> #trait_name<T> for #register_name<#from_state> 
                        where 
                            #to_state: State,
                            #from_state: State
                        {
                            fn #to_state_fn_name<PM: PowerManager<#peripheral_name>>(
                                self,
                                _pm: &PM,
                            ) -> Result<#register_name<#to_state>, PowerError<Self>> {
                                self.#reg_field_name.reg.write(#instruction);

                                unsafe {
                                    Ok(transmute::<
                                        #register_name<#from_state>,
                                        #register_name<#to_state>
                                    >(self))
                                }
                            }
                        }
                    }                   
                } else { 
                    let from_state = self.valid_states.first().expect("state change unwrap").form_concrete_state_type();
                    let to_state = state.form_concrete_state_type();
                    
                    quote! {
                        trait #trait_name: Sized {
                            fn #to_state_fn_name<PM: PowerManager<#peripheral_name>>(
                                self,
                                pm: &PM,
                            ) -> Result<#register_name<#to_state>, PowerError<Self>>;
                        }

                        impl #trait_name for #register_name<#from_state> {
                            fn #to_state_fn_name<PM: PowerManager<#peripheral_name>>(
                                self,
                                _pm: &PM,
                            ) -> Result<#register_name<#to_state>, PowerError<Self>> {
                                self.#reg_field_name.reg.write(#instruction);

                                unsafe {
                                    Ok(transmute::<
                                        #register_name<#from_state>,
                                        #register_name<#to_state>
                                    >(self))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

struct RegisterAttributes {
    states: Punctuated<State, syn::Token![,]>,
    register_type: RegisterType,
    type_name: Ident,
}

impl Parse for RegisterAttributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        let _ = bracketed!(content in input);
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

                let state_ident = state.ident.clone();
                let substate_tokens = if state.substates.is_empty() {
                    quote! {
                        #state_ident
                    }
                } else {
                    quote! {
                        #state_ident<#(#substate_iter),*>
                    }
                };

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
                                #register_name<#substate_tokens>
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
    ) -> (proc_macro2::TokenStream, HashMap<String, State>) {
        let mut created_states: HashSet<syn::Ident> = HashSet::new();

        let mut output = proc_macro2::TokenStream::new();
        let mut unique_substates = HashSet::new();
        unique_substates.insert(format_ident!("Any"));

        let mut state_hash = HashSet::new();

        let mut state_map = HashMap::new();
        for state in &self.states {
            // State hash map used for name mapping later.
            state_map.insert(state.form_concrete_state_type().to_string(), state.clone());

            let mut state = state.clone();
            let original_substates = state.substates.clone();
            // We need to generate each substate as:
            // 1. The specified state.
            // 2. As potentially an any state.
            // 3. As potentially all being any states.
            for iter in 0..(&state.substates.len() + 2) {
                let mut any_type = false;
                state.substates = original_substates.clone();
                
                // Case (2) 
                if iter < state.substates.len() {
                    state.substates[iter] = format_ident!("Any");

                    any_type = true;
                }

                // Case (3)
                if iter == state.substates.len() {
                    state.substates = state.substates.iter().map(|_| format_ident!("Any")).collect();
                    any_type = true;
                }                

                let state_ident = state.ident.clone();

                for substate in &state.substates {
                    unique_substates.insert(substate.clone());
                }

                let struct_name = if state.substates.is_empty() {
                    quote! {#state_ident}
                } else {
                    let generic_params = state.substates.iter().enumerate().map(|(index, _)| {
                        let entry = format!("T{}", index);
                        let generic = syn::Ident::new(&entry, Span::call_site());

                        quote! {
                            #generic: SubState
                        }
                    });

                    quote! {
                        #state_ident<#(#generic_params),*>
                    }
                };
       
                let fields = state.substates.iter().enumerate().map(|(index, _)| {
                    let field_name = format!("associated_{}", index);
                    let generic_name = format!("T{}", index);

                    let generic = syn::Ident::new(&generic_name, Span::call_site());
                    let field = syn::Ident::new(&field_name, Span::call_site());

                    quote! {
                        #field: PhantomData<#generic>
                    }
                });

                let concrete_state_str = state.form_concrete_state_type().to_string();
                eprintln!("concrete_state_str: {:?}", concrete_state_str); 
                if state_hash.contains(&concrete_state_str) {
                    eprintln!("match!");
                    continue;
                } else {
                    state_hash.insert(concrete_state_str.clone());
                }

                if !created_states.contains(&state.ident) {
                    output.extend(
                        quote!{
                            pub struct #struct_name {
                                #(#fields),*
                            }
                        }
                    );
                    
                    created_states.insert(state.ident.clone());
                }
                
                
                output.extend(state.generate_state(register_name, store_name, &struct_name, any_type));
            }
        }

        // create substates 
        for substate in unique_substates {
            let substate_ident = format_ident!("{}", substate);
            output.extend(quote! {
                pub struct #substate_ident {}

                impl SubState for #substate_ident {}
            });
        }

        (output, state_map)
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
pub fn process_register_block(attr: TokenStream, item: TokenStream) -> TokenStream {
    let parsed_input = parse_macro_input!(attr as MacroInput);
    // form reg and store type names from given peripheral name
    let register = format_ident!("{}Registers", parsed_input.peripheral_name);
    let store = format_ident!("{}Store", parsed_input.peripheral_name);
    let peripheral = format_ident!("{}Peripheral", parsed_input.peripheral_name);
    let register_block_str = format!("{}{}", parsed_input.peripheral_name, "RegisterBlock");
    let register_block = format_ident!("{}", register_block_str);

    let mut result = add_imports();

    eprintln!("register: {:?}", register);
    let block = quote! {
        pub struct #register<S: kernel::power_manager::State> {
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
    let (states_generated, state_map) = parsed_input.generate_states(&register, &store);
    result.extend(states_generated);

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


    let ast: DeriveInput = syn::parse(item).expect("ast unwrap");

    let data = match &ast.data {
        syn::Data::Struct(data) => data,
        _ => panic!("Unsupported data type"),
    };

    let mut reg_vec: Vec<Register> = vec![];

    let struct_name_ident = format_ident!("{}RegisterBlock", parsed_input.peripheral_name);

    let field_details = data.fields.iter().map(|field| {
        let field_type = field.ty.clone();
        let field_name = field.ident.clone().expect("field details");

        let requires_gen = field.attrs.iter().any(|attr| {
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
                    return Some(attr.parse_args::<RegisterAttributes>().expect("requires gen"));
                }
                None
            }).expect("reg attribute error");
            if let Type::Path(type_path) = field_type.clone() {
                if let Some(segment) = type_path.path.segments.last() {

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
                            generic_ident.segments.first().expect("inner req gen").ident.clone()
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
                    let reg_type = format_ident!("{}Register", &reg_attr.register_type.to_ident());

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
            pub #field_name: #field_type

        }
    }
});

    let struct_output = quote! {
        pub struct #struct_name_ident<S: State> {
            #(#field_details),*
        }
    };

    for reg in reg_vec {
        result.extend(reg.generate_register_name_type());
        //  result.extend(reg.generate_state_transition(&peripheral, &register));
        result.extend(reg.generate_register_op_bindings(&peripheral, &register));
    }

    // FIX ME
    result.extend(quote! {
        struct StateChangeRegister<T: UIntLike, R: RegisterLongName, Name, S: State> {
            reg: WriteOnly<T, R>,
            associated_state: PhantomData<S>,
            associated_name: PhantomData<Name>,
        }

        struct ReadWriteRegister<T: UIntLike, R: RegisterLongName, Name, S: State> {
            reg: ReadWrite<T, R>,
            associated_state: PhantomData<S>,
            associated_name: PhantomData<Name>,
        }

        struct ReadOnlyRegister<T: UIntLike, R: RegisterLongName, Name, S: State> {
            reg: ReadOnly<T, R>,
            associated_state: PhantomData<S>,
            associated_name: PhantomData<Name>,
        }

        struct WriteOnlyRegister<T: UIntLike, R: RegisterLongName, Name, S: State> {
            reg: WriteOnly<T, R>,
            associated_state: PhantomData<S>,
            associated_name: PhantomData<Name>,
        }
    });

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
