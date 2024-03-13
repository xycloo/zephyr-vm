
use proc_macro::TokenStream;
use quote::quote;
use syn::{self, parse_macro_input, DeriveInput, Expr, ExprLit, FieldsNamed, Ident, Lit, LitStr, Type};

// todo: clean code

#[proc_macro_derive(DatabaseInteract, attributes(with_name))]
pub fn database_interact_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    
    // Extract the struct name
    let struct_name = &input.ident;
    
    // Extract the with_name attribute if it exists
    let with_name_attr = input.attrs.iter().find_map(|attr| {
        if attr.path().is_ident("with_name") {
            let value: Expr = attr.parse_args().unwrap();
            if let Expr::Lit(ExprLit { lit, .. }) = value {
                if let Lit::Str(value) = lit {
                    return Some(value.value())
                } else {
                    panic!("Invalid lit type")
                }
            } else {
                panic!("Invalid type")
            }
        } else {
            panic!("No table name provided")
        } 
    }).expect("No with_name attribute");

    let idents: Vec<(Ident, usize, Ident)> = match input.data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Named(FieldsNamed { named, .. }) => {
                named.iter().enumerate().map(|(idx, field)| {
                    let Type::Path(path) = &field.ty else {
                        panic!("unsupported field type")
                    };

                    (field.ident.clone().unwrap(), idx, path.path.segments[0].ident.clone())

                }).collect()
            }
            _ => panic!("Unnamed structs are not supported.")
        },

        _ => panic!("Unsupported type.")
    };
    let field_literals: Vec<Lit> = idents
        .iter()
        .map(|ident| {
            let field_str = LitStr::new(&ident.0.to_string(), ident.0.span());
            Lit::Str(field_str)
        })
        .collect();

    macro_rules! check_type {
        ($t:expr, $($expected:literal),*) => {
            matches!($t, $($expected)|*)
        };
    }
    
    let construction_code = idents.iter().map(|(ident, _, field_type)| {
        if check_type!(field_type.to_string().as_str(), "i64", "i128", "u64", "f64", "u32", "i32", "f32", "String", "Vec") {
            quote! {
                #ident: #ident.try_into().unwrap(),
            }
        } else {
            quote! {
                #ident,
            }
        }
    });

    let deser_code = idents.iter().map(|(ident, index, field_type)| {
        let field_string = field_type.to_string();
        let field_str = field_string.as_str();
        if check_type!(field_type.to_string().as_str(), "i64", "i128", "u64", "f64", "u32", "i32", "f32", "String", "Vec") {
            quote! {
                let bytes = row.row.get(#index).unwrap();
                let #ident = bincode::deserialize::<ZephyrVal>(&bytes.0).unwrap();
            
            }
        } else if check_type!(field_str, "ScVal", "Hash") {
            quote! {
                let bytes = row.row.get(#index).unwrap();
                let #ident = ReadXdr::from_xdr(&bytes.0, Limits::none()).unwrap();
            
            }
        } else {
            quote! {
                let bytes = row.row.get(#index).unwrap();
                let #ident = bincode::deserialize(&bytes.0).unwrap();
                
            }
        }
    });

    let serialize_type = idents.iter().map(|(ident, _, field_type)| {
        if check_type!(field_type.to_string().as_str(), "i64", "i128", "u64", "f64", "u32", "i32", "f32", "String", "Vec") {
            quote! {
                bincode::serialize(&TryInto::<ZephyrVal>::try_into(self.#ident.clone()).unwrap()).unwrap().as_slice()
            }
        } else if check_type!(field_type.to_string().as_str(), "ScVal", "Hash") {
            quote! {
                self.#ident.clone().to_xdr(Limits::none()).unwrap().as_slice()
            }
        }  else {
            quote! {
                bincode::serialize(&self.#ident).unwrap().as_slice()
            }
        }
    });

    let serialize_type_update = idents.iter().map(|(ident, _, field_type)| {
        if check_type!(field_type.to_string().as_str(), "i64", "i128", "u64", "f64", "u32", "i32", "f32", "String", "Vec") {
            quote! {
                bincode::serialize(&TryInto::<ZephyrVal>::try_into(self.#ident.clone()).unwrap()).unwrap().as_slice()
            }
        } else if check_type!(field_type.to_string().as_str(), "ScVal", "Hash") {
            quote! {
                self.#ident.clone().to_xdr(Limits::none()).unwrap().as_slice()
            }
        } else {
            quote! {
                bincode::serialize(&self.#ident).unwrap().as_slice()
            }
        }
    });

    // Generate the implementation of the trait
    let expanded = quote! {
        //use rs_zephyr_sdk::{bincode, ZephyrVal};
        //use std::convert::TryInto;

        impl DatabaseInteract for #struct_name {
            fn read_to_rows(env: &EnvClient) -> Vec<Self> where Self: Sized {
                let rows = env.db_read(&#with_name_attr, &[#(#field_literals),*]).unwrap();
                let mut result = Vec::new();
                
                for row in rows.rows {
                    #(#deser_code)*
                    result.push(Self {
                        #(#construction_code)*
                    });
                }


                result
            }

            fn put(&self, env: &EnvClient) {
                env.db_write(&#with_name_attr, &[#(#field_literals),*], &[#(#serialize_type),*]).unwrap();
            }

            fn update(&self, env: &EnvClient, conditions: &[Condition]) {
                env.db_update(&#with_name_attr, &[#(#field_literals),*], &[#(#serialize_type_update),*], conditions).unwrap();
            }
        }
    };


    // Return the generated implementation
    TokenStream::from(expanded)
}

