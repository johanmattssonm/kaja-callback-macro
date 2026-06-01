use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, LitStr};

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

    // Extract single argument type
    let arg = input_fn
        .sig
        .inputs
        .first()
        .expect("callback must have one argument");

    let arg_type1 = match arg {
        FnArg::Typed(pat_type) => &pat_type.ty,
        _ => panic!("expected typed argument"),
    };

    let arg2 = input_fn
        .sig
        .inputs
        .iter()
        .nth(1)
        .expect("callback must have one argument");

    let arg_type2 = match arg2 {
        FnArg::Typed(pat_type) => &pat_type.ty,
        _ => panic!("expected typed argument"),
    };

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
                    let callback_js_closure = Closure::<dyn FnMut(wasm_bindgen::JsValue, wasm_bindgen::JsValue)>::new(|val, val2| {
                        let event: #arg_type1 =
                            serde_wasm_bindgen::from_value(val);

                        if event.is_err() {
                            gloo::console::log!("Callback error: {}. Wrong argument type: {}", fn_name, event.err().unwrap());
                            return;
                        }

                        let event2: #arg_type2 =
                            serde_wasm_bindgen::from_value(val2);

                        if event2.is_err() {
                            gloo::console::log!("Callback error: {}. Wrong argument type: {}", fn_name, event2.err().unwrap());
                            return;
                        }

                        let event = event.unwrap();
                        let event2 = event.unwrap();

                        #fn_name(event, event2);
                    });

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
