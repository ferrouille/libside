use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use std::{collections::HashSet, fs};
use syn::{parse_macro_input, Ident, Lit};

enum Component {
    Str(String),
    Param { name: String },
}

impl ToTokens for Component {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Component::Str(s) => {
                tokens.extend(quote! {
                    contents.push_str(&#s);
                });
            }
            Component::Param { name } => {
                let ident = Ident::new(name, Span::call_site());
                tokens.extend(quote! {
                    contents.push_str(&::libside::builder::AsParam::as_param(&p.#ident));
                });
            }
        }
    }
}

fn parse(mut s: &str) -> Vec<Component> {
    let mut result = Vec::new();
    loop {
        if let Some(next) = s.find("{{") {
            result.push(Component::Str(s[..next].to_string()));
            s = s[next + 2..].trim_start();
            let end = s.find("}}").expect("Can't find end of {{ tag");
            let name = s[..end].trim();
            s = &s[end + 2..];
            result.push(Component::Param {
                name: name.to_string(),
            });
        } else {
            result.push(Component::Str(s.to_string()));
            break result;
        }
    }
}

#[proc_macro]
pub fn config_file(tokens: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut iter = tokens.into_iter();
    let mut first_token = proc_macro::TokenStream::new();
    first_token.extend([iter.next().unwrap()]);

    let mut rest = proc_macro::TokenStream::new();
    rest.extend(iter);
    let rest: TokenStream = rest.into();

    let lit = parse_macro_input!(first_token as Lit);
    let path = if let Lit::Str(s) = lit {
        s.value()
    } else {
        panic!();
    };

    let realpath = std::fs::canonicalize(&path).unwrap();
    let realpath = realpath.display().to_string();

    let contents = fs::read_to_string(&path).unwrap();
    let components = parse(&contents);
    let params = components
        .iter()
        .map(|c| match c {
            Component::Param { name } => Some(name.as_str()),
            _ => None,
        })
        .flatten()
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|n| Ident::new(n, Span::call_site()))
        .collect::<Vec<_>>();

    let mut ts = TokenStream::new();

    ts.extend(quote! {
        {
            let _ = include_bytes!(#realpath);
            struct Params<#(#params: libside::builder::AsParam,)*> {
                #(#params: #params),*
            }


            let p = Params {
                #rest
            };

            let mut contents = String::new();
            #(
                #components
            )*

            let contents = contents.as_bytes().to_vec();

            ::libside::builder::fs::ConfigFileData {
                path: std::path::PathBuf::from(#path).file_name().unwrap().into(),
                contents,
                path_dependency: None,
                extra_dependencies: Vec::new(),
            }
        }
    });

    ts.into()
}
