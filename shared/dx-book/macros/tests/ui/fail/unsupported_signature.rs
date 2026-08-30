use storybook::variant;

#[variant(unknown = true, id = "bad--id", name = " Padded ")]
const unsafe extern "C" fn Invalid<T>(value: T, rest: ...) {
    let _ = value;
}

fn main() {}
