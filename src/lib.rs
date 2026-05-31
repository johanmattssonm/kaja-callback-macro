use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, LitStr};

/// Make sures a function is registerd in JS for a given callback in the
/// Kaja Web Framework (WebAssembly, Rust).
///
/// Example usage:
/// ```
/// #[derive(Debug, serde::Deserialize)]
/// struct SomeClickEvent {
///     índex: usize,
/// }

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
    let js_name = parse_macro_input!(attr as LitStr).value();
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let fn_block = &input_fn.block;

    // Extract single argument type
    let arg = input_fn
        .sig
        .inputs
        .first()
        .expect("callback must have one argument");

    let arg_type = match arg {
        FnArg::Typed(pat_type) => &pat_type.ty,
        _ => panic!("expected typed argument"),
    };

    let register_fn_name = syn::Ident::new(&format!("{}_register", fn_name), fn_name.span());

    let expanded = quote! {
        #vis #sig #fn_block

        pub fn #register_fn_name() {
            use wasm_bindgen::closure::Closure;
            use wasm_bindgen::JsCast;

            let window = web_sys::window();

            if window.is_none() {
                gloo::console::log!("Callback error: {}. No window.", fn_name);
                return;
            }

            let window = window.unwrap();
            let callback_js_closure = Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(|val| {
                let event: #arg_type =
                    serde_wasm_bindgen::from_value(val);

                if event.is_err() {
                    gloo::console::log!("Callback error: {}. Wrong argument type: {}", fn_name, event.err().unwrap());
                    return;
                }

                let event = event.unwrap();
                #fn_name(event);
            });

            let set = ::js_sys::Reflect::set(
                &window,
                &#js_name.into(),
                cb.as_ref().unchecked_ref(),
            );

            if set.is_err() {
                let err = set.err().unwrap();

                gloo::console::log!(r#"Callback init error: {}, failed to set property on window.
                    Needed in order to have callback available in JS land."#, err);

                return;
            }

            cb.forget();
        }

        inventory::submit! {
            CallbackRegistration {
                register: #register_fn_name
            }
        }
    };

    TokenStream::from(expanded)
}
