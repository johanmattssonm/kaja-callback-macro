// Copyright (c) 2026 Johan Mattsson
// License: MIT

#![doc = include_str!("../README.md")]

use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, Ident, ItemFn, LitStr, parse_macro_input};

/// This macro makes sure a function is registerd in JS DOM Window for a given callback
/// in the Kaja Web Framework (WebAssembly, Rust).
///
/// Example usage:
/// ```rust,ignore
/// kaja_web::prelude::*
///
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
///
/// The program using this macro need to call `init_callback()` at startup
/// in order to have make the callbacks available in JavaScript.
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
    let is_async = input_fn.sig.asyncness.is_some();
    let callback_closure = generate_callback_closure(
        &fn_name,
        input_fn.sig.inputs.clone(),
        is_async,
        &input_fn.sig.output,
    );

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

                    let previous = match Reflect::get(window.as_ref(), &JsValue::from_str(#register_fn_name_lit)) {
                        Ok(v) => v,
                        Err(e) => {
                            ::gloo::console::error!("Callback registration check failed:", e);
                            JsValue::UNDEFINED
                        }
                    };

                    if !previous.is_undefined() {
                        ::gloo::console::error!("Callback already registered: {}. Callbacks are global on the entire document", #register_fn_name_lit);
                        return;
                    }

                    let _ = Reflect::set(
                        window.as_ref(),
                        &JsValue::from_str(#register_fn_name_lit),
                        callback_closure.as_ref().unchecked_ref(),
                    );

                    let key = JsValue::from_str(#js_name_lit);
                    let _ = Reflect::set(window.as_ref(), &key, callback_closure.as_ref().unchecked_ref());
                    callback_closure.forget();
                }
            }

            ::inventory::submit! {
                InitFn(#register_fn_name)
            }
        }
    };

    TokenStream::from(expanded)
}

// The closure will look like this:
/*
    let callback_js_closure = Closure::<dyn FnMut(wasm_bindgen::JsValue, wasm_bindgen::JsValue)>::new(|val, val2| {
        let event: SomeStruct =
            serde_wasm_bindgen::from_value(val);

        if event.is_err() {
            gloo::console::error!("Callback error: {}. Wrong argument type: {}", fn_name, event.err().unwrap());
            return;
        }

        let event2: u32 =
            serde_wasm_bindgen::from_value(val2);

        if event2.is_err() {
            gloo::console::error!("Callback error: {}. Wrong argument type: {}", fn_name, event2.err().unwrap());
            return;
        }

        let event = event.unwrap();
        let event2 = event.unwrap();

        the_rust_callback_function(event, event2);
    });
*/
fn generate_callback_closure(
    fn_name: &Ident,
    inputs: syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    is_async: bool,
    return_type: &syn::ReturnType,
) -> proc_macro2::TokenStream {
    use proc_macro2::Span;
    use quote::quote;

    let mut js_value_types: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut js_arg_idents: Vec<syn::Ident> = Vec::new();
    let mut rust_arg_types: Vec<syn::Type> = Vec::new();
    let mut rs_arg_idents: Vec<syn::Ident> = Vec::new();
    let mut temp_result_idents: Vec<syn::Ident> = Vec::new();

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

    let mut conversions = Vec::new();
    for ((res_ident, rs_ident), (val_ident, ty)) in temp_result_idents
        .iter()
        .zip(rs_arg_idents.iter())
        .zip(js_arg_idents.iter().zip(rust_arg_types.iter()))
    {
        // If the expected Rust type is JsValue, skip serde conversion and
        // pass the JsValue through directly.
        let ty_tokens = quote! { #ty }.to_string();

        let conv = if ty_tokens.contains("JsValue") {
            quote! {
                let #rs_ident = #val_ident;
            }
        } else if ty_tokens.contains("web_sys")
            || ty_tokens.contains("js_sys")
            || ty_tokens.contains("wasm_bindgen")
            || ty_tokens.contains("HtmlElement")
        {
            // Convert JsValue -> web_sys/js_sys type using JsCast::dyn_into
            // Treat bare `HtmlElement` identifiers as web_sys elements as well
            quote! {
                let #res_ident = #val_ident.clone().dyn_into::<#ty>();
                if #res_ident.is_err() {
                    gloo::console::log!(
                        concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument"),
                        #res_ident.err().unwrap()
                    );
                    return JsValue::UNDEFINED;
                }
                let #rs_ident = #res_ident.unwrap();
            }
        } else {
            quote! {
                let #res_ident = serde_wasm_bindgen::from_value::<#ty>(#val_ident.clone());
                if #res_ident.is_err() {
                    gloo::console::log!(
                        concat!("Callback error: ", stringify!(#fn_name), ". Wrong argument"),
                        #res_ident.err().unwrap()
                    );
                    return JsValue::UNDEFINED;
                }
                let #rs_ident = #res_ident.unwrap();
            }
        };

        conversions.push(conv);
    }

    let js_types: Vec<proc_macro2::TokenStream> = (0..js_arg_idents.len())
        .map(|_| quote! { wasm_bindgen::JsValue })
        .collect();

    // Build the call + conversion to JsValue for the function's return value.
    let ret_handling = match return_type {
        syn::ReturnType::Default => {
            // no return type -> return undefined
            quote! {
                #fn_name( #(#rs_arg_idents),* );
                JsValue::UNDEFINED
            }
        }
        syn::ReturnType::Type(_, ty) => {
            let ret_ty_tokens = quote! { #ty }.to_string();

            if ret_ty_tokens.contains("JsValue") {
                // function returns a JsValue directly
                quote! {
                    let res = #fn_name( #(#rs_arg_idents),* );
                    res
                }
            } else if ret_ty_tokens.contains("web_sys")
                || ret_ty_tokens.contains("js_sys")
                || ret_ty_tokens.contains("wasm_bindgen")
                || ret_ty_tokens.contains("HtmlElement")
            {
                // web_sys/js_sys types -> convert to JsValue via Into
                quote! {
                    let res = #fn_name( #(#rs_arg_idents),* );
                    wasm_bindgen::JsValue::from(res)
                }
            } else {
                // fallback: serialize with serde_wasm_bindgen
                quote! {
                    let res = #fn_name( #(#rs_arg_idents),* );
                    match serde_wasm_bindgen::to_value(&res) {
                        Ok(v) => v,
                        Err(e) => {
                            ::gloo::console::error!(
                                concat!("Callback error: ", stringify!(#fn_name), ". Serialize error"),
                                e
                            );
                            JsValue::UNDEFINED
                        }
                    }
                }
            }
        }
    };

    // Build async conversion logic (returns Result<JsValue, JsValue> inside the future)
    let async_conv = match return_type {
        syn::ReturnType::Default => quote! { Ok(JsValue::UNDEFINED) },
        syn::ReturnType::Type(_, ty) => {
            let ret_ty_tokens = quote! { #ty }.to_string();
            if ret_ty_tokens.contains("JsValue") {
                quote! { Ok(ret) }
            } else if ret_ty_tokens.contains("web_sys")
                || ret_ty_tokens.contains("js_sys")
                || ret_ty_tokens.contains("wasm_bindgen")
                || ret_ty_tokens.contains("HtmlElement")
            {
                quote! { Ok(wasm_bindgen::JsValue::from(ret)) }
            } else {
                quote! {
                    match serde_wasm_bindgen::to_value(&ret) {
                        Ok(v) => Ok(v),
                        Err(e) => {
                            ::gloo::console::error!(
                                concat!("Callback error: ", stringify!(#fn_name), ". Serialize error"),
                                e
                            );
                            Ok(JsValue::UNDEFINED)
                        }
                    }
                }
            }
        }
    };

    let expanded = if is_async {
        // For async functions return a Promise (JsValue) by using future_to_promise
        quote! {
            let callback_closure = Closure::wrap(Box::new(move | #( #js_arg_idents : #js_types ),* | -> wasm_bindgen::JsValue {
                #(#conversions)*
                let promise = ::wasm_bindgen_futures::future_to_promise(async move {
                    let ret = #fn_name( #(#rs_arg_idents),* ).await;
                    #async_conv
                });

                promise.into()
            }) as Box<dyn FnMut( #(#js_types),* ) -> wasm_bindgen::JsValue + 'static>);
        }
    } else {
        quote! {
            let callback_closure = Closure::wrap(Box::new(move | #( #js_arg_idents : #js_types ),* | -> wasm_bindgen::JsValue {
                #(#conversions)*
                #ret_handling
            }) as Box<dyn FnMut( #(#js_types),* ) -> wasm_bindgen::JsValue + 'static>);
        }
    };

    return expanded;
}

struct CallbackArg {
    name: String,
    span: proc_macro2::Span,
}

// remove quote if present
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
