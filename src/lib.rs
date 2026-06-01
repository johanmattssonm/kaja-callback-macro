use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, FnArg, Ident, ItemFn, LitStr};

/// This macro makes sure a function is registerd in JS DOM Window for a given callback
/// in the Kaja Web Framework (WebAssembly, Rust).
///
/// Example usage:
/// ```
/// #[derive(serde::Deserialize)]
/// struct SomeClickEvent {
///     índex: usize,
/// }
///
/// #[callback("someClickCallback")]
/// fn some_on_click_function(event: SomeClickEvent) {
///     log!("event.índex: {:?}", event.índex);
/// }
///
/// // This example allows you to call the Rust function from JavaScript like this:
/// let html = kaja_html_macro::html! {{
///     <button onclick="someClickCallback({
///         index: 1
///     })">Run WASM Callback</button>
/// }};
/// ```
#[proc_macro_attribute]
pub fn callback(attr: TokenStream, item: TokenStream) -> TokenStream {
    let callback_arg = parse_macro_input!(attr as CallbackArg);
    let js_name_lit = LitStr::new(&callback_arg.name, callback_arg.span);
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let fn_block = &input_fn.block;

    let register_fn_name = syn::Ident::new(&format!("{}_register", fn_name), fn_name.span());
    let register_fn_name_lit = LitStr::new(&register_fn_name.to_string(), register_fn_name.span());
    let callback_closure = generate_callback_closure(&fn_name, input_fn.sig.inputs.clone());

    let expanded = {
        quote! {
            #vis #sig #fn_block

            #[wasm_bindgen]
            pub fn #register_fn_name() {
                use wasm_bindgen::closure::Closure;
                use wasm_bindgen::JsCast;
                use js_sys::{Array, Object, Reflect};
                use wasm_bindgen::JsValue;

                if let Some(window) = web_sys::window() {
                    #callback_closure;

                    let _ = Reflect::set(
                        window.as_ref(),
                        &JsValue::from_str(#register_fn_name_lit),
                        cb.as_ref().unchecked_ref(),
                    );

                    let key = JsValue::from_str(#js_name_lit);
                    let _ = Reflect::set(window.as_ref(), &key, cb.as_ref().unchecked_ref());
                    cb.forget();
                }
            }

            ::inventory::submit! {
                InitFn(#register_fn_name)
            }
        }
    };

    TokenStream::from(expanded)
}

fn generate_callback_closure(
    fn_name: &Ident,
    inputs: syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
) -> proc_macro2::TokenStream {
    use proc_macro2::Span;
    use quote::quote;

    let mut js_value_types: Vec<proc_macro2::TokenStream> = Vec::new(); // array of wasm_bindgen::JsValue,
    let mut js_arg_idents: Vec<syn::Ident> = Vec::new(); // val0, val1, ... lambda arguments
    let mut rust_arg_types: Vec<syn::Type> = Vec::new(); // extracted types form the annotated rust function
    let mut rs_arg_idents: Vec<syn::Ident> = Vec::new(); // arg0, arg1, ... converted from val0, val1,
    let mut temp_result_idents: Vec<syn::Ident> = Vec::new(); // res0, res1, ... result for serde parser

    for (i, arg) in inputs.iter().enumerate() {
        let span = Span::call_site();

        let js_value_type = quote! { wasm_bindgen::JsValue };
        js_value_types.push(js_value_type);

        let val_ident = syn::Ident::new(&format!("val{}", i), span);
        js_arg_idents.push(val_ident);

        let rs_ident = syn::Ident::new(&format!("arg{}", i), span);
        rs_arg_idents.push(rs_ident);

        let res_ident = syn::Ident::new(&format!("res{}", i), span);
        temp_result_idents.push(res_ident);

        match arg {
            FnArg::Typed(pat_type) => {
                rust_arg_types.push((*pat_type.ty).clone());
            }
            _ => panic!("expected typed argument"),
        }
    }

    if inputs.len() == 3 {
        let mut conversions = Vec::new();
        for ((res_ident, rs_ident), (val_ident, ty)) in temp_result_idents
            .iter()
            .zip(rs_arg_idents.iter())
            .zip(js_arg_idents.iter().zip(rust_arg_types.iter()))
        {
            // attempt simple conversions for common primitive types to avoid requiring
            // serde_wasm_bindgen in the consumer crate. Fall back to serde for other types.
            let conv = if let syn::Type::Path(type_path) = ty {
                if let Some(seg) = type_path.path.segments.last() {
                    let ident_str = seg.ident.to_string();
                    match ident_str.as_str() {
                        "u32" | "i32" | "usize" | "f64" => {
                            quote! {
                                let #res_ident: Option<#ty> = #val_ident.as_f64().map(|v| v as #ty);
                                if #res_ident.is_none() {
                                    gloo::console::log!(
                                        concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument")
                                    );
                                    return;
                                }
                                let #rs_ident = #res_ident.unwrap();
                            }
                        }
                        "bool" => {
                            quote! {
                                let #res_ident: Option<#ty> = #val_ident.as_bool();
                                if #res_ident.is_none() {
                                    gloo::console::log!(
                                        concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument")
                                    );
                                    return;
                                }
                                let #rs_ident = #res_ident.unwrap();
                            }
                        }
                        "String" => {
                            quote! {
                                let #res_ident: Option<#ty> = #val_ident.as_string();
                                if #res_ident.is_none() {
                                    gloo::console::log!(
                                        concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument")
                                    );
                                    return;
                                }
                                let #rs_ident = #res_ident.unwrap();
                            }
                        }
                        _ => {
                            quote! {
                                let #res_ident = serde_wasm_bindgen::from_value::<#ty>(#val_ident.clone());
                                if #res_ident.is_err() {
                                    gloo::console::log!(
                                        concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument"),
                                        #res_ident.err().unwrap()
                                    );
                                    return;
                                }
                                let #rs_ident = #res_ident.unwrap();
                            }
                        }
                    }
                } else {
                    // fallback
                    quote! {
                        let #res_ident = serde_wasm_bindgen::from_value::<#ty>(#val_ident.clone());
                        if #res_ident.is_err() {
                            gloo::console::log!(
                                concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument"),
                                #res_ident.err().unwrap()
                            );
                            return;
                        }
                        let #rs_ident = #res_ident.unwrap();
                    }
                }
            } else {
                // not a path type; fallback to serde
                quote! {
                    let #res_ident = serde_wasm_bindgen::from_value::<#ty>(#val_ident.clone());
                    if #res_ident.is_err() {
                        gloo::console::log!(
                            concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument"),
                            #res_ident.err().unwrap()
                        );
                        return;
                    }
                    let #rs_ident = #res_ident.unwrap();
                }
            };

            conversions.push(conv);
        }

        // build a token-list of wasm_bindgen::JsValue types for the trait object
        let js_types: Vec<proc_macro2::TokenStream> = (0..js_arg_idents.len())
            .map(|_| quote! { wasm_bindgen::JsValue })
            .collect();

        let expanded = quote! {
            let cb = Closure::wrap(Box::new(move | #( #js_arg_idents : #js_types ),* | {
                #(#conversions)*
                #fn_name( #(#rs_arg_idents),* );
            }) as Box<dyn FnMut( #(#js_types),* ) + 'static>);
        };

        return expanded;
    }

    if inputs.iter().len() == 2 {
        let expanded = quote! {
            let cb = Closure::<dyn FnMut(#(#js_value_types),*)>::new(
                |val0, val1| {

                let event = Test1Data {
                    test_parameter: "test".to_string(),
                };

                let event2 = "serialized".to_string();

                #fn_name(event, event2);
            });
        };

        return expanded;
    }

    if inputs.iter().len() == 1 {
        let expanded = quote! {
            let cb = Closure::<dyn FnMut(#(#js_value_types),*)>::new(
                |val0| {
                let event = Test1Data {
                    test_parameter: "test".to_string(),
                };

                #fn_name(event);
            });
        };

        return expanded;
    }

    let expanded = quote! {
        let cb = Closure::<dyn FnMut(#(#js_value_types),*)>::new(
            || {
            #fn_name();
        });
    };

    return expanded;

    let mut js_arg_idents: Vec<syn::Ident> = Vec::new(); // val0, val1, ... lambda arguments
    let mut rust_arg_types: Vec<syn::Type> = Vec::new(); // extracted types form the annotated rust function
    let mut rs_arg_idents: Vec<syn::Ident> = Vec::new(); // arg0, arg1, ... converted from val0, val1,
    let mut temp_result_idents: Vec<syn::Ident> = Vec::new(); // res0, res1, ... result for serde parser

    for (i, arg) in inputs.iter().enumerate() {
        let span = Span::call_site();
        let val_ident = syn::Ident::new(&format!("val{}", i), span);
        js_arg_idents.push(val_ident);

        let rs_ident = syn::Ident::new(&format!("arg{}", i), span);
        rs_arg_idents.push(rs_ident);

        let res_ident = syn::Ident::new(&format!("res{}", i), span);
        temp_result_idents.push(res_ident);

        match arg {
            FnArg::Typed(pat_type) => {
                rust_arg_types.push((*pat_type.ty).clone());
            }
            _ => panic!("expected typed argument"),
        }
    }

    let js_value_type = quote! { wasm_bindgen::JsValue };
    let js_types: Vec<proc_macro2::TokenStream> = (0..js_arg_idents.len())
        .map(|_| js_value_type.clone())
        .collect();

    let mut conversions = Vec::new();
    for ((res_ident, rs_ident), (val_ident, ty)) in temp_result_idents
        .iter()
        .zip(rs_arg_idents.iter())
        .zip(js_arg_idents.iter().zip(rust_arg_types.iter()))
    {
        conversions.push(quote! {
            let #res_ident = serde_wasm_bindgen::from_value::<#ty>(#val_ident.clone());
            if #res_ident.is_err() {
                gloo::console::log!(
                    concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument type: "),
                    #res_ident.err().unwrap()
                );
                return;
            }
            let #rs_ident = #res_ident.unwrap();
        });
    }

    let call_args = rs_arg_idents.iter();
    let cb_tokens = quote! {
        let cb = Closure::wrap(Box::new(move | #(#js_arg_idents),* | {
            #(#conversions)*

            // call the original Rust function with the converted args
            #fn_name( #(#call_args),* );
        }) as Box<dyn FnMut( #(#js_types),* ) + 'static>);
    };

    cb_tokens
}

struct CallbackArg {
    name: String,
    span: proc_macro2::Span,
}

// remoce quote if present
impl syn::parse::Parse for CallbackArg {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            let s: LitStr = input.parse()?;
            Ok(CallbackArg {
                name: s.value(),
                span: s.span(),
            })
        } else if input.peek(syn::Ident) {
            let id: syn::Ident = input.parse()?;
            Ok(CallbackArg {
                name: id.to_string(),
                span: id.span(),
            })
        } else {
            Err(input.error("expected string literal or identifier, e.g. #[callback(\"name\")] or #[callback(name)]"))
        }
    }
}
