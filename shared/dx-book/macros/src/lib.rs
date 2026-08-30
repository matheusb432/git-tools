use proc_macro::TokenStream;

mod attributes;
mod diagnostics;
mod facade;
mod metadata;
mod naming;
mod story;
mod variant;

/// Defines and registers a story from a const array of [`variant`] functions.
///
/// Required arguments use `key = value` syntax:
///
/// - `id = "stable-id"` sets the URL-safe story ID.
/// - `name = "Display name"` sets the catalog label.
///
/// `extra_docs = PATH` appends a `&'static str` Markdown constant to the const's
/// doc comments. `preview = function_name` selects a separately declared
/// variant for compact catalog cards. The annotated const must have type `()`
/// and contain `&[variant_name, ...]` (an unreferenced array is also accepted).
///
/// ```
/// use dioxus::prelude::*;
/// # use storybook as dx_book;
/// use dx_book::{story, variant};
///
/// #[variant]
/// fn default() -> Element {
///     rsx! { "Default" }
/// }
///
/// #[story(id = "example_story", name = "Example story")]
/// const EXAMPLE: () = &[default];
/// ```
#[proc_macro_attribute]
pub fn story(arguments: TokenStream, item: TokenStream) -> TokenStream {
    story::expand(arguments.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Defines a story variant from a zero-argument Dioxus render function.
///
/// The function must be a safe, synchronous Rust function with no parameters
/// or generics and an explicit `-> Element` return type. Optional
/// `id = "stable-id"` and `name = "Display name"` arguments override metadata
/// derived from the `snake_case` function name. Generated metadata constants are
/// always private; the render callback creates a distinct Dioxus component so
/// hooks belong to the variant and reset when that component is replaced.
///
/// ```
/// use dioxus::prelude::*;
/// # use storybook as dx_book;
/// use dx_book::variant;
///
/// #[variant(id = "with_count", name = "With count")]
/// fn stateful() -> Element {
///     let count = use_signal(|| 0_u32);
///     rsx! { output { "{count}" } }
/// }
/// ```
#[proc_macro_attribute]
pub fn variant(arguments: TokenStream, item: TokenStream) -> TokenStream {
    variant::expand(arguments.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
