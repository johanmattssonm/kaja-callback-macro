# Callback Macro

This is a tag for annotating Rust functions that should be used as callbacks in
JavaScript using the kaja-web framework.

Example usage:
```rust,ignore
use kaja_html_macro;
use kaja_web::prelude::*;

#[derive(serde::Deserialize)]
struct SomeClickEvent {
    index: usize,
}

#[callback("someClickCallback")]
fn some_on_click_function(event: SomeClickEvent) {
    log!("event.index: {:?}", event.index);
}

// This example allows you to call the Rust function from JavaScript like this:
let html = html! {{
    <button onclick="someClickCallback({
        index: 1
    })">Run WASM Callback</button>
}};

#[wasm_bindgen(start)]
pub fn init() {
    // make the callbacks available to JavaScript
    init_callbacks(); 
}
```

## Home Page
https://kajacode.com/kajaweb.html

## Author and Contact
- Written by Johan Mattsson
- johan.mattsson.m@gmail.com
- https://kajacode.com
