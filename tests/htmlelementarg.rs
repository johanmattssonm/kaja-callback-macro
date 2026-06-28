use kaja_callback_macro::callback;
use wasm_bindgen::prelude::*;
use web_sys::HtmlElement;

pub struct InitFn(pub fn());

inventory::collect!(InitFn);

#[callback(counterComponentDisconnect)]
fn pass_html_element_in_args(element: HtmlElement, _wasm_id: u32) {
    println!("Works {}", element.id());
}

#[test]
fn it_compiles() {}
