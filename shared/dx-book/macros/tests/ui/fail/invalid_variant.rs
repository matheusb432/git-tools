use storybook::variant;

#[variant(name = "Invalid")]
async fn InvalidVariant(value: usize) {
    let _ = value;
}

fn main() {}
