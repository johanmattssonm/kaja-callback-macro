use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, LitStr};

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
    // parse attribute as either a string literal `"name"` or an identifier `name`
    struct CallbackArg {
        name: String,
        span: proc_macro2::Span,
    }

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

    let callback_arg = parse_macro_input!(attr as CallbackArg);
    let js_name_lit = LitStr::new(&callback_arg.name, callback_arg.span);
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let fn_block = &input_fn.block;

    let register_fn_name = syn::Ident::new(&format!("{}_register", fn_name), fn_name.span());
    let register_fn_name_lit = LitStr::new(&register_fn_name.to_string(), register_fn_name.span());

    let expanded = match input_fn.sig.inputs.len() {
        0 => {
            // No argument: ignore the JsValue passed from JS and call the function directly.
            quote! {
                #vis #sig #fn_block

                #[wasm_bindgen]
                pub fn #register_fn_name() {
                    use wasm_bindgen::closure::Closure;
                    use wasm_bindgen::JsCast;
                    use js_sys::{Array, Object, Reflect};
                    use wasm_bindgen::JsValue;

                    if let Some(window) = web_sys::window() {
                        let cb = Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(|_val| {
                            #fn_name();
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
        }
        _ => {
            // More than one argument is not supported by this macro.
            return TokenStream::from(quote! {
                compile_error!("#[callback(...)] only supports functions with 0 arguments");
            });
        }
    };

    TokenStream::from(expanded)
}
